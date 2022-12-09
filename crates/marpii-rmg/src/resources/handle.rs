//! # Handles
//!
//! There are multiple levels of handles. The lowest levels are `*Key`s. This are the direct
//! handles into the [Resource](crate::resources::Resources) structure. They do not carry any context.
//!
//! The next level are `ImageHandel`, `BufferHandle` and `SamplerHandle`. They carry a reference to the actual data
//! (at the moment). They are used to detect whenever resources are not needed anymore, and when communicating
//! with the "outside".
//!
//! Around both the key and handle types the `AnyKey` and `AnyHandle` types form an abstraction that allows
//! working with somewhat anonymous resources.

use crate::resources::res_states::{BufferKey, ImageKey, SamplerKey};
use marpii::{
    ash::vk,
    resources::{BufDesc, Buffer, Image, ImageType, ImgDesc, Sampler},
    util::ImageRegion,
};
use std::{
    any::Any,
    fmt::{Debug, Display},
    marker::PhantomData,
    sync::Arc,
};

use super::res_states::AnyResKey;

#[derive(Clone)]
pub struct ImageHandle {
    //reference to the key. The arc signals the garbage collector when we
    // dropped
    pub(crate) key: ImageKey,
    pub(crate) imgref: Arc<Image>,
}

impl ImageHandle {
    pub fn format(&self) -> &vk::Format {
        &self.imgref.desc.format
    }

    pub fn usage_flags(&self) -> &vk::ImageUsageFlags {
        &self.imgref.desc.usage
    }

    pub fn extent_2d(&self) -> vk::Extent2D {
        self.imgref.extent_2d()
    }

    pub fn extent_3d(&self) -> vk::Extent3D {
        self.imgref.extent_3d()
    }

    pub fn image_type(&self) -> &ImageType {
        &self.imgref.desc.img_type
    }

    pub fn region_all(&self) -> ImageRegion {
        self.imgref.image_region()
    }

    pub fn image_desc(&self) -> &ImgDesc {
        &self.imgref.desc
    }
}

impl Debug for ImageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ImageHandle({:?})", self.key)
    }
}

#[derive(Clone)]
pub struct BufferHandle<T: 'static> {
    //reference to the key. The arc signals the garbage collector when we
    // dropped
    pub(crate) key: BufferKey,
    pub(crate) bufref: Arc<Buffer>,
    pub(crate) data_type: PhantomData<T>,
}

impl<T: 'static> BufferHandle<T> {
    ///Returns the size in bytes. If you want to know how many
    /// objects of type `T` fit in the buffer, use `count`.
    pub fn size(&self) -> u64 {
        self.bufref.desc.size
    }

    pub fn count(&self) -> usize {
        (self.bufref.desc.size / core::mem::size_of::<T>() as u64)
            .try_into()
            .unwrap()
    }

    pub fn usage_flags(&self) -> &vk::BufferUsageFlags {
        &self.bufref.desc.usage
    }

    pub fn buf_desc(&self) -> &BufDesc {
        &self.bufref.desc
    }
}

impl<T: 'static> Debug for BufferHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BufferHandle({:?})", self.key)
    }
}

#[derive(Clone)]
pub struct SamplerHandle {
    //reference to the key. The arc signals the garbage collector when we
    // dropped
    pub(crate) key: SamplerKey,
    pub(crate) samref: Arc<Sampler>,
}

impl Debug for SamplerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SamplerHandle({:?})", self.key)
    }
}

pub struct AnyHandle {
    ///Keeps the atomic reference to *something* alive. Used internally to
    /// verify if there are owners *outside* of rmg.
    #[allow(dead_code)]
    pub(crate) atomic_ref: Option<Arc<dyn Any + Send + Sync + 'static>>,
    pub(crate) key: AnyResKey,
}

impl AnyHandle {
    pub fn has_atomic_ref(&self) -> bool {
        self.atomic_ref.is_some()
    }
}

impl Debug for AnyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AnyHandle({:#?})", self.key)
    }
}

impl Display for AnyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AnyHandle({})", self.key)
    }
}

impl From<AnyResKey> for AnyHandle {
    fn from(res: AnyResKey) -> Self {
        AnyHandle {
            atomic_ref: None,
            key: res,
        }
    }
}

impl From<ImageHandle> for AnyHandle {
    fn from(h: ImageHandle) -> Self {
        AnyHandle {
            atomic_ref: Some(h.imgref),
            key: h.key.into(),
        }
    }
}

impl From<&ImageHandle> for AnyHandle {
    fn from(h: &ImageHandle) -> Self {
        AnyHandle {
            atomic_ref: Some(h.imgref.clone()),
            key: h.key.into(),
        }
    }
}

impl<T: 'static> From<BufferHandle<T>> for AnyHandle {
    fn from(h: BufferHandle<T>) -> Self {
        AnyHandle {
            atomic_ref: Some(h.bufref),
            key: h.key.into(),
        }
    }
}
impl<T: 'static> From<&BufferHandle<T>> for AnyHandle {
    fn from(h: &BufferHandle<T>) -> Self {
        AnyHandle {
            atomic_ref: Some(h.bufref.clone()),
            key: h.key.into(),
        }
    }
}

impl From<ImageKey> for AnyHandle {
    fn from(k: ImageKey) -> Self {
        AnyHandle {
            atomic_ref: None,
            key: k.into(),
        }
    }
}

impl From<&ImageKey> for AnyHandle {
    fn from(k: &ImageKey) -> Self {
        AnyHandle {
            atomic_ref: None,
            key: (*k).into(),
        }
    }
}

impl From<BufferKey> for AnyHandle {
    fn from(k: BufferKey) -> Self {
        AnyHandle {
            atomic_ref: None,
            key: k.into(),
        }
    }
}

impl From<&BufferKey> for AnyHandle {
    fn from(k: &BufferKey) -> Self {
        AnyHandle {
            atomic_ref: None,
            key: (*k).into(),
        }
    }
}

impl From<SamplerHandle> for AnyHandle {
    fn from(h: SamplerHandle) -> Self {
        AnyHandle {
            atomic_ref: Some(h.samref),
            key: h.key.into(),
        }
    }
}

impl From<&SamplerHandle> for AnyHandle {
    fn from(h: &SamplerHandle) -> Self {
        AnyHandle {
            atomic_ref: Some(h.samref.clone()),
            key: h.key.into(),
        }
    }
}
