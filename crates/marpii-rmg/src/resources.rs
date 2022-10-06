use crossbeam_channel::{Receiver, Sender};
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
use std::{marker::PhantomData, sync::Arc};
use thiserror::Error;

use crate::{
    resources::{
        descriptor::{Bindless, ResourceHandle},
        res_states::{
            BufferKey, ImageKey, QueueOwnership, ResBuffer, ResImage, ResSampler, SamplerKey,
        },
    },
    track::Tracks,
    BufferHandle, ImageHandle, SamplerHandle,
};

use self::{handle::AnyHandle, res_states::AnyResKey};

pub(crate) mod descriptor;
pub(crate) mod handle;
pub(crate) mod res_states;

#[derive(Debug, Error)]
pub enum ResourceError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Resource already existed")]
    ResourceExists(AnyHandle),

    #[error("Resource {0:?} was already bound to {1:?}")]
    AlreadyBound(AnyHandle, ResourceHandle),

    #[error("Image has both, SAMPLED and STORAGE flags set")]
    ImageIntersectingUsageFlags,

    #[error("Image has none of SAMPLED and STORAGE flags set. Can't decide which to use")]
    ImageNoUsageFlags,

    #[error("Binding a resource failed")]
    BindingFailed,

    #[error("Failed to get new swapchain image")]
    SwapchainError,

    #[error("There is no Track for queue family {0}")]
    NoTrackForQueueFamily(u32),
}

pub struct Resources {
    pub(crate) bindless: Bindless,
    pub(crate) bindless_layout: Arc<PipelineLayout>,

    pub(crate) images: SlotMap<ImageKey, ResImage>,
    pub(crate) buffer: SlotMap<BufferKey, ResBuffer>,
    pub(crate) sampler: SlotMap<SamplerKey, ResSampler>,

    pub(crate) swapchain: Swapchain,
    pub(crate) last_known_surface_extent: vk::Extent2D,

    ///Channel used by the handles to signal their drop.
    #[allow(dead_code)]
    pub(crate) handle_drop_channel: (Sender<AnyResKey>, Receiver<AnyResKey>),
}

impl Resources {
    pub fn new(device: &Arc<Device>, surface: &Arc<Surface>) -> Result<Self, ResourceError> {
        let bindless = Bindless::new_default(device)?;
        let bindless_layout =
            Arc::new(bindless.new_pipeline_layout(&[]));

        let swapchain = Swapchain::builder(device, surface)?
            .with(move |b| {
                b.create_info.usage =
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST;
            })
            .build()?;

        let handle_drop_channel = crossbeam_channel::unbounded();

        Ok(Resources {
            bindless,
            bindless_layout,
            buffer: SlotMap::with_key(),
            images: SlotMap::with_key(),
            sampler: SlotMap::with_key(),
            swapchain,
            last_known_surface_extent: vk::Extent2D::default(),
            handle_drop_channel,
        })
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
                    return Err(ResourceError::AlreadyBound(res.into(), *hdl));
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
                    return Err(ResourceError::AlreadyBound(res.into(), *hdl));
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
                    return Err(ResourceError::AlreadyBound(res.into(), *hdl));
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

    ///Adds an image, assuming it is uninitialised. If the image is initialised, owned by a queue or similar,
    /// use the [import](Self::import_image) function instead.
    pub fn add_image(&mut self, image: Arc<Image>) -> Result<ImageHandle, ResourceError> {
        let image_view_desc = image.view_all();

        let image_view = Arc::new(image.view(&image.device, image_view_desc)?);

        let key = self.images.insert(ResImage {
            image: image.clone(),
            view: image_view,
            ownership: QueueOwnership::Uninitialized,
            mask: vk::AccessFlags2::empty(),
            layout: vk::ImageLayout::UNDEFINED,
            guard: None,
            descriptor_handle: None,
        });

        Ok(ImageHandle { key, imgref: image })
    }

    pub fn add_sampler(&mut self, sampler: Arc<Sampler>) -> Result<SamplerHandle, ResourceError> {
        let key = self.sampler.insert(ResSampler {
            descriptor_handle: None,
            sampler: sampler.clone(),
        });

        Ok(SamplerHandle {
            key,
            samref: sampler,
        })
    }

    ///Adds an buffer, assuming it is uninitialised. If the buffer is initialised, owned by a queue or similar,
    /// use the [import](Rmg::import_buffer) function instead.
    pub fn add_buffer<T: 'static>(
        &mut self,
        buffer: Arc<Buffer>,
    ) -> Result<BufferHandle<T>, ResourceError> {
        let key = self.buffer.insert(ResBuffer {
            buffer: buffer.clone(),
            ownership: QueueOwnership::Uninitialized,
            mask: vk::AccessFlags2::empty(),
            guard: None,
            descriptor_handle: None,
        });

        Ok(BufferHandle {
            key,
            bufref: buffer,
            data_type: PhantomData,
        })
    }

