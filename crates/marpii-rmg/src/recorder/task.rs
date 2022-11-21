use crate::{
    resources::{
        res_states::{AnyResKey, BufferKey, ImageKey, SamplerKey},
        Resources,
    },
    BufferHandle, CtxRmg, ImageHandle, RecordError, SamplerHandle,
};
use ahash::AHashSet;
use marpii::{
    ash::vk::{self, ImageLayout},
    context::Device,
};
use std::{any::Any, ops::Deref, sync::Arc};

pub struct ResourceRegistry {
    images: AHashSet<(
        ImageKey,
        vk::PipelineStageFlags2,
        vk::AccessFlags2,
        vk::ImageLayout,
    )>,
    buffers: AHashSet<(BufferKey, vk::PipelineStageFlags2, vk::AccessFlags2)>,
    sampler: AHashSet<SamplerKey>,

    foreign_sem: Vec<Arc<vk::Semaphore>>,
    ///Collects all resources handle used in the registry
    /// is later used to move them into an executions collector
    pub(crate) resource_collection: Vec<Box<dyn Any + Send>>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        ResourceRegistry {
            images: AHashSet::new(),
            buffers: AHashSet::new(),
            sampler: AHashSet::new(),
            foreign_sem: Vec::new(),
            resource_collection: Vec::new(),
        }
    }

    ///Registers `image` as needed image. The Image will be supplied using the given `access`, transitioned to `layout`, and guaranteed available
    /// starting on `stage`.
    ///
    ///
    /// Returns `Err` if the image was already registered.
    pub fn request_image(
        &mut self,
        image: &ImageHandle,
        stage: vk::PipelineStageFlags2,
        access: vk::AccessFlags2,
        layout: ImageLayout,
    ) -> Result<(), ()> {
        if !self.images.insert((image.key, stage, access, layout)) {
            return Err(());
        }
        self.resource_collection
            .push(Box::new(image.imgref.clone()));
        Ok(())
    }

    ///Registers `buffer` as needed buffer. The buffer will be available in the given `stage` when using `access`.
    ///
    ///
    /// Returns `Err` if the image was already registered.
    pub fn request_buffer<T: 'static>(
        &mut self,
        buffer: &BufferHandle<T>,
        stage: vk::PipelineStageFlags2,
        access: vk::AccessFlags2,
    ) -> Result<(), ()> {
        if !self.buffers.insert((buffer.key, stage, access)) {
            return Err(());
        }
        self.resource_collection
            .push(Box::new(buffer.bufref.clone()));
        Ok(())
    }

    ///Registers `sampler` as needed sampler.
    ///
    ///
    ///
    /// Returns `Err` if the image was already registered.
    pub fn request_sampler(&mut self, sampler: &SamplerHandle) -> Result<(), ()> {
        if !self.sampler.insert(sampler.key) {
            return Err(());
        }
        self.resource_collection
            .push(Box::new(sampler.samref.clone()));

        Err(())
    }

    ///Registers *any*thing to be kept alive until the task finishes its execution.
    pub fn register_asset<T: Any + Send + 'static>(&mut self, asset: T) {
        self.resource_collection.push(Box::new(asset));
    }

    ///Registers that this foreign semaphore must be signalled after execution. Needed for swapchain stuff.
    pub fn register_foreign_semaphore(&mut self, semaphore: Arc<vk::Semaphore>) {
        self.foreign_sem.push(semaphore.clone());
        self.resource_collection.push(Box::new(semaphore))
    }

    pub(crate) fn any_res_iter<'a>(&'a self) -> impl Iterator<Item = AnyResKey> + 'a {
        self.images
            .iter()
            .map(|img| AnyResKey::Image(img.0))
            .chain(self.buffers.iter().map(|buf| AnyResKey::Buffer(buf.0)))
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
