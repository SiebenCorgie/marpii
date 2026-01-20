//! Generic, temporal compute pass recording.

use crate::{
    helper::{BufferUsage, ImageUsage, ResourceStorage},
    BufferHandle, ImageHandle, RecordError, Rmg, RmgError, SamplerHandle, Task,
};
use marpii::{
    ash::vk::{self, DeviceSize},
    resources::{ComputePipeline, PushConstant, ShaderModule},
    OoS,
};
use std::sync::Arc;

///A generic compute pass that dispatches an amount of wavefronts for a given
/// pipeline using a push-constant.
///
/// This is designed to be used _once_ per frame. I.e. it does not allocate much
/// and the pass is not expected to be mutated.
pub struct GenericComputePass<P: 'static> {
    pipeline: Arc<ComputePipeline>,
    push: PushConstant<P>,
    dispatch: DispatchType,
    name: Option<String>,
    storage: ResourceStorage,
}

pub enum DispatchType {
    ///Launches the compute shader directly with the given extent
    Direct([u32; 3]),
    ///Launches the compute shader by reading the dispatch parameter in `buffer` at `offset` (in bytes) once.
    Indirect {
        buffer: BufferHandle<u32>,
        addional_usage: BufferUsage,
        offset: DeviceSize,
    },
}

impl DispatchType {
    fn uses_buffer<T>(&self, buffer: &BufferHandle<T>) -> bool {
        if let DispatchType::Indirect { buffer: inuse, .. } = self {
            buffer.key == inuse.key
        } else {
            false
        }
    }
}

impl Default for DispatchType {
    fn default() -> Self {
        DispatchType::Direct([1; 3])
    }
}

impl<P: 'static> GenericComputePass<P> {
    ///Initializes the pass for `pipeline` with no other features set
    pub fn init(pipeline: Arc<ComputePipeline>) -> GenericComputePass<()> {
        GenericComputePass {
            pipeline,
            push: PushConstant::new((), vk::ShaderStageFlags::COMPUTE),
            dispatch: DispatchType::default(),
            name: None,
            storage: ResourceStorage::new(),
        }
    }

    ///Lets you reconfigure the push constant _in-place_
    ///
    /// # Safety
    ///
    /// Make sure that you have registered any used resource, before making it availabel to use.
    pub fn push_constant_content_mut(&mut self) -> &mut P {
        self.push.get_content_mut()
    }

    pub fn push_constant_content(&self) -> &P {
        self.push.get_content()
    }

    ///Swaps out the pipeline used for dispatching the pass.
    ///
    /// # Safety: its your resonsibility to make sure that the pipeline object
    /// is compatible with the resources, push constant etc.
    pub fn swap_pipeline(&mut self, pipeline: Arc<ComputePipeline>) {
        self.pipeline = pipeline;
    }

    ///Clones the inner, used pipeline. Nice if you want to setup yet another pass
    /// based on this pipeline without having to recreate the actual pipeline (which is expensive).
    pub fn pipeline(&self) -> Arc<ComputePipeline> {
        self.pipeline.clone()
    }

    ///Schedules the pass for direct execution with the given number of waves per axis.
    ///
    /// # Safety: Its your responsibility to make sure that the dispatch size matches the intended usecase this pass was originally
    /// created for.
    ///
    /// Returns an error if this pass was recorded for indirect dispatch.
    pub fn set_direct_dispatch_size(&mut self, dispatch_size: [u32; 3]) -> Result<(), RecordError> {
        #[cfg(feature = "log")]
        if dispatch_size.contains(&0) {
            log::error!(
                "Dispatch: {}: {:?} contain invalid zero-sized axis!",
                self.name(),
                dispatch_size
            );
        }

        if let DispatchType::Direct(sizes) = &mut self.dispatch {
            *sizes = dispatch_size;
            Ok(())
        } else {
            Err(RecordError::GenericPassError(
                "Tried to set dispatch size, but in indirectly dispatched!".to_owned(),
            ))
        }
    }

    ///Sets the source buffer for the indirect dispatch command (and its in-shader usage). Fails if this
    /// pass was never setup for indirect dispatching.
    pub fn set_indirect_dispatch(
        &mut self,
        buffer: BufferHandle<u32>,
        offset: DeviceSize,
        usage: BufferUsage,
    ) -> Result<(), RecordError> {
        if let DispatchType::Indirect {
            buffer: inner_buffer,
            addional_usage,
            offset: inne_offset,
        } = &mut self.dispatch
        {
            *inner_buffer = buffer;
            *inne_offset = offset;
            *addional_usage = usage;

            Ok(())
        } else {
            Err(RecordError::GenericPassError(
                "Was not setup for indirect dispatch".to_owned(),
            ))
        }
    }

    pub fn is_direct_dispatch(&self) -> bool {
        if let DispatchType::Direct(_) = self.dispatch {
            true
        } else {
            false
        }
    }

    ///Returns the current direct-dispatch size, if this pass is configured for direct dispatch.
    pub fn dispatch_size(&self) -> Option<[u32; 3]> {
        if let DispatchType::Direct(d) = self.dispatch {
            Some(d)
        } else {
            None
        }
    }

    ///Allows the reconfiguration of the pass while reusing allocated buffers.
    ///
    /// If `keep_resources` is true, keeps any knowledge about used resources (i.e. via `use_image` etc.).
    /// Otherwise its reset.
    ///
    /// The push-constant is reset regardless, since unregistered `ResourceHandles` might slip through otherwise
    pub fn reconfigure<'rmg>(
        mut self,
        rmg: &'rmg mut Rmg,
        keep_resources: bool,
    ) -> ComputePassBuilder<'rmg, ()> {
        if !keep_resources {
            self.storage.reset();
        }

        ComputePassBuilder {
            task_setup: GenericComputePass {
                pipeline: self.pipeline,
                push: PushConstant::new((), vk::ShaderStageFlags::COMPUTE),
                dispatch: self.dispatch,
                name: self.name,
                storage: self.storage,
            },
            rmg,
        }
    }
}

