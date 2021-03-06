use anyhow::Result;
use marpii::ash::vk::Extent2D;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::{
    ComputePipeline, DescriptorPool, PipelineLayout, PushConstant, SafeImageView, ShaderModule,
};
use marpii::{
    ash,
    ash::vk,
    context::Ctx,
    resources::{Image, ImgDesc},
};
use marpii_command_graph::pass::{AssumedState, Pass, SubPassRequirement};
use marpii_command_graph::{ImageState, StImage};
use marpii_commands::Recorder;
use marpii_descriptor::managed_descriptor::{Binding, ManagedDescriptorSet};
use std::sync::{Arc, Mutex, RwLock};
#[repr(C)]
pub struct PushConst {
    pub radius: f32,
    pub opening: f32,
    pub offset: [f32; 2],
}

///Simple Compute dispatch pass that writes to `target image`
///
/// Note: In this implementation the pipeline gets created multiple times.
///       In a real world scenario you might want to share it. Possibly by implementing a factory-pattern like
///       host that creates passes for certain images.
pub struct ComputeDispatch {
    //shader_stage: ShaderStage,
    pub target_image: StImage,
    assumed_state: AssumedState,
    descriptor_set: Arc<RwLock<ManagedDescriptorSet<DescriptorPool>>>,
    pipeline: Arc<ComputePipeline>,
    push_constant: Arc<Mutex<PushConstant<PushConst>>>,
}

impl ComputeDispatch {
    pub fn new(ctx: &Ctx<Allocator>, extent: Extent2D) -> Self {
        let push_constant = Arc::new(Mutex::new(PushConstant::new(
            PushConst {
                offset: [500.0, 500.0],
                opening: (10.0f32).to_radians(),
                radius: 450.0,
            },
            ash::vk::ShaderStageFlags::COMPUTE,
        )));

        //load shader from file
        let shader_module =
            ShaderModule::new_from_file(&ctx.device, "resources/test_shader.spv").unwrap();
        let descriptor_set_layouts = shader_module.create_descriptor_set_layouts().unwrap();
        let shader_stage =
            shader_module.into_shader_stage(ash::vk::ShaderStageFlags::COMPUTE, "main".to_owned());

        let pipeline = {
            let pipeline_layout = PipelineLayout::from_layout_push(
                &ctx.device,
                &descriptor_set_layouts,
                &push_constant.lock().unwrap(),
            )
            .unwrap();

            let pipeline =
                ComputePipeline::new(&ctx.device, &shader_stage, None, pipeline_layout).unwrap();

            Arc::new(pipeline)
        };

        //Now create the target image and descriptor set
        let width = extent.width;
        let height = extent.height;

        let target_format = ctx
            .device
            .select_format(
                vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::STORAGE,
                vk::ImageTiling::OPTIMAL,
                &[
                    vk::Format::R8G8B8A8_UNORM,
                    vk::Format::R8G8B8A8_UINT,
                    vk::Format::R32G32B32A32_SFLOAT,
                ],
            )
            .unwrap();

        let imgdesc = ImgDesc::storage_image_2d(width, height, target_format);
        println!("Desc: {:#?}", imgdesc);

        let target_image = StImage::unitialized(
            Image::new(
                &ctx.device,
                &ctx.allocator,
                imgdesc,
                marpii::allocator::MemoryUsage::GpuOnly,
                Some("TargetImage"),
                None,
            )
            .unwrap(),
        );
        let image_view = Arc::new(
            target_image
                .image()
                .view(&ctx.device, target_image.image().view_all())
                .unwrap(),
        );
        //NOTE bad practise, should be done per app.
        let pool = DescriptorPool::new_for_module(
            &ctx.device,
            ash::vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET,
            shader_stage.module(),
            1,
        )
        .unwrap();

        let descriptor_set = Arc::new(RwLock::new(
            ManagedDescriptorSet::new(
                &ctx.device,
                pool,
                [Binding::new_image(
                    image_view,
                    ash::vk::ImageLayout::GENERAL,
                )],
                ash::vk::ShaderStageFlags::ALL,
            )
            .unwrap(),
        ));

        let assumed_state = AssumedState::Image {
            image: target_image.clone(),
            state: ImageState {
                access_mask: vk::AccessFlags::SHADER_WRITE,
                layout: vk::ImageLayout::GENERAL,
            },
        };

        ComputeDispatch {
            //shader_stage,
            target_image,
            assumed_state,
            descriptor_set,
            pipeline,
            push_constant,
        }
    }

    pub fn push_const(&self, c: PushConst) {
        *self.push_constant.lock().unwrap().get_content_mut() = c;
    }
}

impl Pass for ComputeDispatch {
    ///All outside facing resources state as it is assumed by this pass.
    fn assumed_states(&self) -> &[AssumedState] {
        core::slice::from_ref(&self.assumed_state)
    }

    ///The actual recording step. Gets provided with access to the actual resources through the
    /// `ResourceManager` as well as the `command_buffer` recorder that is currently in use.
    fn record(&mut self, recorder: &mut Recorder) -> Result<(), anyhow::Error> {
        //Setup pipeline etc.
        recorder.record({
            let pipe = self.pipeline.clone();
            move |dev, cmd| unsafe {
                dev.cmd_bind_pipeline(*cmd, ash::vk::PipelineBindPoint::COMPUTE, pipe.pipeline)
            }
        });

        recorder.record({
            let pipe = self.pipeline.clone();
            let descset = self.descriptor_set.clone();
            move |dev, cmd| unsafe {
                dev.cmd_bind_descriptor_sets(
                    *cmd,
                    ash::vk::PipelineBindPoint::COMPUTE,
                    pipe.layout.layout,
                    0,
                    &[*descset.read().unwrap().raw()],
                    &[],
                );
            }
        });

        recorder.record({
            let pipe = self.pipeline.clone();
            let push = self.push_constant.clone();
            move |dev, cmd| unsafe {
                dev.cmd_push_constants(
                    *cmd,
                    pipe.layout.layout,
                    ash::vk::ShaderStageFlags::COMPUTE,
                    0,
                    push.lock().unwrap().content_as_bytes(),
                )
            }
        });

        let ext = self.target_image.image().extent_2d();
        //now submit for the extend. We know that the shader is executing in 8x8x1, therefore conservatifly use the dispatch size.
        let submit_size = [
            (ext.width as f32 / 8.0).ceil() as u32,
            (ext.height as f32 / 8.0).ceil() as u32,
            1,
        ];

        //actual dispatch, since we can assume that the image is in the correct layout.
        recorder.record({
            move |dev, cmd| unsafe {
                dev.cmd_dispatch(*cmd, submit_size[0], submit_size[1], submit_size[2]);
            }
        });
        Ok(())
    }

    fn requirements(&self) -> &'static [SubPassRequirement] {
        &[SubPassRequirement::ComputeBit]
    }
}
