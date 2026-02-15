use ahash::{AHashMap, AHashSet};
use marpii::ash::vk;

use crate::{BufferHandle, ImageHandle, SamplerHandle, resources::handle::TypeErased};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ImageState {
    stage: vk::PipelineStageFlags2,
    usage: vk::ImageUsageFlags,
    layout: vk::ImageLayout,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct BufferState {
    stage: vk::PipelineStageFlags2,
    usage: vk::BufferUsageFlags,
}

///Helper whenever you are using _a-lot_ of resources at once.
///
/// uses a special internal fast-path to register the resources.
pub struct ResourceRegister {
    images: AHashMap<ImageHandle, ImageState>,
    buffers: AHashMap<BufferHandle<TypeErased>, BufferState>,
    samplers: AHashSet<SamplerHandle>,
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
        usage: vk::ImageUsageFlags,
        layout: vk::ImageLayout,
    ) -> Option<(
        vk::PipelineStageFlags2,
        vk::ImageUsageFlags,
        vk::ImageLayout,
    )> {
        //On debug, make sure the usage flags are okay
        debug_assert!(
            image.usage_flags().contains(usage),
            "image does not contain {usage:?} flags"
        );

        if let Some(ImageState {
            stage,
            usage,
            layout,
        }) = self.images.insert(
            image,
            ImageState {
                stage,
                usage,
                layout,
            },
        ) {
            Some((stage, usage, layout))
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
        usage: vk::BufferUsageFlags,
    ) -> Option<(vk::PipelineStageFlags2, vk::BufferUsageFlags)> {
        debug_assert!(
            buffer.usage_flags().contains(usage),
            "Buffer does not have {usage:?} enabled"
        );

        if let Some(BufferState { stage, usage }) = self
            .buffers
            .insert(buffer.type_erase(), BufferState { stage, usage })
        {
            Some((stage, usage))
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
}
