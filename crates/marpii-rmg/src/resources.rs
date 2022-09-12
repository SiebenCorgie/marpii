//! # Resources
//!
//! User facing resource definitions.
//!
//! TODO: Examples once this stabilises.
//!

use std::{marker::PhantomData, sync::Arc, time::Instant};
use marpii::{resources::{Buffer, Image, Sampler, BufDesc}, ash::vk::{self, PhysicalDeviceDepthStencilResolvePropertiesKHR}, sync::Semaphore, context::Ctx, allocator::{Allocator, MemoryUsage}};
use slotmap::SlotMap;

use crate::{task::Attachment, TrackId};

slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `BufferHdl<T>` and `ImageHdl` for user facing API.
    pub struct BufferKey;
);

impl<T: 'static> From<BufferHdl<T>> for BufferKey{
    fn from(key: BufferHdl<T>) -> Self {
        key.hdl
    }
}

slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `BufferHdl<T>` and `ImageHdl` for user facing API.
    pub struct ImageKey;
);


impl From<ImageHdl> for ImageKey{
    fn from(key: ImageHdl) -> Self {
        key.hdl
    }
}

slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `SamplerHdl` user facing API.
    pub struct SamplerKey;
);

impl From<SamplerHdl> for SamplerKey{
    fn from(key: SamplerHdl) -> Self {
        key.hdl
    }
}


pub(crate) mod handle;


#[derive(Clone, Copy)]
pub struct BufferHdl<T: 'static>{
    pub(crate) hdl: BufferKey,
    ty: PhantomData<T>
}

#[derive(Clone, Copy)]
pub struct ImageHdl{
    pub(crate) hdl: ImageKey,
}

#[derive(Clone, Copy)]
pub struct SamplerHdl{
    pub(crate) hdl: SamplerKey,
}


//TODO: We might be able to remove the Arc's around the resources...
///Combined state of a single image.
pub(crate) struct ResImage{
    pub(crate) image: Arc<Image>,
    pub(crate) sampler: Option<SamplerHdl>,
    pub(crate) owning_family: Option<u32>,
    pub(crate) mask: vk::AccessFlags2,
    pub(crate) layout: vk::ImageLayout,

    ///Some if the image is currently guarded by some execution.
    pub(crate) guard: Option<Guard>
}


///Combined state of a single buffer, type tagged. Note that it is valid to use a `u8` as type, which turns this buffer into a simple byte-address-buffer.
pub(crate) struct ResBuffer{
    pub(crate) buffer: Arc<Buffer>,
    pub(crate) owning_family: Option<u32>,
    pub(crate) mask: vk::AccessFlags2,

    ///Some if the image is currently guarded by some execution.
    pub(crate) guard: Option<Guard>
}

pub(crate) struct ResSampler{
    sampler: Arc<Sampler>
}

///Guard for some execution. Can be polled if the execution has finished or not.
pub(crate) struct Guard{
    ///The track that currently guards
    pub(crate) track: TrackId,
    ///The target semaphore value that needs to be reached on the track to free the guarded value.
    pub(crate) target_val: u64,
}



struct AttachmentReg{
    last_use: Instant,
    key: ImageKey,

}

///Resource handler. TODO Document.
// FIXME: Use [Synchronization2](https://www.khronos.org/blog/vulkan-sdk-offers-developers-a-smooth-transition-path-to-synchronization2)
//        For Scheduling
pub(crate) struct Resources{
    images: SlotMap<ImageKey, ResImage>,
    buffers: SlotMap<BufferKey, ResBuffer>,
    sampler: SlotMap<SamplerKey, ResSampler>,
}


impl Resources {
    pub(crate) fn new() -> Self{
        Resources {
            images: SlotMap::with_key(),
            buffers: SlotMap::with_key(),
            sampler: SlotMap::with_key(),
        }
    }

    pub(crate) fn new_buffer<T: 'static>(&mut self, buffer: Arc<Buffer>) -> BufferHdl<T>{

        let buffer = ResBuffer{
            buffer,
            owning_family: None,
            mask: vk::AccessFlags2::empty(),
            guard: None,
        };

        let key = self.buffers.insert(buffer);

        BufferHdl { hdl: key, ty: PhantomData }
    }


    pub(crate) fn new_image(&mut self, image: Arc<Image>, sampler: Option<SamplerHdl>) -> ImageHdl{

        let image = ResImage{
            image,
            sampler,
            owning_family: None,
            mask: vk::AccessFlags2::empty(),
            layout: vk::ImageLayout::UNDEFINED,
            guard: None
        };

        let key = self.images.insert(image);

        ImageHdl { hdl: key }
    }

    pub(crate) fn new_sampler(&mut self, sampler: Arc<Sampler>) -> SamplerHdl{
        let sampler = ResSampler{
            sampler
        };

        let key = self.sampler.insert(sampler);
        SamplerHdl{hdl: key}
    }

    pub(crate) fn get_image(&self, image: ImageKey) -> Option<&ResImage> {
        self.images.get(image)
    }

    pub(crate) fn get_buffer(&self, buffer: BufferKey) -> Option<&ResBuffer> {
        self.buffers.get(buffer)
    }

    pub(crate) fn get_image_mut(&mut self, image: ImageKey) -> Option<&mut ResImage> {
        self.images.get_mut(image)
    }

    pub(crate) fn get_buffer_mut(&mut self, buffer: BufferKey) -> Option<&mut ResBuffer> {
        self.buffers.get_mut(buffer)
    }

    ///Registers a new image attachment with the given properties.
    pub(crate) fn register_attachment(&mut self, attachment: &Attachment) -> ImageKey{
        //check all attachments for an unused one with the properties. While iterating drop all that are
        // unused
        todo!();
    }
}
