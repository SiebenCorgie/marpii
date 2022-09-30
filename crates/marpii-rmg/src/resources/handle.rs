use crate::resources::res_states::{BufferKey, ImageKey, SamplerKey};
use marpii::resources::{Buffer, Image, Sampler};
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
