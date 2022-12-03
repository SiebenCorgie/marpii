use crate::{
    resources::{
        res_states::{AnyResKey, BufferKey, ImageKey, SamplerKey},
        Resources,
    },
    BufferHandle, CtxRmg, ImageHandle, RecordError, ResourceError, Rmg, SamplerHandle,
};
use ahash::{AHashMap, AHashSet};
use marpii::{
    ash::vk::{self, ImageLayout},
    context::Device, sync::BinarySemaphore,
};
use marpii_commands::BarrierBuilder;
use std::{any::Any, sync::Arc};

pub struct ResourceRegistry {
    images: AHashMap<ImageKey, (vk::PipelineStageFlags2, vk::AccessFlags2, vk::ImageLayout)>,
    buffers: AHashMap<BufferKey, (vk::PipelineStageFlags2, vk::AccessFlags2)>,
    sampler: AHashSet<SamplerKey>,

    binary_signal_sem: Vec<Arc<BinarySemaphore>>,
    binary_wait_sem: Vec<Arc<BinarySemaphore>>,
    ///Collects all resources handle used in the registry
    /// is later used to move them into an executions collector
    pub(crate) resource_collection: Vec<Box<dyn Any + Send>>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        ResourceRegistry {
            images: AHashMap::new(),
            buffers: AHashMap::new(),
            sampler: AHashSet::new(),
            binary_signal_sem: Vec::new(),
            binary_wait_sem: Vec::new(),
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
    ) -> Result<(), ResourceError> {
        if self
            .images
            .insert(image.key, (stage, access, layout))
            .is_some()
        {
            return Err(ResourceError::ResourceAlreadyRequested);
        }
        self.resource_collection
            .push(Box::new(image.imgref.clone()));
        Ok(())
    }

