use marpii::{
    ash::vk,
    context::Device,
    resources::{Buffer, Image, SafeImageView, Sampler},
    surface::Surface,
    swapchain::{Swapchain, SwapchainImage},
};
use slotmap::SlotMap;
use std::sync::Arc;
use thiserror::Error;

use self::{
    descriptor::Bindless,
    res_states::{
        BufferKey, ImageKey, QueueOwnership, ResBuffer, ResImage, ResSampler, SamplerKey,
    },
};

pub(crate) mod descriptor;
pub(crate) mod res_states;

#[derive(Debug, Error)]
pub enum ResourceError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

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

    ///Marks the image for removal. Is kept alive until all executions using the image have finished.
    pub fn remove_image(&mut self, _image: ImageKey) -> Result<(), ResourceError> {
        println!("Bufferremoval");
        Ok(())
    }

    ///Marks the sampler for removal. Is kept alive until all executions using the image have finished.
    pub fn remove_sampler(&mut self, _sampler: SamplerKey) -> Result<(), ResourceError> {
        println!("Bufferremoval");
        Ok(())
    }

    ///Marks the buffer for removal. Is kept alive until all executions using the buffer have finished.
    pub fn remove_buffer(&mut self, _buffer: BufferKey) -> Result<(), ResourceError> {
        println!("Bufferremoval");
        Ok(())
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
