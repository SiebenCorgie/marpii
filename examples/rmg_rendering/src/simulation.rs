use marpii::{resources::{ComputePipeline, ShaderModule, PushConstant}, ash::vk};
use marpii_rmg::{Rmg, BufferKey, RmgError, Task};
use shared::SimObj;

use crate::OBJECT_COUNT;

pub struct Simulation{
    ///Simulation buffer where one is *src* and the other is *dst*
    /// with alternating keys.
    sim_buffer: [BufferKey; 2],
    ///Points to the current *src* buffer. Switches after each execution.
    current: usize,

    is_init: bool,

    pipeline: ComputePipeline,
    push: PushConstant<shared::SimPush>,
}

impl Simulation{

    pub const SUBGROUP_COUNT: u32 = 64;

    pub fn new(rmg: &mut Rmg) -> Result<Self, RmgError>{
        println!("Setup Simulation");
        let push = PushConstant::new(
            shared::SimPush{
                sim_src_buffer: shared::ResourceHandle::new(shared::ResourceHandle::TYPE_STORAGE_BUFFER, 0),
                sim_dst_buffer: shared::ResourceHandle::new(shared::ResourceHandle::TYPE_STORAGE_BUFFER, 0),
                is_init: 0,
                buf_size: OBJECT_COUNT as u32,
                pad: [0u32; 2]
            },
            vk::ShaderStageFlags::COMPUTE
        );
        let shader_module = ShaderModule::new_from_file(&rmg.ctx.device, "resources/simulation.spv")?;
        let shader_stage = shader_module.into_shader_stage(
            vk::ShaderStageFlags::COMPUTE,
            "main"
        );
        //No additional descriptors for us
        let layout = rmg.resources().bindless_pipeline_layout(&[]);
        let pipeline = ComputePipeline::new(&rmg.ctx.device, &shader_stage, None, layout)?;

        Ok(Simulation {
            sim_buffer: [
                rmg.new_buffer::<SimObj>(OBJECT_COUNT, Some("SimBuffer 1"))?,
                rmg.new_buffer::<SimObj>(OBJECT_COUNT, Some("SimBuffer 2"))?,
            ],
            current: 0,
            is_init: false,
            pipeline,
            push
        })
    }

    fn src_buffer(&self) -> BufferKey{
        self.sim_buffer[self.current % 2]
    }

    pub fn dst_buffer(&self) -> BufferKey{
        self.sim_buffer[(self.current + 1) % 2]
    }

    fn switch(&mut self){
        self.current = (self.current + 1) % 2;
    }

    fn dispatch_count() -> u32{
        ((OBJECT_COUNT as f32) / Self::SUBGROUP_COUNT as f32).ceil() as u32
    }
}


impl Task for Simulation {
    fn name(&self) -> &'static str {
        "Simulation"
    }

    fn post_execution(&mut self, _resources: &mut marpii_rmg::Resources, _ctx: &marpii_rmg::CtxRmg) -> Result<(), marpii_rmg::RecordError> {
        self.switch();
        Ok(())
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }

    fn pre_record(&mut self, resources: &mut marpii_rmg::Resources, _ctx: &marpii_rmg::CtxRmg) -> Result<(), marpii_rmg::RecordError> {
        self.push.get_content_mut().sim_src_buffer = resources.get_resource_handle(self.src_buffer())?;
        self.push.get_content_mut().sim_dst_buffer = resources.get_resource_handle(self.dst_buffer())?;
        self.push.get_content_mut().is_init = self.is_init.into();

        if !self.is_init{
            self.is_init = true;
        }

        Ok(())
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry.request_buffer(self.dst_buffer());
        registry.request_buffer(self.src_buffer());
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        _resources: &marpii_rmg::Resources,
    ) {

        //bind commandbuffer, setup push constant and execute
        unsafe{
            device.inner.cmd_bind_pipeline(*command_buffer, vk::PipelineBindPoint::COMPUTE, self.pipeline.pipeline);
            device.inner.cmd_push_constants(
                *command_buffer,
                self.pipeline.layout.layout,
                vk::ShaderStageFlags::ALL,
                0,
                self.push.content_as_bytes()
            );

            device.inner.cmd_dispatch(*command_buffer, Self::dispatch_count(), 0, 0);
        }
    }
}
