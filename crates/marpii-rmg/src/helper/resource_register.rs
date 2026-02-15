use ahash::{AHashMap, AHashSet};
use marpii::ash::vk;

use crate::{
    BufferHandle, ImageHandle, ResourceRegistry, SamplerHandle, resources::handle::TypeErased,
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct ImageState {
    stage: vk::PipelineStageFlags2,
    access: vk::AccessFlags2,
    layout: vk::ImageLayout,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct BufferState {
    stage: vk::PipelineStageFlags2,
    access: vk::AccessFlags2,
}

///Helper whenever you are using _a-lot_ of resources at once.
///
/// uses a special internal fast-path to register the resources.
pub struct ResourceRegister {
    pub(crate) images: AHashMap<ImageHandle, ImageState>,
    pub(crate) buffers: AHashMap<BufferHandle<TypeErased>, BufferState>,
    pub(crate) samplers: AHashSet<SamplerHandle>,
}

impl ResourceRegister {
    pub fn new() -> Self {
        ResourceRegister {
            images: AHashMap::default(),
            buffers: AHashMap::default(),
            samplers: AHashSet::default(),
        }
    }

    ///Notifies that the register of the given images usage at a given stage and layout.
    ///
    /// If the same image was already register, returns the _old_ state it was registered at.
    pub fn register_image(
        &mut self,
        image: ImageHandle,
        stage: vk::PipelineStageFlags2,
        access: vk::AccessFlags2,
        layout: vk::ImageLayout,
    ) -> Option<(vk::PipelineStageFlags2, vk::AccessFlags2, vk::ImageLayout)> {
        if let Some(ImageState {
            stage,
            access,
            layout,
        }) = self.images.insert(
            image,
            ImageState {
                stage,
                access,
                layout,
            },
        ) {
            Some((stage, access, layout))
        } else {
            None
        }
    }

    ///Notifies that the register of the given buffer usage at a given stage.
    ///
    /// If the same buffer was already register, returns the _old_ state it was registered at.
    pub fn register_buffer<T: 'static>(
        &mut self,
        buffer: BufferHandle<T>,
        stage: vk::PipelineStageFlags2,
        access: vk::AccessFlags2,
    ) -> Option<(vk::PipelineStageFlags2, vk::AccessFlags2)> {
        if let Some(BufferState { stage, access }) = self
            .buffers
            .insert(buffer.type_erase(), BufferState { stage, access })
        {
            Some((stage, access))
        } else {
            None
        }
    }

    ///Notifies that the register of the given sampler. Returns true, if the sampler was already
    /// registered before
    pub fn register_sampler(&mut self, sampler: SamplerHandle) -> bool {
        !self.samplers.insert(sampler)
    }

    ///Resets the whole registry.
    pub fn reset(&mut self) {
        self.images.clear();
        self.buffers.clear();
        self.samplers.clear();
    }

    ///Registers all resources in `self` with `registry`
    pub fn register_all(&self, registry: &mut ResourceRegistry) {
        for (buffer, BufferState { stage, access }) in &self.buffers {
            registry.request_buffer(buffer, *stage, *access).unwrap();
        }

        for (
            image,
            ImageState {
                stage,
                access,
                layout,
            },
        ) in &self.images
        {
            registry
                .request_image(image, *stage, *access, *layout)
                .unwrap();
        }

        for sampler in &self.samplers {
            registry.request_sampler(sampler).unwrap();
        }
    }
}