    ///Registers `buffer` as needed buffer. The buffer will be available in the given `stage` when using `access`.
    ///
    ///
    /// Returns `Err` if the buffer was already registered.
    pub fn request_buffer<T: 'static>(
        &mut self,
        buffer: &BufferHandle<T>,
        stage: vk::PipelineStageFlags2,
        access: vk::AccessFlags2,
    ) -> Result<(), ResourceError> {
        if self.buffers.insert(buffer.key, (stage, access)).is_some() {
            return Err(ResourceError::ResourceAlreadyRequested);
        }
        self.resource_collection
            .push(Box::new(buffer.bufref.clone()));
        Ok(())
    }

    ///Registers `sampler` as needed sampler.
    ///
    ///
    ///
    /// Returns `Err` if the sampler was already registered.
    pub fn request_sampler(&mut self, sampler: &SamplerHandle) -> Result<(), ResourceError> {
        if !self.sampler.insert(sampler.key) {
            return Err(ResourceError::ResourceAlreadyRequested);
        }
        self.resource_collection
            .push(Box::new(sampler.samref.clone()));

        Ok(())
    }

    ///Registers *any*thing to be kept alive until the task finishes its execution.
    pub fn register_asset<T: Any + Send + 'static>(&mut self, asset: T) {
        self.resource_collection.push(Box::new(asset));
    }

    ///Registers that this foreign semaphore must be signalled after execution. Needed for swapchain stuff.
    pub fn register_binary_signal_semaphore(&mut self, semaphore: Arc<BinarySemaphore>) {
        self.binary_signal_sem.push(semaphore.clone());
        self.resource_collection.push(Box::new(semaphore))
    }

    ///Registers that this foreign semaphore must be waited uppon before execution. Needed for swapchain stuff.
    pub fn register_binary_wait_semaphore(&mut self, semaphore: Arc<BinarySemaphore>) {
        self.binary_wait_sem.push(semaphore.clone());
        self.resource_collection.push(Box::new(semaphore))
    }

    pub(crate) fn any_res_iter<'a>(&'a self) -> impl Iterator<Item = AnyResKey> + 'a {
        self.images
            .keys()
            .map(|img| AnyResKey::Image(*img))
            .chain(self.buffers.keys().map(|buf| AnyResKey::Buffer(*buf)))
            .chain(self.sampler.iter().map(|sam| AnyResKey::Sampler(*sam)))
    }

    /// Appends all foreign binary semaphores. Mostly used to integrate swapchains.
    pub(crate) fn append_binary_signal_semaphores(
        &self,
        infos: &mut Vec<vk::SemaphoreSubmitInfo>,
    ) {
        for sem in self.binary_signal_sem.iter() {
            #[cfg(feature = "logging")]
            log::trace!("Registering foreign semaphore {:?}", sem.inner);

            infos.push(
                vk::SemaphoreSubmitInfo::builder()
                    .semaphore(sem.inner)
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .build(),
            );
        }
    }

    /// Appends all foreign binary semaphores. Mostly used to integrate swapchains.
    pub(crate) fn append_binary_wait_semaphores(&self, infos: &mut Vec<vk::SemaphoreSubmitInfo>) {
        for sem in self.binary_wait_sem.iter() {
            #[cfg(feature = "logging")]
            log::trace!("Registering foreign semaphore {:?}", sem.inner);

            infos.push(
                vk::SemaphoreSubmitInfo::builder()
                    .semaphore(sem.inner)
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .build(),
            );
        }
    }

    ///If in the registry: returns the stage flags the resource is registered for
    pub(crate) fn get_stage_mask(&self, resource: &AnyResKey) -> Option<vk::PipelineStageFlags2> {
        match resource {
            AnyResKey::Buffer(buf) => {
                if let Some(st) = self.buffers.get(buf) {
                    Some(st.0)
                } else {
                    None
                }
            }
            AnyResKey::Image(img) => {
                if let Some(st) = self.images.get(img) {
                    Some(st.0)
                } else {
                    None
                }
            }
            AnyResKey::Sampler(_) => None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn contains_key(&self, resource: &AnyResKey) -> bool {
        match resource {
            AnyResKey::Buffer(buf) => self.buffers.contains_key(buf),
            AnyResKey::Image(img) => self.images.contains_key(img),
            AnyResKey::Sampler(sam) => self.sampler.contains(sam),
        }
    }

    ///Calculates the difference between the current state of `resource`and the state it is registered in `self`. Uses `src_stage` to block
    /// barrier until this stage is reached. This is basically the main "on queue" sync mechanism between tasks. Use `ALL_COMMANDS` if unsure and
    /// refine later.
    ///
    /// If the there is a difference, a transition is calculated and appended to the `builder`. The new state is
    /// also set in the resources state within `rmg`
    pub(crate) fn add_diff_transition(
        &self,
        rmg: &mut Rmg,
        builder: &mut BarrierBuilder,
        resource: AnyResKey,
        src_stage: vk::PipelineStageFlags2,
    ) {
        match resource {
            AnyResKey::Buffer(buf) => {
                let bufstate = rmg.resources_mut().buffer.get_mut(buf).unwrap();
                let target_state = self.buffers.get(&buf).unwrap();
                let mut barrier = vk::BufferMemoryBarrier2::builder()
                    .buffer(bufstate.buffer.inner)
                    .offset(0)
                    .size(vk::WHOLE_SIZE);
                #[cfg(feature = "logging")]
                log::trace!("Trans Buffer {:?}", buf);
                //update access mask if needed
                if bufstate.mask != target_state.1 {
                    #[cfg(feature = "logging")]
                    log::trace!("    {:#?} -> {:#?}", bufstate.mask, target_state.1);
                    barrier = barrier
                        .src_access_mask(bufstate.mask)
                        .dst_access_mask(target_state.1);

                    bufstate.mask = target_state.1;
                }

                //add pipeline stages
                #[cfg(feature = "logging")]
                log::trace!("    {:#?} -> {:#?}", src_stage, target_state.0);
                barrier = barrier
                    .src_stage_mask(src_stage)
                    .dst_stage_mask(target_state.0);

                //now add
                builder.buffer_custom_barrier(*barrier);
            }
            AnyResKey::Image(img) => {
                let imgstate = rmg.resources_mut().images.get_mut(img).unwrap();
                let target_state = self.images.get(&img).unwrap();
                let mut barrier = vk::ImageMemoryBarrier2::builder()
                    .image(imgstate.image.inner)
                    .subresource_range(imgstate.image.subresource_all());
                #[cfg(feature = "logging")]
                log::trace!("Trans Image {:?}", img);

                //update access mask if needed
                if imgstate.mask != target_state.1 {
                    #[cfg(feature = "logging")]
                    log::trace!("    {:#?} -> {:#?}", imgstate.mask, target_state.1);
                    barrier = barrier
                        .src_access_mask(imgstate.mask)
                        .dst_access_mask(target_state.1);

                    imgstate.mask = target_state.1;
                }

                //update layout if neede
                if imgstate.layout != target_state.2 {
                    #[cfg(feature = "logging")]
                    log::trace!("    {:#?} -> {:#?}", imgstate.layout, target_state.2);
                    barrier = barrier
                        .old_layout(imgstate.layout)
                        .new_layout(target_state.2);

                    imgstate.layout = target_state.2;
                }

                //add pipeline stages
                #[cfg(feature = "logging")]
                log::trace!("    {:#?} -> {:#?}", src_stage, target_state.0);
                barrier = barrier
                    .src_stage_mask(src_stage)
                    .dst_stage_mask(target_state.0);

                //now add
                builder.image_custom_barrier(*barrier);
            }
            AnyResKey::Sampler(_) => {} //samplers never have a state
        }
    }

    pub(crate) fn num_resources(&self) -> usize {
        self.resource_collection.len()
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
