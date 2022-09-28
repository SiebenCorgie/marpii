use crate::{
    resources::{
        res_states::{AnyResKey, BufferKey, ImageKey, SamplerKey},
        Resources,
    },
    CtxRmg, RecordError,
};
use marpii::{
    ash::vk,
    context::Device,
};
use std::{ops::Deref, sync::Arc};

pub struct ResourceRegistry {
    images: Vec<ImageKey>,
    buffers: Vec<BufferKey>,
    sampler: Vec<SamplerKey>,

    foreign_sem: Vec<Arc<vk::Semaphore>>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        ResourceRegistry {
            images: Vec::new(),
            buffers: Vec::new(),
            sampler: Vec::new(),
            foreign_sem: Vec::new(),
        }
    }

    ///Registers `image` as needed storage image.
    pub fn request_image(&mut self, image: ImageKey) {
        self.images.push(image);
    }

    ///Registers `buffer` as needed storage buffer.
    pub fn request_buffer(&mut self, buffer: BufferKey) {
        self.buffers.push(buffer);
    }

    ///Registers `sampler` as needed sampler.
    pub fn request_sampler(&mut self, sampler: SamplerKey) {
        self.sampler.push(sampler);
    }

    ///Registers that this foreign semaphore must be signaled after execution. Needed for swapchain stuff.
    pub(crate) fn register_foreign_semaphore(&mut self, semaphore: Arc<vk::Semaphore>) {
        self.foreign_sem.push(semaphore);
    }

    pub fn any_res_iter<'a>(&'a self) -> impl Iterator<Item = AnyResKey> + 'a {
        self.images
            .iter()
            .map(|img| AnyResKey::Image(*img))
            .chain(self.buffers.iter().map(|buf| AnyResKey::Buffer(*buf)))
            .chain(self.sampler.iter().map(|sam| AnyResKey::Sampler(*sam)))
    }

    pub(crate) fn append_foreign_signal_semaphores(
        &self,
        infos: &mut Vec<vk::SemaphoreSubmitInfo>,
    ) {
        for sem in self.foreign_sem.iter() {
            #[cfg(feature = "logging")]
            log::trace!("Registering foreign semaphore {:?}", sem.deref().deref());

            infos.push(
                vk::SemaphoreSubmitInfo::builder()
                    .semaphore(**sem)
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .build(),
            );
        }
    }
}

pub trait Task {
    ///Gets called right before building the execution graph. Allows access to the Resources.
    fn pre_record(&mut self, _resources: &mut Resources, _ctx: &CtxRmg) -> Result<(), RecordError> {
        Ok(())
    }

    ///Gets called right after executing the resource graph
    fn post_execution(
        &mut self,
        _resources: &mut Resources,
        _ctx: &CtxRmg,
    ) -> Result<(), RecordError> {
        Ok(())
    }

    ///Gets called while building a execution graph. This function must register all resources that are
    /// needed for successfull execution.
    fn register(&self, registry: &mut ResourceRegistry);

    fn record(
        &mut self,
        device: &Arc<Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    );

    ///Signals the task type to the recorder. By default this is compute only.
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }

    ///Can be implemented to make debugging easier
    fn name(&self) -> &'static str {
        "Unnamed Task"
    }
}
