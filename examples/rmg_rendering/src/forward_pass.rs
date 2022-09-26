use marpii::{ash::vk, context::Device, resources::{ComputePipeline, PushConstant, ShaderModule, ImgDesc}};
use marpii_rmg::{ImageKey, ResourceRegistry, AttachmentDescription, Resources, Task, BufferKey, Rmg, RmgError};
use shared::ResourceHandle;
use std::sync::Arc;

use crate::OBJECT_COUNT;


pub struct ForwardPass {
//    attdesc: AttachmentDescription,
    pub dst_img: ImageKey,
    pub sim_src: Option<BufferKey>,

    pipeline: ComputePipeline,
    push: PushConstant<shared::ForwardPush>,
}

impl ForwardPass {

    pub const SUBGROUP_COUNT: u32 = 64;
    pub const EXT: vk::Extent2D = vk::Extent2D{
        width: 1024,
        height: 1024
    };

    pub fn new(rmg: &mut Rmg) -> Result<Self, RmgError>{
        println!("Setup Forward");
        let push = PushConstant::new(
            shared::ForwardPush{
                buf: ResourceHandle::new(0, 0),
                target_img: ResourceHandle::new(0, 0),
                width: Self::EXT.width,
                height: Self::EXT.height,
                buffer_size: OBJECT_COUNT as u32,
                pad: 0
            },
            vk::ShaderStageFlags::COMPUTE
        );
        let shader_module = ShaderModule::new_from_file(&rmg.ctx.device, "resources/forward_test.spv")?;
        let shader_stage = shader_module.into_shader_stage(
            vk::ShaderStageFlags::COMPUTE,
            "main"
        );
        //No additional descriptors for us
        let layout = rmg.resources().bindless_pipeline_layout(&[]);
        let pipeline = ComputePipeline::new(&rmg.ctx.device, &shader_stage, None, layout)?;

        let target_img = rmg.new_image_uninitialized(
            ImgDesc::storage_image_2d(
                Self::EXT.width,
                Self::EXT.height,
                vk::Format::R32G32B32A32_SFLOAT
            ),
            Some("target img")
        )?;


        Ok(ForwardPass {
            dst_img: target_img,
            sim_src: None,

            pipeline,
            push
        })
    }

    fn dispatch_count() -> u32{
        (((Self::EXT.width * Self::EXT.height) as f32) / Self::SUBGROUP_COUNT as f32).ceil() as u32
    }
}

impl Task for ForwardPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        if let Some(buf) = self.sim_src{
            registry.request_buffer(buf);
        }
        registry.request_image(self.dst_img);

    }

    fn pre_record(&mut self, resources: &mut Resources, _ctx: &marpii_rmg::CtxRmg) -> Result<(), marpii_rmg::RecordError> {
        self.push.get_content_mut().buf = resources.get_resource_handle(self.sim_src.unwrap())?;
        self.push.get_content_mut().target_img = resources.get_resource_handle(self.dst_img)?;

        println!("Push: buf: {:x}, {:x}", self.push.get_content().buf.index(), self.push.get_content().buf.handle_type());
        println!("Push: img: {:x}, {:x}", self.push.get_content().target_img.index(), self.push.get_content().target_img.handle_type());
        Ok(())
    }

    fn record(
        &mut self,
        device: &Arc<Device>,
        command_buffer: &vk::CommandBuffer,
        _resources: &Resources,
    ) {
        unsafe{
            device.inner.cmd_bind_pipeline(*command_buffer, vk::PipelineBindPoint::COMPUTE, self.pipeline.pipeline);
            device.inner.cmd_push_constants(
                *command_buffer,
                self.pipeline.layout.layout,
                vk::ShaderStageFlags::ALL,
                0,
                self.push.content_as_bytes()
            );
            println!("Dispatching {}", Self::dispatch_count());
            device.inner.cmd_dispatch(*command_buffer, Self::dispatch_count(), 0, 0);
        }
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE
    }
    fn name(&self) -> &'static str {
        "ForwardPass"
    }
}
