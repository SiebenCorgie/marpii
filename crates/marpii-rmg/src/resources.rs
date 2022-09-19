use marpii::{
    ash::vk,
    context::Device,
    resources::{Buffer, CommandBufferAllocator, CommandPool, Image, SafeImageView, Sampler}, swapchain::{Swapchain, SwapchainImage}, surface::Surface,
};
use slotmap::SlotMap;
use std::sync::Arc;
use thiserror::Error;

use self::{
    descriptor::Bindless,
    res_states::{
        AnyResKey, BufferKey, ImageKey, QueueOwnership, ResBuffer, ResImage, ResSampler, SamplerKey,
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
}

pub struct Resources {
    bindless: Bindless,

    pub(crate) images: SlotMap<ImageKey, ResImage>,
    pub(crate) buffer: SlotMap<BufferKey, ResBuffer>,
    pub(crate) sampler: SlotMap<SamplerKey, ResSampler>,

    pub(crate) swapchain: Swapchain
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
            swapchain
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
                .map_err(|e| {
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
                .map_err(|e| {
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
        let handle = self.bindless.bind_sampler(sampler.clone()).map_err(|e| {
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
            .map_err(|e| {
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

    pub fn get_next_swapchain_image(&mut self) -> Result<SwapchainImage, ResourceError>{
        if let Ok(img) = self.swapchain.acquire_next_image(){
            Ok(img)
        }else{
            //try to recreate, otherwise panic.
            todo!("Recreating swapchain needed")
        }
    }

    ///Schedules swapchain image for present
    pub fn present_image(&mut self, image: SwapchainImage){
        let queue = self.swapchain.device.first_queue_for_attribute(true, false, false).unwrap(); //FIXME use track instead
        self.swapchain.present_image(image, &*queue.inner()).unwrap();
    }
}
