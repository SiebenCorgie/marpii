use std::sync::Arc;

use marpii::{resources::{Image, Buffer, Sampler}, ash::vk};

use crate::track::TrackId;

use super::descriptor::ResourceHandle;


///Combined state of a single image.
pub(crate) struct ResImage {
    pub(crate) image: Arc<Image>,
    pub(crate) owning_family: Option<u32>,
    pub(crate) mask: vk::AccessFlags2,
    pub(crate) layout: vk::ImageLayout,

    ///Last known execution guard.
    pub(crate) guard: Option<Guard>,

    ///Handle into bindless this is located at.
    pub descriptor_handle: ResourceHandle,
}

impl ResImage{
    pub fn is_sampled(&self) -> bool{
        match self.descriptor_handle.handle_type() {
            ResourceHandle::TYPE_SAMPLED_IMAGE => true,
            _ => false
        }
    }
}

///Combined state of a single buffer,
pub(crate) struct ResBuffer {
    pub(crate) buffer: Arc<Buffer>,
    pub(crate) owning_family: Option<u32>,
    pub(crate) mask: vk::AccessFlags2,

    ///Some if the image is currently guarded by some execution.
    pub(crate) guard: Option<Guard>,
    ///Handle into bindless this is located at.
    pub descriptor_handle: ResourceHandle,
}

pub(crate) struct ResSampler {
    sampler: Arc<Sampler>,
    ///Handle into bindless this is located at.
    pub descriptor_handle: ResourceHandle,
}


pub(crate) struct Guard{
    track: TrackId,
    value: u64
}

pub(crate) enum AnyRes{
    Image(ResImage),
    Buffer(ResBuffer),
    Sampler(Sampler)
}
