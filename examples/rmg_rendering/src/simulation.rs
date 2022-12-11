use marpii::{
    ash::vk,
    resources::{ComputePipeline, ImgDesc, PushConstant, ShaderModule},
};
use marpii_rmg::{BufferHandle, ImageHandle, Rmg, RmgError, Task};
use shared::{ResourceHandle, SimObj};
use std::sync::Arc;

use crate::OBJECT_COUNT;
const SHADER_COMP: &[u8] = include_bytes!("../resources/simulation.spv");

pub struct Simulation {
    ///Simulation buffer
    pub sim_buffer: BufferHandle<SimObj>,
    pub feedback_image: ImageHandle,
    is_init: bool,

    pipeline: Arc<ComputePipeline>,
    push: PushConstant<shared::SimPush>,
}

impl Simulation {
    pub const SUBGROUP_COUNT: u32 = 64;

    pub fn new(rmg: &mut Rmg) -> Result<Self, RmgError> {
        let push = PushConstant::new(
            shared::SimPush {
                sim_buffer: shared::ResourceHandle::new(
                    shared::ResourceHandle::TYPE_STORAGE_BUFFER,
                    0,
                ),
                is_init: 0,
                buf_size: OBJECT_COUNT as u32,
                img_handle: ResourceHandle::INVALID,
                img_width: 64,
                img_height: 64,
                pad: [0u32; 2],
            },
            vk::ShaderStageFlags::COMPUTE,
        );
        let shader_module = ShaderModule::new_from_bytes(&rmg.ctx.device, SHADER_COMP)?;
        let shader_stage = shader_module.into_shader_stage(vk::ShaderStageFlags::COMPUTE, "main");
        //No additional descriptors for us
        let layout = rmg.resources().bindless_layout();
        let pipeline = Arc::new(ComputePipeline::new(
            &rmg.ctx.device,
            &shader_stage,
            None,
            layout,
        )?);

        let feedback_image = rmg.new_image_uninitialized(
            ImgDesc::storage_image_2d(64, 64, vk::Format::R8G8B8A8_UNORM),
            None,
        )?;

        Ok(Simulation {
            sim_buffer: rmg.new_buffer::<SimObj>(OBJECT_COUNT, Some("SimBuffer 1"))?,
            feedback_image,
            is_init: false,
            pipeline,
            push,
        })
    }

    fn dispatch_count() -> u32 {
        ((OBJECT_COUNT as f32) / Self::SUBGROUP_COUNT as f32).ceil() as u32
    }
}

impl Task for Simulation {
    fn name(&self) -> &'static str {
        "Simulation"
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }

    fn pre_record(
        &mut self,
        resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        self.push.get_content_mut().sim_buffer =
            resources.resource_handle_or_bind(self.sim_buffer.clone())?;
        self.push.get_content_mut().img_handle =
            resources.resource_handle_or_bind(self.feedback_image.clone())?;
        self.push.get_content_mut().is_init = self.is_init.into();

        if !self.is_init {
            self.is_init = true;
        }

        Ok(())
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_buffer(
                &self.sim_buffer,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::empty(),
            )
            .unwrap();
        registry.register_asset(self.pipeline.clone());
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        _resources: &marpii_rmg::Resources,
    ) {
        //bind commandbuffer, setup push constant and execute
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

            device
                .inner
                .cmd_dispatch(*command_buffer, Self::dispatch_count(), 1, 1);
        }
    }
}
