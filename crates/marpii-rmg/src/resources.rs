//! # Resources
//!
//! User facing resource definitions.
//!
//! TODO: Examples once this stabilises.
//!

use std::{marker::PhantomData, sync::Arc, time::Instant};
use marpii::{resources::{Buffer, Image, Sampler}, ash::vk, sync::Semaphore};
use slotmap::SlotMap;

use crate::{task::Attachment, TrackId};

slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `BufferHdl<T>` and `ImageHdl` for user facing API.
    pub struct BufferKey;
);
slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `BufferHdl<T>` and `ImageHdl` for user facing API.
    pub struct ImageKey;
);


pub(crate) mod handle;


#[derive(Clone, Copy)]
pub struct BufferHdl<T>{
    pub(crate) hdl: BufferKey,
    ty: PhantomData<T>
}

#[derive(Clone, Copy)]
pub struct ImageHdl{
    pub(crate) hdl: ImageKey,
}

///Combined state of a single image.
pub(crate) struct ResImage{
    pub(crate) image: Arc<Image>,
    pub(crate) sampler: Option<Arc<Sampler>>,
    pub(crate) owning_family: Option<u32>,
    pub(crate) mask: vk::AccessFlags,
    pub(crate) layout: vk::ImageLayout,

    ///True if this image was created as an attachment. This info is interesting for dependency tracking and cleaning up
    /// unused resources.
    is_attachment: bool,

    ///Some if the image is currently guarded by some execution.
    pub(crate) guard: Option<Guard>
}


///Combined state of a single buffer, type tagged. Note that it is valid to use a `u8` as type, which turns this buffer into a simple byte-address-buffer.
pub(crate) struct ResBuffer{
    pub(crate) buffer: Arc<Buffer>,
    pub(crate) owning_family: usize,
    pub(crate) mask: vk::AccessFlags,

    ///Some if the image is currently guarded by some execution.
    pub(crate) guard: Option<Guard>
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
pub struct Resources{
    images: SlotMap<ImageKey, ResImage>,
    buffers: SlotMap<BufferKey, ResBuffer>,
}


impl Resources {
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
