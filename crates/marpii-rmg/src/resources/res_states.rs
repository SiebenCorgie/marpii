use std::sync::Arc;

use marpii::{resources::{Image, Buffer, Sampler, ImageView}, ash::vk};

use crate::track::TrackId;

use super::descriptor::ResourceHandle;


slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `BufferHdl<T>` and `ImageHdl` for user facing API.
    pub struct BufferKey;
);
slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `BufferHdl<T>` and `ImageHdl` for user facing API.
    pub struct ImageKey;
);
slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `SamplerHdl` user facing API.
    pub struct SamplerKey;
);



///Combined state of a single image.
pub struct ResImage {
    pub(crate) image: Arc<Image>,
    pub(crate) view: Arc<ImageView>,
    pub(crate) owning_family: Option<u32>,
    pub(crate) mask: vk::AccessFlags2,
    pub(crate) layout: vk::ImageLayout,

    ///Last known execution guard. None if either the resource has just been created, or all operations have finished.
    pub(crate) guard: Option<Guard>,

    ///Handle into bindless this is located at.
    pub descriptor_handle: ResourceHandle,
}

impl ResImage{
    pub fn is_sampled_image(&self) -> bool{
        self.descriptor_handle.handle_type() == ResourceHandle::TYPE_SAMPLED_IMAGE
    }
}

///Combined state of a single buffer,
pub struct ResBuffer {
    pub(crate) buffer: Arc<Buffer>,
    pub(crate) owning_family: Option<u32>,
    pub(crate) mask: vk::AccessFlags2,

    ///Some if the buffer is currently guarded by some execution. None if either the resource has just been created, or all operations have finished.
    pub(crate) guard: Option<Guard>,
    ///Handle into bindless this is located at.
    pub descriptor_handle: ResourceHandle,
}

impl ResBuffer {
    pub fn is_storage_buffer(&self) -> bool{
         self.descriptor_handle.handle_type() == ResourceHandle::TYPE_STORAGE_BUFFER
    }
}

pub struct ResSampler {
    pub(crate) sampler: Arc<Sampler>,
    ///Handle into bindless this is located at.
    pub descriptor_handle: ResourceHandle,
}


pub(crate) struct Guard{
    pub track: TrackId,
    pub target_value: u64
}

pub(crate) enum AnyRes{
    Image(ResImage),
    Buffer(ResBuffer),
    Sampler(Sampler)
}
