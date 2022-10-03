use crate::{
    resources::{
        res_states::{AnyResKey, BufferKey, ImageKey, SamplerKey},
        Resources,
    },
    BufferHandle, CtxRmg, ImageHandle, RecordError, SamplerHandle,
};
use marpii::{ash::vk, context::Device};
use std::{any::Any, ops::Deref, sync::Arc};

pub struct ResourceRegistry {
    images: Vec<ImageKey>,
    buffers: Vec<BufferKey>,
    sampler: Vec<SamplerKey>,

    foreign_sem: Vec<Arc<vk::Semaphore>>,
    ///Collects all resources handle used in the registry
    /// is later used to move them into an executions collector
    pub(crate) resource_collection: Vec<Box<dyn Any + Send>>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        ResourceRegistry {
            images: Vec::new(),
            buffers: Vec::new(),
            sampler: Vec::new(),
            foreign_sem: Vec::new(),
            resource_collection: Vec::new(),
        }
    }

    ///Registers `image` as needed storage image.
    pub fn request_image(&mut self, image: &ImageHandle) {
        self.images.push(image.key);
        self.resource_collection
            .push(Box::new(image.imgref.clone()));
    }

    ///Registers `buffer` as needed storage buffer.
    pub fn request_buffer<T: 'static>(&mut self, buffer: &BufferHandle<T>) {
        self.buffers.push(buffer.key);
        self.resource_collection
            .push(Box::new(buffer.bufref.clone()));
    }

    ///Registers `sampler` as needed sampler.
    pub fn request_sampler(&mut self, sampler: &SamplerHandle) {
        self.sampler.push(sampler.key);
        self.resource_collection
            .push(Box::new(sampler.samref.clone()));
    }

    ///Registers *any*thing to be kept alive until the task finishes its execution.
    pub fn register_asset<T: Any + Send + 'static>(&mut self, asset: T) {
        self.resource_collection.push(Box::new(asset));
    }

    ///Registers that this foreign semaphore must be signaled after execution. Needed for swapchain stuff.
    pub fn register_foreign_semaphore(&mut self, semaphore: Arc<vk::Semaphore>) {
        self.foreign_sem.push(semaphore.clone());
        self.resource_collection.push(Box::new(semaphore))
    }

    pub(crate) fn any_res_iter<'a>(&'a self) -> impl Iterator<Item = AnyResKey> + 'a {
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
