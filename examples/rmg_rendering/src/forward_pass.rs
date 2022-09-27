use marpii::{ash::vk, context::Device, resources::{ComputePipeline, PushConstant, ShaderModule, ImgDesc, Image}, allocator::MemoryUsage};
use marpii_rmg::{ImageKey, ResourceRegistry, AttachmentDescription, Resources, Task, BufferKey, Rmg, RmgError, CtxRmg};
use shared::ResourceHandle;
use std::sync::Arc;

use crate::OBJECT_COUNT;


pub struct ForwardPass {
//    attdesc: AttachmentDescription,
    pub dst_img: ImageKey,
    pub sim_src: Option<BufferKey>,

    target_img_ext: vk::Extent2D,

    pipeline: ComputePipeline,
    push: PushConstant<shared::ForwardPush>,
}

impl ForwardPass {

    pub const SUBGROUP_COUNT: u32 = 64;
    pub const FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;

    pub fn new(rmg: &mut Rmg) -> Result<Self, RmgError>{
        println!("Setup Forward");
        let push = PushConstant::new(
            shared::ForwardPush{
                buf: 0,
                target_img: 0,
                width: 1,
                height: 1,
                buffer_size: OBJECT_COUNT as u32,
                pad: [0; 3]
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
                1,
                1,
                Self::FORMAT
            ),
            Some("target img")
        )?;


        Ok(ForwardPass {
            dst_img: target_img,
            sim_src: None,
            target_img_ext: vk::Extent2D::default(),

            pipeline,
            push
        })
    }

    fn dispatch_count(&self) -> [u32; 2]{

        [
            (self.target_img_ext.width as f32 / 8 as f32).ceil() as u32,
            (self.target_img_ext.height as f32 / 8  as f32).ceil() as u32,
        ]
    }

    fn flip_target_buffer(&mut self, resources: &mut Resources, ctx: &CtxRmg) -> Result<(), marpii_rmg::RecordError> {
        println!("Renewing target image for -> {:?}!", resources.get_surface_extent());
        resources.remove_resource(self.dst_img)?;
        self.dst_img = resources.add_image(Arc::new(
            Image::new(
                &ctx.device,
                &ctx.allocator,
                ImgDesc::storage_image_2d(self.target_img_ext.width, self.target_img_ext.height, vk::Format::R32G32B32A32_SFLOAT),
                MemoryUsage::GpuOnly,
                None,
                None
            )?
        ))?;

        Ok(())
    }
}

impl Task for ForwardPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        if let Some(buf) = self.sim_src{
            registry.request_buffer(buf);
        }
        registry.request_image(self.dst_img);

    }

    fn pre_record(&mut self, resources: &mut Resources, ctx: &marpii_rmg::CtxRmg) -> Result<(), marpii_rmg::RecordError> {

        let img_ext = {
            let desc = resources.get_image_desc(self.dst_img).unwrap();
            vk::Extent2D{
                width: desc.extent.width,
                height: desc.extent.height
            }
        };
        if resources.get_surface_extent() != img_ext{
            self.target_img_ext = resources.get_surface_extent();
            self.flip_target_buffer(resources, ctx)?;
        }

        self.push.get_content_mut().buf = resources.get_resource_handle(self.sim_src.unwrap())?.index();
        self.push.get_content_mut().target_img = resources.get_resource_handle(self.dst_img)?.index();
        self.push.get_content_mut().width = self.target_img_ext.width;
        self.push.get_content_mut().height = self.target_img_ext.height;

        println!("Push: buf: {:x}", self.push.get_content().buf);
        println!("Push: img: {:x}", self.push.get_content().target_img);
        Ok(())
    }

    fn post_execution(&mut self, resources: &mut Resources, ctx: &CtxRmg) -> Result<(), marpii_rmg::RecordError> {
        self.flip_target_buffer(resources, ctx)
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
            let disp = self.dispatch_count();
            println!("Dispatching for {:?} @ {:?}", disp, self.target_img_ext);
            device.inner.cmd_dispatch(*command_buffer, disp[0], disp[1], 1);
        }
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE
    }
    fn name(&self) -> &'static str {
        "ForwardPass"
    }
}
