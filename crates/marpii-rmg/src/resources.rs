use marpii::{
    ash::vk,
    context::Device,
    resources::{
        BufDesc, Buffer, DescriptorSetLayout, Image, ImgDesc, PipelineLayout, SafeImageView,
        Sampler,
    },
    surface::Surface,
    swapchain::{Swapchain, SwapchainImage},
};
use slotmap::SlotMap;
use std::sync::Arc;
use thiserror::Error;

use crate::{track::Tracks, AnyResKey};

use self::{
    descriptor::{Bindless, ResourceHandle},
    res_states::{
        BufferKey, ImageKey, QueueOwnership, ResBuffer, ResImage, ResSampler, SamplerKey,
    },
};

pub(crate) mod descriptor;
pub(crate) mod res_states;
pub(crate) mod handle;

#[derive(Debug, Error)]
pub enum ResourceError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Resource already existed")]
    ResourceExists(AnyResKey),

    #[error("Resource {0:?} was already bound to {1:?}")]
    AlreadyBound(AnyResKey, ResourceHandle),

    #[error("Image has both, SAMPLED and STORAGE flags set")]
    ImageIntersectingUsageFlags,

    #[error("Image has none of SAMPLED and STORAGE flags set. Can't decide which to use")]
    ImageNoUsageFlags,

    #[error("Binding a resource failed")]
    BindingFailed,

    #[error("Failed to get new swapchain image")]
    SwapchainError,
}

pub struct Resources {
    pub(crate) bindless: Bindless,
    pub(crate) bindless_layout: Arc<PipelineLayout>,

    pub(crate) images: SlotMap<ImageKey, ResImage>,
    pub(crate) buffer: SlotMap<BufferKey, ResBuffer>,
    pub(crate) sampler: SlotMap<SamplerKey, ResSampler>,

    pub(crate) swapchain: Swapchain,
    pub(crate) last_known_surface_extent: vk::Extent2D,

    ///Keeps track of resources that are scheduled for removal.
    remove_list: Vec<AnyResKey>,
}

impl Resources {
    pub fn new(device: &Arc<Device>, surface: &Arc<Surface>) -> Result<Self, ResourceError> {
        let bindless = Bindless::new_default(device)?;
        let bindless_layout =
            Arc::new(bindless.new_pipeline_layout(Bindless::MAX_PUSH_CONSTANT_SIZE, &[]));

        let swapchain = Swapchain::builder(device, surface)?
            .with(move |b| {
                b.create_info.usage =
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST;
            })
            .build()?;

        Ok(Resources {
            bindless,
            bindless_layout,
            buffer: SlotMap::with_key(),
            images: SlotMap::with_key(),
            sampler: SlotMap::with_key(),
            swapchain,
            last_known_surface_extent: vk::Extent2D::default(),
            remove_list: Vec::with_capacity(100),
        })
    }

    ///creates an additional pipeline layout, compatible with bindless, where each additional
    /// set is bound starting at descriptor_set index 5.
    ///
    //TODO expose "bind" function?
    pub fn bindless_pipeline_layout(
        &self,
        additional_descriptor_sets: &[DescriptorSetLayout],
    ) -> Arc<PipelineLayout> {
        //TODO cache based on layout properties
        Arc::new(self.bindless
            .new_pipeline_layout(Bindless::MAX_PUSH_CONSTANT_SIZE, additional_descriptor_sets))
    }

    pub fn bindless_layout(&self) -> Arc<PipelineLayout> {
        self.bindless_layout.clone()
    }