impl<P: 'static> Task for GenericComputePass<P> {
    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("GenericComputePass")
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }

    fn register(&self, registry: &mut crate::ResourceRegistry) {
        //if there is an indirect buffer in use, register it with its declared usage _AND_
        // the appropriate pipeline stage
        if let DispatchType::Indirect {
            buffer,
            addional_usage,
            offset: _,
        } = &self.dispatch
        {
            registry
                .request_buffer(
                    buffer,
                    vk::PipelineStageFlags2::DRAW_INDIRECT
                        | vk::PipelineStageFlags2::COMPUTE_SHADER,
                    vk::AccessFlags2::INDIRECT_COMMAND_READ | addional_usage.into_access_flags(),
                )
                .expect("Could not register indirect dispatch buffer");
        }

        self.storage
            .register_all(registry, vk::PipelineStageFlags2::COMPUTE_SHADER);
        //Always keep pipeline alive as long as possible
        registry.register_asset(self.pipeline.clone());
    }

    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &crate::Resources,
    ) {
        //bind pipeline, setup push constant and execute
        unsafe {
            device.inner.cmd_bind_pipeline(
                *command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline.pipeline,
            );
            device.inner.cmd_push_constants(
                *command_buffer,
                self.pipeline.layout.layout,
                vk::ShaderStageFlags::ALL,
                0,
                self.push.content_as_bytes(),
            );

            //Depending on the dispatch, write down the actualy launch command
            match &self.dispatch {
                DispatchType::Direct([x, y, z]) => {
                    device.inner.cmd_dispatch(*command_buffer, *x, *y, *z);
                }
                DispatchType::Indirect {
                    buffer,
                    offset,
                    addional_usage: _,
                } => {
                    let buffer_access = &resources.get_buffer_state(buffer).buffer;
                    device.inner.cmd_dispatch_indirect(
                        *command_buffer,
                        buffer_access.inner,
                        *offset,
                    );
                }
            }
        }
    }
}

pub struct ComputePassBuilder<'ctx, P: 'static> {
    task_setup: GenericComputePass<P>,
    rmg: &'ctx mut Rmg,
}

