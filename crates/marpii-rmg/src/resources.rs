use std::sync::Arc;
use marpii::{context::Device, ash::vk, resources::{SafeImageView, Image, Buffer, Sampler, CommandBufferAllocator, CommandPool}};
use slotmap::SlotMap;
use thiserror::Error;

use self::{descriptor::Bindless, res_states::{ResImage, ResBuffer, ResSampler, ImageKey, BufferKey, SamplerKey, AnyResKey}};


pub(crate) mod descriptor;
pub(crate) mod res_states;


#[derive(Debug, Error)]
pub enum ResourceError{

    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Binding a resource failed")]
    BindingFailed
}




pub struct Resources{
    bindless: Bindless,

    images: SlotMap<ImageKey, ResImage>,
    buffer: SlotMap<BufferKey, ResBuffer>,
    sampler: SlotMap<SamplerKey, ResSampler>,
}


impl Resources {
    pub fn new(device: &Arc<Device>) -> Result<Self, ResourceError>{
        let bindless = Bindless::new_default(device)?;

        Ok(Resources{
            bindless,
            buffer: SlotMap::with_key(),
            images: SlotMap::with_key(),
            sampler: SlotMap::with_key()
        })
    }

    pub fn add_image(&mut self, image: Arc<Image>, is_sampled: bool) -> Result<ImageKey, ResourceError>{

        let mut image_view_desc = image.view_all();

        let (handle, view) = if is_sampled{


            let image_view = Arc::new(image.view(&image.device, image_view_desc)?);

            let handle = self.bindless.bind_sampled_image(image_view.clone()).map_err(|e|{
                #[cfg(feature="logging")]
                log::error!("Binding sampled image failed");

                ResourceError::BindingFailed
            })?;

            (handle, image_view)
        }else{
            let image_view = Arc::new(image.view(&image.device, image_view_desc)?);

            let handle = self.bindless.bind_storage_image(image_view.clone()).map_err(|e|{
                #[cfg(feature="logging")]
                log::error!("Binding storage image failed");

                ResourceError::BindingFailed
            })?;

            (handle, image_view)
        };

        let key = self.images.insert(ResImage {
            image,
            view,
            owning_family: None,
            mask: vk::AccessFlags2::empty(),
            layout: vk::ImageLayout::PREINITIALIZED,
            guard: None,
            descriptor_handle: handle
        });

        Ok(key)
    }

    pub fn add_sampler(&mut self, sampler: Arc<Sampler>) -> Result<SamplerKey, ResourceError>{
        let handle = self.bindless.bind_sampler(sampler.clone()).map_err(|e|{
            #[cfg(feature="logging")]
            log::error!("Binding sampler failed");

            ResourceError::BindingFailed
        })?;

        let key = self.sampler.insert(ResSampler{
            descriptor_handle: handle,
            sampler,
        });

        Ok(key)
    }

    pub fn add_buffer(&mut self, buffer: Arc<Buffer>) -> Result<BufferKey, ResourceError>{
        let handle = self.bindless.bind_storage_buffer(buffer.clone()).map_err(|e|{
            #[cfg(feature="logging")]
            log::error!("Binding storage buffer failed");

            ResourceError::BindingFailed
        })?;

        let key = self.buffer.insert(ResBuffer {
            buffer,
            owning_family: None,
            mask: vk::AccessFlags2::empty(),
            guard: None,
            descriptor_handle: handle
        });

        Ok(key)
    }
}