    ///Binds the resource for use on the gpu.
    fn bind(&mut self, res: impl Into<AnyResKey>) -> Result<ResourceHandle, ResourceError> {
        let res = res.into();
        match res {
            AnyResKey::Buffer(buf) => {
                let mut buffer = self.buffer.get_mut(buf).unwrap();
                if let Some(hdl) = &buffer.descriptor_handle {
                    return Err(ResourceError::AlreadyBound(res, *hdl));
                }
                buffer.descriptor_handle = Some(
                    self.bindless
                        .bind_storage_buffer(buffer.buffer.clone())
                        .map_err(|_| ResourceError::BindingFailed)?,
                );
                Ok(buffer.descriptor_handle.unwrap())
            }
            AnyResKey::Image(img) => {
                let mut image = self.images.get_mut(img).unwrap();
                if let Some(hdl) = &image.descriptor_handle {
                    return Err(ResourceError::AlreadyBound(res, *hdl));
                }
                if image.is_sampled_image() {
                    image.descriptor_handle = Some(
                        self.bindless
                            .bind_sampled_image(image.view.clone())
                            .map_err(|_| ResourceError::BindingFailed)?,
                    );
                } else {
                    image.descriptor_handle = Some(
                        self.bindless
                            .bind_storage_image(image.view.clone())
                            .map_err(|_| ResourceError::BindingFailed)?,
                    );
                }
                Ok(image.descriptor_handle.unwrap())
            }
            AnyResKey::Sampler(sam) => {
                let mut sampler = self.sampler.get_mut(sam).unwrap();
                if let Some(hdl) = &sampler.descriptor_handle {
                    return Err(ResourceError::AlreadyBound(res, *hdl));
                }
                sampler.descriptor_handle = Some(
                    self.bindless
                        .bind_sampler(sampler.sampler.clone())
                        .map_err(|_| ResourceError::BindingFailed)?,
                );
                Ok(sampler.descriptor_handle.unwrap())
            }
        }
    }

    pub fn add_image(&mut self, image: Arc<Image>) -> Result<ImageKey, ResourceError> {
        let image_view_desc = image.view_all();

        let image_view = Arc::new(image.view(&image.device, image_view_desc)?);

        let key = self.images.insert(ResImage {
            image,
            view: image_view,
            ownership: QueueOwnership::Uninitialized,
            mask: vk::AccessFlags2::empty(),
            layout: vk::ImageLayout::UNDEFINED,
            guard: None,
            descriptor_handle: None,
        });

        Ok(key)
    }

    pub fn add_sampler(&mut self, sampler: Arc<Sampler>) -> Result<SamplerKey, ResourceError> {
        let key = self.sampler.insert(ResSampler {
            descriptor_handle: None,
            sampler,
        });

        Ok(key)
    }

    pub fn add_buffer(&mut self, buffer: Arc<Buffer>) -> Result<BufferKey, ResourceError> {
        let key = self.buffer.insert(ResBuffer {
            buffer,
            ownership: QueueOwnership::Uninitialized,
            mask: vk::AccessFlags2::empty(),
            guard: None,
            descriptor_handle: None,
        });

        Ok(key)
    }

    ///Marks the resource for removal. Is kept alive until all executions using the image have finished.
    pub fn remove_resource(&mut self, res: impl Into<AnyResKey>) -> Result<(), ResourceError> {
        self.remove_list.push(res.into());
        Ok(())
    }

    ///Tries to get the resource's bindless handle. If not already bound, tries to bind the resource
    pub fn get_resource_handle(
        &mut self,
        res: impl Into<AnyResKey>,
    ) -> Result<ResourceHandle, ResourceError> {
        let res = res.into();
        let hdl = match res {
            AnyResKey::Buffer(buf) => self.buffer.get(buf).unwrap().descriptor_handle,
            AnyResKey::Image(img) => self.images.get(img).unwrap().descriptor_handle,
            AnyResKey::Sampler(sam) => self.sampler.get(sam).unwrap().descriptor_handle,
        };

        if let Some(hdl) = hdl {
            return Ok(hdl);
        } else {
            //have to bind, try that
            Ok(self.bind(res)?)
        }
    }