impl<'ctx, P: 'static> ComputePassBuilder<'ctx, P> {
    ///Generates the _final_ push constant for the pass. I.e. use `configure` to fetch all
    /// `ResourceHandle`
    pub fn with_push_constant<PC: 'static>(
        self,
        configure: impl Fn(&mut Rmg) -> PC,
    ) -> ComputePassBuilder<'ctx, PC> {
        assert!(
            std::mem::size_of::<PC>()
                <= self.rmg.config().limit.limits.max_push_constants_size as usize,
            "Push constant size exceeds limit"
        );

        let GenericComputePass {
            pipeline,
            push: _,
            dispatch,
            name,
            storage,
        } = self.task_setup;

        let push_constant = configure(self.rmg);
        let new_push_constant = PushConstant::new(push_constant, vk::ShaderStageFlags::COMPUTE);

        ComputePassBuilder {
            task_setup: GenericComputePass {
                pipeline,
                dispatch,
                name,
                push: new_push_constant,
                storage,
            },
            rmg: self.rmg,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.task_setup.name = Some(name.into());
        self
    }

    /// Signals that the pass will read this image through either storage or sample operations
    /// based on the signaled flag.
    pub fn use_image(mut self, image: ImageHandle, usage: ImageUsage) -> Self {
        assert!(!usage.is_attachment(), "Cannot use attachments in Compute");
        self.task_setup.storage.images.push((image, usage));
        self
    }

    /// Signals all images with the same usage. Nice if you have a set of textures for instance
    /// where you want to mark all as sampled.
    pub fn use_images(mut self, images: &[ImageHandle], usage: ImageUsage) -> Self {
        assert!(!usage.is_attachment(), "Cannot use attachments in Compute");
        self.task_setup
            .storage
            .images
            .extend(images.iter().map(|img| (img.clone(), usage)));
        self
    }

    /// Signals that the pass will write to this resource
    pub fn use_buffer<T: 'static>(mut self, buffer: BufferHandle<T>, usage: BufferUsage) -> Self {
        if self.task_setup.dispatch.uses_buffer(&buffer) {
            panic!("buffer is already in use as indirect-dispatch source!");
        }

        self.task_setup
            .storage
            .buffers
            .push((buffer.type_erase(), usage));
        self
    }

    /// Signals that the pass will use the sampler
    pub fn use_sampler(mut self, sampler: SamplerHandle) -> Self {
        self.task_setup.storage.samplers.push(sampler);
        self
    }

    ///Schedules the pass for direct execution with the given number of waves per axis.
    pub fn direct_dispatch_size(mut self, dispatch_size: [u32; 3]) -> Result<Self, RecordError> {
        #[cfg(feature = "log")]
        if dispatch_size.contains(&0) {
            log::error!(
                "Dispatch: {}: {:?} contain invalid zero-sized axis!",
                self.task_setup.name(),
                dispatch_size
            );
        }

        self.task_setup.dispatch = DispatchType::Direct(dispatch_size);

        Ok(self)
    }

    ///Schedules the pass for indirect execution via the given `buffer` that holds
    /// the dispatch parameters (3x u32) at the `offset` (in bytes).
    ///
    /// If the buffer is used in the dispatched shader, use `usage` to add addional usage flags.
    pub fn indirect_dispatch(
        mut self,
        buffer: BufferHandle<u32>,
        offset: DeviceSize,
        usage: BufferUsage,
    ) -> Result<Self, RecordError> {
        //Set the buffer as the indirect launch buffer, and, if needed register an additional
        // buffer usage.

        //make sure the buffer isn't already in use by the registry
        if self
            .task_setup
            .storage
            .buffers
            .iter()
            .find(|(used_buffer, _usage)| used_buffer.key == buffer.key)
            .is_some()
        {
            return Err(RecordError::GenericPassError(
                "Indirect dispatch error was already registered with a different usage!".to_owned(),
            ));
        }

        self.task_setup.dispatch = DispatchType::Indirect {
            buffer,
            offset,
            addional_usage: usage,
        };
        Ok(self)
    }

    pub fn finish(self) -> GenericComputePass<P> {
        let ComputePassBuilder { task_setup, rmg: _ } = self;
        task_setup
    }
}

impl Rmg {
    ///Creates a new, configurable compute pass.
    pub fn new_compute_pass<'rmg>(
        &'rmg mut self,
        pipeline: Arc<ComputePipeline>,
    ) -> ComputePassBuilder<'rmg, ()> {
        ComputePassBuilder {
            task_setup: GenericComputePass::<()>::init(pipeline),
            rmg: self,
        }
    }

    ///Creates a new generic compute-pipeline that matches the bindless
    /// pipeline-layout and enters `shader_code` at `entry_point`
    pub fn compute_pipeline(
        &mut self,
        entry_point: &str,
        shader_code: &[u8],
    ) -> Result<Arc<ComputePipeline>, RmgError> {
        let shader_module = ShaderModule::new_from_bytes(&self.ctx.device, shader_code)
            .map_err(|e| RecordError::MarpiiError(e.into()))?;
        let shader_stage =
            shader_module.into_shader_stage(vk::ShaderStageFlags::COMPUTE, entry_point);

        let layout = self.resources.bindless_layout();
        Ok(Arc::new(
            ComputePipeline::new(
                &self.ctx.device,
                &shader_stage,
                None,
                OoS::new_shared(layout),
            )
            .map_err(|e| RecordError::MarpiiError(e.into()))?,
        ))
    }
}
