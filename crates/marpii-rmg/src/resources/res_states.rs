use std::{fmt::Display, sync::Arc};

use marpii::{
    ash::vk,
    resources::{Buffer, Image, ImageView, Sampler},
};

use crate::{
    track::{Guard, TrackId, Tracks},
    Resources, Rmg,
};

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

#[derive(PartialEq, Hash, Debug, Clone, Copy)]
pub enum QueueOwnership {
    Uninitialized,
    Released { src_family: u32, dst_family: u32 },
    Owned(u32),
}

impl QueueOwnership {
    pub fn is_initalised(&self) -> bool {
        self != &QueueOwnership::Uninitialized
    }

    ///If owned (not released), the queue family.
    pub fn owner(&self) -> Option<u32> {
        if let QueueOwnership::Owned(q) = self {
            Some(*q)
        } else {
            None
        }
    }
}

///Combined state of a single image.
#[allow(dead_code)]
pub struct ResImage {
    pub(crate) image: Arc<Image>,
    pub(crate) view: Arc<ImageView>,
    pub(crate) ownership: QueueOwnership,
    pub(crate) mask: vk::AccessFlags2,
    pub(crate) layout: vk::ImageLayout,

    ///Last known execution guard. None if either the resource has just been created, or all operations have finished.
    pub(crate) guard: Option<Guard>,

    ///Handle into bindless this is located at.
    pub descriptor_handle: Option<ResourceHandle>,
}

impl ResImage {
    pub fn is_sampled_image(&self) -> bool {
        self.image.desc.usage.contains(vk::ImageUsageFlags::SAMPLED)
    }
}

///Combined state of a single buffer,
#[allow(dead_code)]
pub struct ResBuffer {
    pub(crate) buffer: Arc<Buffer>,
    pub(crate) ownership: QueueOwnership,
    pub(crate) mask: vk::AccessFlags2,

    ///Some if the buffer is currently guarded by some execution. None if either the resource has just been created, or all operations have finished.
    pub(crate) guard: Option<Guard>,
    ///Handle into bindless this is located at.
    pub descriptor_handle: Option<ResourceHandle>,
}

impl ResBuffer {
    pub fn is_storage_buffer(&self) -> bool {
        self.buffer
            .desc
            .usage
            .contains(vk::BufferUsageFlags::STORAGE_BUFFER)
    }
}

#[allow(dead_code)]
pub struct ResSampler {
    pub(crate) sampler: Arc<Sampler>,
    ///Handle into bindless this is located at.
    pub descriptor_handle: Option<ResourceHandle>,
}

#[allow(dead_code)]
pub(crate) enum AnyRes {
    Image(ResImage),
    Buffer(ResBuffer),
    Sampler(Sampler),
}

#[derive(Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Debug)]
pub enum AnyResKey {
    Image(ImageKey),
    Buffer(BufferKey),
    Sampler(SamplerKey),
}

impl Display for AnyResKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnyResKey::Image(imgk) => write!(f, "AnyResKey::Image({:?})", imgk),
            AnyResKey::Buffer(bufk) => write!(f, "AnyResKey::Buffer({:?})", bufk),
            AnyResKey::Sampler(samk) => write!(f, "AnyResKey::Sampler({:?})", samk),
        }
    }
}

impl From<ImageKey> for AnyResKey {
    fn from(k: ImageKey) -> Self {
        AnyResKey::Image(k)
    }
}

impl From<BufferKey> for AnyResKey {
    fn from(k: BufferKey) -> Self {
        AnyResKey::Buffer(k)
    }
}
impl From<SamplerKey> for AnyResKey {
    fn from(k: SamplerKey) -> Self {
        AnyResKey::Sampler(k)
    }
}

impl AnyResKey {
    ///Returns the currently owning track, or none if there is no owner. In that case the resource is probably
    /// not initialised, released, or a sampler, which has no owner.
    pub fn current_owner(&self, rmg: &Rmg) -> Option<TrackId> {
        match self {
            AnyResKey::Image(imgkey) => {
                if let Some(img) = rmg.res.images.get(*imgkey) {
                    img.ownership
                        .owner()
                        .map(|qf| rmg.queue_idx_to_trackid(qf))
                        .flatten()
                } else {
                    #[cfg(feature = "logging")]
                    log::warn!("Tried to get image for invalid key");
                    None
                }
            }
            AnyResKey::Buffer(bufkey) => {
                if let Some(buf) = rmg.res.buffer.get(*bufkey) {
                    buf.ownership
                        .owner()
                        .map(|qf| rmg.queue_idx_to_trackid(qf))
                        .flatten()
                } else {
                    #[cfg(feature = "logging")]
                    log::warn!("Tried to get buffer for invalid key");
                    None
                }
            }
            AnyResKey::Sampler(_) => None,
        }
    }

    pub fn is_initialised(&self, rmg: &Rmg) -> bool {
        match self {
            AnyResKey::Image(imgkey) => {
                if let Some(img) = rmg.res.images.get(*imgkey) {
                    img.ownership.is_initalised() && (img.layout != vk::ImageLayout::UNDEFINED)
                } else {
                    false
                }
            }
            AnyResKey::Buffer(bufkey) => {
                if let Some(buf) = rmg.res.buffer.get(*bufkey) {
                    buf.ownership.is_initalised()
                } else {
                    false
                }
            }
            AnyResKey::Sampler(_) => true,
        }
    }

    ///Returns the guards value, if there is any, or 0.
    pub fn guarded_until(&self, rmg: &Rmg) -> u64 {
        match self {
            AnyResKey::Image(imgkey) => {
                if let Some(img) = rmg.res.images.get(*imgkey) {
                    img.guard.as_ref().map(|g| g.wait_value()).unwrap_or(0)
                } else {
                    0
                }
            }
            AnyResKey::Buffer(bufkey) => {
                if let Some(buf) = rmg.res.buffer.get(*bufkey) {
                    buf.guard.as_ref().map(|g| g.wait_value()).unwrap_or(0)
                } else {
                    0
                }
            }
            AnyResKey::Sampler(_) => 0,
        }
    }

    ///Returns true if either no guard is set, or if set the guard is expired.
    pub(crate) fn guard_expired(&self, res: &Resources, tracks: &Tracks) -> bool {
        match self {
            AnyResKey::Image(imgkey) => {
                if let Some(img) = res.images.get(*imgkey) {
                    img.guard.map(|g| g.expired(&tracks)).unwrap_or(true)
                } else {
                    true
                }
            }
            AnyResKey::Buffer(bufkey) => {
                if let Some(buf) = res.buffer.get(*bufkey) {
                    buf.guard.map(|g| g.expired(&tracks)).unwrap_or(true)
                } else {
                    true
                }
            }
            AnyResKey::Sampler(_) => true,
        }
    }
}