    ///Tick the resource manager that a new frame has started
    //TODO: Currently we use the rendering frame to do all the cleanup. In a perfect world we'd use
    //      another thread for that to not stall the recording process
    pub(crate) fn tick_record(&mut self, tracks: &Tracks) {

        //now check all resources that are marked for removal if they can be dropped.
        let remove_mask = self
            .remove_list
            .iter()
            .map(|k| {
                let is = k.guard_expired(&self, tracks);
                is
            })
            .collect::<Vec<_>>(); //FIXME: its late :(
        for (idx, is_removable) in remove_mask.into_iter().enumerate().rev() {
            if is_removable {
                let res = self.remove_list.remove(idx);
                #[cfg(feature = "logging")]
                log::trace!("Dropping {:?}", res);

                //remove from bindless and the key-value-store

                match res {
                    AnyResKey::Image(img) => {
                        if let Some(image) = self.images.remove(img) {
                            //If bound, unbind
                            if let Some(hdl) = image.descriptor_handle {
                                if image.is_sampled_image() {
                                    self.bindless.remove_sampled_image(hdl);
                                } else {
                                    self.bindless.remove_storage_image(hdl);
                                }
                            }
                        } else {
                            #[cfg(feature = "logging")]
                            log::error!("Tried removing {:?}, but was already removed", img)
                        }
                    }
                    AnyResKey::Buffer(buf) => {
                        if let Some(buffer) = self.buffer.remove(buf) {
                            if let Some(hdl) = buffer.descriptor_handle {
                                self.bindless.remove_storage_buffer(hdl);
                            }
                        } else {
                            #[cfg(feature = "logging")]
                            log::error!("Tried removing {:?}, but was already removed", buf)
                        }
                    }
                    AnyResKey::Sampler(sam) => {
                        if let Some(sampler) = self.sampler.remove(sam) {
                            if let Some(hdl) = sampler.descriptor_handle {
                                self.bindless.remove_sampler(hdl);
                            }
                        } else {
                            #[cfg(feature = "logging")]
                            log::error!("Tried removing {:?}, but was already removed", sam)
                        }
                    }
                }
            }
        }
    }

    pub fn get_image_desc(&self, hdl: ImageKey) -> Option<&ImgDesc> {
        self.images.get(hdl).map(|img| &img.image.desc)
    }

    pub fn get_buffer_desc(&self, hdl: BufferKey) -> Option<&BufDesc> {
        self.buffer.get(hdl).map(|buf| &buf.buffer.desc)
    }

    ///Returns the current state of the given image.
    ///
    /// # Safety
    /// If a the state gets changed in a command buffer, make sure that the final state is the
    /// same as the initial state reported by this function. Otherwise scheduling might produce a
    /// wrong value.
    pub fn get_image_state(&self, hdl: ImageKey) -> Option<&ResImage>{
        self.images.get(hdl)
    }

    ///Returns the current state of the given buffer.
    ///
    /// # Safety
    /// If a the state gets changed in a command buffer, make sure that the final state is the
    /// same as the initial state reported by this function. Otherwise scheduling might produce a
    /// wrong value.
    pub fn get_buffer_state(&self, hdl: BufferKey) -> Option<&ResBuffer>{
        self.buffer.get(hdl)
    }
    ///Returns the current state of the given sampler.
    ///
    /// # Safety
    /// If a the state gets changed in a command buffer, make sure that the final state is the
    /// same as the initial state reported by this function. Otherwise scheduling might produce a
    /// wrong value.
    pub fn get_sampler_state(&self, hdl: SamplerKey) -> Option<&ResSampler>{
        self.sampler.get(hdl)
    }

    pub fn get_next_swapchain_image(&mut self) -> Result<SwapchainImage, ResourceError> {
        let surface_extent = self
            .swapchain
            .surface
            .get_current_extent(&self.swapchain.device.physical_device)
            .unwrap_or(self.last_known_surface_extent);
        if self.swapchain.images[0].extent_2d() != surface_extent {
            #[cfg(feature = "logging")]
            log::info!("Recreating swapchain with extent {:?}!", surface_extent);

            self.swapchain.recreate(surface_extent)?;
            self.last_known_surface_extent = vk::Extent2D {
                width: self.swapchain.images[0].desc.extent.width,
                height: self.swapchain.images[0].desc.extent.height,
            };
        }

        if let Ok(img) = self.swapchain.acquire_next_image() {
            Ok(img)
        } else {
            //try to recreate, otherwise panic.
            #[cfg(feature = "logging")]
            log::info!("Failed to get new swapchain image!");
            return Err(ResourceError::SwapchainError);
        }
    }

    pub fn get_surface_extent(&self) -> vk::Extent2D {
        self.last_known_surface_extent
    }

    ///Schedules swapchain image for present
    pub fn present_image(&mut self, image: SwapchainImage) {
        let queue = self
            .swapchain
            .device
            .first_queue_for_attribute(true, false, false)
            .unwrap(); //FIXME use track instead
        if let Err(e) = self.swapchain.present_image(image, &*queue.inner()) {
            #[cfg(feature = "logging")]
            log::error!("present failed with: {}, recreating swapchain", e);
        }
    }
}