    ///Imports the buffer with the given state. Returns an error if a given queue_family index has no internal TrackId.
    pub(crate) fn import_buffer<T: 'static>(
        &mut self,
        tracks: &Tracks,
        buffer: Arc<Buffer>,
        queue_family: Option<u32>,
        access_flags: Option<vk::AccessFlags2>,
    ) -> Result<BufferHandle<T>, ResourceError> {
        let owner = if let Some(fam) = queue_family {
            let track = tracks.0.iter().find_map(|(track_id, track)| {
                if track.queue_idx == fam {
                    Some(track_id)
                } else {
                    None
                }
            });
            if let Some(_t) = track {
                QueueOwnership::Owned(fam)
            } else {
                return Err(ResourceError::NoTrackForQueueFamily(fam));
            }
        } else {
            QueueOwnership::Uninitialized
        };

        let access = access_flags.unwrap_or(vk::AccessFlags2::NONE);

        let key = self.buffer.insert(ResBuffer {
            buffer: buffer.clone(),
            ownership: owner,
            mask: access,
            guard: None,
            descriptor_handle: None,
        });

        Ok(BufferHandle {
            key,
            bufref: buffer,
            data_type: PhantomData,
        })
    }

    ///Tries to get the resource's bindless handle. If not already bound, tries to bind the resource
    pub fn get_resource_handle(
        &mut self,
        res: impl Into<AnyHandle>,
    ) -> Result<ResourceHandle, ResourceError> {
        let res = res.into();
        let hdl = match res.key {
            AnyResKey::Buffer(buf) => self.buffer.get(buf).unwrap().descriptor_handle,
            AnyResKey::Image(img) => self.images.get(img).unwrap().descriptor_handle,
            AnyResKey::Sampler(sam) => self.sampler.get(sam).unwrap().descriptor_handle,
        };

        if let Some(hdl) = hdl {
            return Ok(hdl);
        } else {
            //have to bind, try that
            Ok(self.bind(res.key)?)
        }
    }

    ///Tick the resource manager that a new frame has started
    //TODO: Currently we use the rendering frame to do all the cleanup. In a perfect world we'd use
    //      another thread for that to not stall the recording process
    pub(crate) fn tick_record(&mut self, tracks: &Tracks) {
        self.images.retain(|key, img| {
            if img.is_orphaned() && img.guard.map(|g| g.expired(tracks)).unwrap_or(true) {
                #[cfg(feature = "logging")]
                log::info!("Dropping {:?}", key);

                if let Some(hdl) = img.descriptor_handle {
                    if img.is_sampled_image() {
                        self.bindless.remove_sampled_image(hdl);
                    } else {
                        self.bindless.remove_storage_image(hdl);
                    }
                }
                false
            } else {
                true
            }
        });

        self.buffer.retain(|key, buffer| {
            if buffer.is_orphaned() && buffer.guard.map(|g| g.expired(tracks)).unwrap_or(true) {
                #[cfg(feature = "logging")]
                log::info!("Dropping {:?}", key);

                if let Some(hdl) = buffer.descriptor_handle {
                    self.bindless.remove_storage_buffer(hdl);
                }
                false
            } else {
                true
            }
        });

        self.sampler.retain(|key, sampler| {
            if sampler.is_orphaned() {
                #[cfg(feature = "logging")]
                log::info!("Dropping {:?}", key);

                if let Some(hdl) = sampler.descriptor_handle {
                    self.bindless.remove_sampler(hdl);
                }
                false
            } else {
                true
            }
        });
    }

    pub fn get_image_desc(&self, hdl: &ImageHandle) -> &ImgDesc {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        &self
            .images
            .get(hdl.key)
            .as_ref()
            .expect("Used invalid image handle")
            .image
            .desc
    }

    pub fn get_buffer_desc<T: 'static>(&self, hdl: &BufferHandle<T>) -> &BufDesc {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        &self
            .buffer
            .get(hdl.key)
            .as_ref()
            .expect("Used invalid buffer handle")
            .buffer
            .desc
    }

    ///Returns the current state of the given image.
    ///
    /// # Safety
    /// If a the state gets changed in a command buffer, make sure that the final state is the
    /// same as the initial state reported by this function. Otherwise scheduling might produce a
    /// wrong value.
    pub fn get_image_state(&self, hdl: &ImageHandle) -> &ResImage {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        self.images
            .get(hdl.key)
            .as_ref()
            .expect("Used invalid ImageHandle")
    }

    ///Returns the current state of the given buffer.
    ///
    /// # Safety
    /// If a the state gets changed in a command buffer, make sure that the final state is the
    /// same as the initial state reported by this function. Otherwise scheduling might produce a
    /// wrong value.
    pub fn get_buffer_state<T: 'static>(&self, hdl: &BufferHandle<T>) -> &ResBuffer {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        self.buffer
            .get(hdl.key)
            .as_ref()
            .expect(&format!("Used invalid BufferHandle {:?}", hdl.key))
    }
    ///Returns the current state of the given sampler.
    ///
    /// # Safety
    /// If a the state gets changed in a command buffer, make sure that the final state is the
    /// same as the initial state reported by this function. Otherwise scheduling might produce a
    /// wrong value.
    pub fn get_sampler_state(&self, hdl: &SamplerHandle) -> &ResSampler {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        self.sampler
            .get(hdl.key)
            .as_ref()
            .expect("Used invalid Sampler Handle")
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
