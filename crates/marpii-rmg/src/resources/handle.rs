use crate::resources::res_states::{BufferKey, ImageKey, SamplerKey};
use marpii::resources::{Buffer, Image, Sampler};
use std::{any::Any, marker::PhantomData, sync::Arc, fmt::{Debug, Display}};

use super::res_states::AnyResKey;

#[derive(Clone)]
pub struct ImageHandle {
    //reference to the key. The arc signals the garbage collector when we
    // dropped
    pub(crate) key: ImageKey,
    pub(crate) imgref: Arc<Image>,
}

#[derive(Clone)]
pub struct BufferHandle<T: 'static> {
    //reference to the key. The arc signals the garbage collector when we
    // dropped
    pub(crate) key: BufferKey,
    pub(crate) bufref: Arc<Buffer>,
    pub(crate) data_type: PhantomData<T>,
}

#[derive(Clone)]
pub struct SamplerHandle {
    //reference to the key. The arc signals the garbage collector when we
    // dropped
    pub(crate) key: SamplerKey,
    pub(crate) samref: Arc<Sampler>
}

pub struct AnyHandle{
    ///Keeps the atomic reference to *something* alive. Used internally to
    /// verify if there are oweners *outside* of rmg.
    #[allow(dead_code)]
    pub(crate) atomic_ref: Option<Arc<dyn Any + Send + Sync + 'static>>,
    pub(crate) key: AnyResKey
}

impl Debug for AnyHandle{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AnyHandle({:#?})", self.key)
    }
}

impl Display for AnyHandle{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AnyHandle({})", self.key)
    }
}

impl From<AnyResKey> for AnyHandle{
    fn from(res: AnyResKey) -> Self {
        AnyHandle { atomic_ref: None, key: res }
    }
}

impl From<ImageHandle> for AnyHandle{
    fn from(h: ImageHandle) -> Self {
        AnyHandle { atomic_ref: Some(h.imgref), key: h.key.into() }
    }
}

impl From<&ImageHandle> for AnyHandle{
    fn from(h: &ImageHandle) -> Self {
        AnyHandle { atomic_ref: Some(h.imgref.clone()), key: h.key.into() }
    }
}

impl<T: 'static> From<BufferHandle<T>> for AnyHandle{
    fn from(h: BufferHandle<T>) -> Self {
        AnyHandle { atomic_ref: Some(h.bufref), key: h.key.into() }
    }
}
impl<T: 'static> From<&BufferHandle<T>> for AnyHandle{
    fn from(h: &BufferHandle<T>) -> Self {
        AnyHandle { atomic_ref: Some(h.bufref.clone()), key: h.key.into() }
    }
}

impl From<SamplerHandle> for AnyHandle{
    fn from(h: SamplerHandle) -> Self {
        AnyHandle { atomic_ref: Some(h.samref), key: h.key.into() }
    }
}

impl From<&SamplerHandle> for AnyHandle{
    fn from(h: &SamplerHandle) -> Self {
        AnyHandle { atomic_ref: Some(h.samref.clone()), key: h.key.into() }
    }
}
