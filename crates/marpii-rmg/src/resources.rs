use marpii::{
    ash::vk,
    context::Device,
    resources::{Buffer, Image, SafeImageView, Sampler},
    surface::Surface,
    swapchain::{Swapchain, SwapchainImage},
};
use slotmap::SlotMap;
use std::{sync::Arc, collections::{BTreeMap, LinkedList}};
use thiserror::Error;

use crate::{AnyResKey, CtxRmg, track::Tracks};

use self::{
    descriptor::Bindless,
    res_states::{
        BufferKey, ImageKey, QueueOwnership, ResBuffer, ResImage, ResSampler, SamplerKey,
    }, temporary::TempResources,
};

pub(crate) mod descriptor;
pub(crate) mod res_states;
mod temporary;

#[derive(Debug, Error)]
pub enum ResourceError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Resource already existed")]
    ResourceExists(AnyResKey),

    #[error("Binding a resource failed")]
    BindingFailed,

    #[error("Failed to get new swapchain image")]
    SwapchainError,
}

pub struct Resources {
    bindless: Bindless,

    pub(crate) images: SlotMap<ImageKey, ResImage>,
    pub(crate) buffer: SlotMap<BufferKey, ResBuffer>,
    pub(crate) sampler: SlotMap<SamplerKey, ResSampler>,

    pub(crate) swapchain: Swapchain,
    pub(crate) last_known_surface_extent: vk::Extent2D,

    ///Handles tracking of temporary resources. Basically a simple garbage collector.
    temporary_resources: TempResources,

    ///Keeps track of resources that are scheduled for removal.
    remove_list: Vec<AnyResKey>,
}

impl Resources {
    pub fn new(device: &Arc<Device>, surface: &Arc<Surface>) -> Result<Self, ResourceError> {
        let bindless = Bindless::new_default(device)?;

        let swapchain = Swapchain::builder(device, surface)?
            .with(move |b| {
                b.create_info.usage =
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST
            })
            .build()?;

        Ok(Resources {
            bindless,
            buffer: SlotMap::with_key(),
            images: SlotMap::with_key(),
            sampler: SlotMap::with_key(),
            swapchain,
            last_known_surface_extent: vk::Extent2D::default(),
            temporary_resources: TempResources::new(),
            remove_list: Vec::with_capacity(100),
        })
    }

    pub fn add_image(
        &mut self,
        image: Arc<Image>,
        is_sampled: bool,
    ) -> Result<ImageKey, ResourceError> {
        let image_view_desc = image.view_all();

        let (handle, view) = if is_sampled {
            let image_view = Arc::new(image.view(&image.device, image_view_desc)?);

            let handle = self
                .bindless
                .bind_sampled_image(image_view.clone())
                .map_err(|_e| {
                    #[cfg(feature = "logging")]
                    log::error!("Binding sampled image failed");

                    ResourceError::BindingFailed
                })?;

            (handle, image_view)
        } else {
            let image_view = Arc::new(image.view(&image.device, image_view_desc)?);

            let handle = self
                .bindless
                .bind_storage_image(image_view.clone())
                .map_err(|_e| {
                    #[cfg(feature = "logging")]
                    log::error!("Binding storage image failed");

                    ResourceError::BindingFailed
                })?;

            (handle, image_view)
        };

        let key = self.images.insert(ResImage {
            image,
            view,
            ownership: QueueOwnership::Uninitialized,
            mask: vk::AccessFlags2::empty(),
            layout: vk::ImageLayout::UNDEFINED,
            guard: None,
            descriptor_handle: handle,
        });

        Ok(key)
    }

    pub fn add_sampler(&mut self, sampler: Arc<Sampler>) -> Result<SamplerKey, ResourceError> {
        let handle = self.bindless.bind_sampler(sampler.clone()).map_err(|_e| {
            #[cfg(feature = "logging")]
            log::error!("Binding sampler failed");

            ResourceError::BindingFailed
        })?;

        let key = self.sampler.insert(ResSampler {
            descriptor_handle: handle,
            sampler,
        });

        Ok(key)
    }

    pub fn add_buffer(&mut self, buffer: Arc<Buffer>) -> Result<BufferKey, ResourceError> {
        let handle = self
            .bindless
            .bind_storage_buffer(buffer.clone())
            .map_err(|_e| {
                #[cfg(feature = "logging")]
                log::error!("Binding storage buffer failed");

                ResourceError::BindingFailed
            })?;

        let key = self.buffer.insert(ResBuffer {
            buffer,
            ownership: QueueOwnership::Uninitialized,
            mask: vk::AccessFlags2::empty(),
            guard: None,
            descriptor_handle: handle,
        });

        Ok(key)
    }

    ///Marks the resource for removal. Is kept alive until all executions using the image have finished.
    pub fn remove_resource(&mut self, res: impl Into<AnyResKey>) -> Result<(), ResourceError> {
        self.remove_list.push(res.into());
        Ok(())
    }

    ///Tick the resource manager that a new frame has started
    //TODO: Currently we use the rendering frame to do all the cleanup. In a perfect world we'd use
    //      another thread for that to not stall the recording process
    pub(crate) fn tick_record(&mut self, tracks: &Tracks){
        //checkout the removals
        self.temporary_resources.tick(&mut self.remove_list);

        //now check all resources that are marked for removal if they can be dropped.
        let remove_mask = self.remove_list.iter().map(|k| k.guard_expired(&self, tracks)).collect::<Vec<_>>(); //FIXME: its late :(
        for (idx, is_removable) in remove_mask.into_iter().enumerate().rev(){
            if is_removable{
                let res = self.remove_list.remove(idx);
                #[cfg(feature="logging")]
                log::trace!("Dropping {:?}", res);
                match res {
                    AnyResKey::Image(img) => if self.images.remove(img).is_none(){
                        #[cfg(feature="logging")]
                        log::error!("Tried removing {:?}, but was already removed", img)
                    }
                    AnyResKey::Buffer(buf) => if self.buffer.remove(buf).is_none(){
                        #[cfg(feature="logging")]
                        log::error!("Tried removing {:?}, but was already removed", buf)
                    }
                    AnyResKey::Sampler(sam) => if self.sampler.remove(sam).is_none(){
                        #[cfg(feature="logging")]
                        log::error!("Tried removing {:?}, but was already removed", sam)
                    }
                }
            }
        }
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

    ///Schedules swapchain image for present
    pub fn present_image(&mut self, image: SwapchainImage) {
        let queue = self
            .swapchain
            .device
            .first_queue_for_attribute(true, false, false)
            .unwrap(); //FIXME use track instead
        self.swapchain
            .present_image(image, &*queue.inner())
            .unwrap();
    }
}
