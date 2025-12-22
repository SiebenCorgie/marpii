use std::sync::Arc;

use iced::Rectangle;
use iced_graphics::Settings;
use iced_marpii_shared::{CmdQuadGradient, QuadPush};
use marpii::{
    ash::vk,
    resources::{GraphicsPipeline, PushConstant, ShaderModule, ShaderStage},
    OoS,
};
use marpii_rmg::{ImageHandle, Rmg, Task};

use super::BatchCall;

pub fn pack_gradient_quad(gradient: iced::Gradient, bounds: Rectangle) -> CmdQuadGradient {
    let packed = match gradient {
        iced::Gradient::Linear(linear) => {
            let (start, end) = linear.angle.to_distance(&bounds);
            let direction = [start.x, start.y, end.x, end.y];

            let mut colors = [[0.0; 4]; 8];
            let mut offsets = [0.0; 8];

            //extract color and stop offset
            for (i, stop) in linear.stops.iter().enumerate() {
                colors[i] =
                    iced_graphics::color::pack(stop.map_or(iced::Color::default(), |s| s.color))
                        .components();
                offsets[i] = stop.map_or(0.0, |s| s.offset);
            }

            CmdQuadGradient {
                direction,
                colors_0: colors[0],
                colors_1: colors[1],
                colors_2: colors[2],
                colors_3: colors[3],
                colors_4: colors[4],
                colors_5: colors[5],
                colors_6: colors[6],
                colors_7: colors[7],
                offsets_0: [offsets[0], offsets[1], offsets[2], offsets[3]],
                offsets_1: [offsets[4], offsets[5], offsets[6], offsets[7]],
                ..Default::default()
            }
        }
    };

    packed
}

///The actual renderpass used to render the the gradient quad.
///
/// It uses a vertexbuffer-less DynamicRendering strategy.
///
/// What we do is registering all residing buffer-states
pub struct QuadGradientPass {
    color_image: ImageHandle,
    depth_image: ImageHandle,

    pipeline: Arc<GraphicsPipeline>,
    pub batches: Vec<BatchCall<CmdQuadGradient>>,
    pub push: PushConstant<QuadPush>,
}

impl QuadGradientPass {
    const SHADER_SOURCE: &'static [u8] =
        include_bytes!("../../shaders/spirv/shader_quad_gradient.spv");
    pub fn new(
        rmg: &mut Rmg,
        _settings: &Settings,
        color_image: ImageHandle,
        depth_image: ImageHandle,
    ) -> Self {
        let push = PushConstant::new(QuadPush::default(), vk::ShaderStageFlags::ALL_GRAPHICS);

        //setup the pipeline
        let mut shader_module =
            OoS::new(ShaderModule::new_from_bytes(&rmg.ctx.device, Self::SHADER_SOURCE).unwrap());

        let vertex_shader_stage = ShaderStage::from_module(
            shader_module.share(),
            vk::ShaderStageFlags::VERTEX,
            "vertex".to_owned(),
        );

        let fragment_shader_stage = ShaderStage::from_module(
            shader_module,
            vk::ShaderStageFlags::FRAGMENT,
            "fragment".to_owned(),
        );

        let pipeline = Self::quad_pipeline(
            rmg,
            &[vertex_shader_stage, fragment_shader_stage],
            color_image.format(),
            depth_image.format(),
        );

        Self {
            color_image,
            depth_image,
            pipeline,
            push,
            batches: Vec::new(),
        }
    }

    fn quad_pipeline(
        rmg: &mut Rmg,
        shader_stages: &[ShaderStage],
        color_format: &vk::Format,
        depth_format: &vk::Format,
    ) -> Arc<GraphicsPipeline> {
        let color_blend_attachments = vk::PipelineColorBlendAttachmentState::default()
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true);

        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .blend_constants([0.0; 4])
            .attachments(core::slice::from_ref(&color_blend_attachments)); //only the color target

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
            .depth_write_enable(true)
            .depth_test_enable(true)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false);

        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        //no other dynamic state

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .min_sample_shading(1.0)
            .alpha_to_one_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .cull_mode(vk::CullModeFlags::NONE)
            .depth_bias_enable(false)
            .depth_clamp_enable(false)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0)
            .polygon_mode(vk::PolygonMode::FILL);

        let tesselation_state = vk::PipelineTessellationStateCreateInfo::default();

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();

        let create_info = vk::GraphicsPipelineCreateInfo::default()
            .color_blend_state(&color_blend_state)
            .depth_stencil_state(&depth_stencil_state)
            .dynamic_state(&dynamic_state)
            .input_assembly_state(&input_assembly_state)
            .multisample_state(&multisample_state)
            .rasterization_state(&rasterization_state)
            .viewport_state(&viewport_state)
            .tessellation_state(&tesselation_state)
            .vertex_input_state(&vertex_input_state);

        let layout = rmg.resources.bindless_layout();
        let pipeline = GraphicsPipeline::new_dynamic_pipeline(
            &rmg.ctx.device,
            create_info,
            layout,
            shader_stages,
            std::slice::from_ref(color_format),
            Some(*depth_format),
        )
        .unwrap();
        Arc::new(pipeline)
    }

    pub fn resize(&mut self, color_buffer: ImageHandle, depth_buffer: ImageHandle) {
        self.color_image = color_buffer;
        self.depth_image = depth_buffer;
        let width = self.color_image.extent_2d().width;
        let height = self.color_image.extent_2d().height;
        self.push.get_content_mut().resolution = [width, height];
    }
}

impl Task for QuadGradientPass {
    fn name(&self) -> &'static str {
        "IcedQuadGradient"
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        for call in self.batches.iter() {
            registry
                .request_buffer(
                    &call.buffer,
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    vk::AccessFlags2::SHADER_STORAGE_READ,
                )
                .unwrap();
        }

        registry.register_asset(self.pipeline.clone());
        registry
            .request_image(
                &self.color_image,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            )
            .unwrap();

        registry
            .request_image(
                &self.depth_image,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
                vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            )
            .unwrap();
    }

    fn pre_record(
        &mut self,
        resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        //bind all resources
        for batch in &mut self.batches {
            batch.resource_handle = Some(resources.resource_handle_or_bind(batch.buffer.clone())?);
        }
        Ok(())
    }

    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        let (colorimg, colorview) = {
            let img_access = resources.get_image_state(&self.color_image);
            (img_access.image.clone(), img_access.view.clone())
        };
        let depthview = resources.get_image_state(&self.depth_image).view.clone();

        let render_area = colorimg.image_region().as_rect_2d();

        self.push.get_content_mut().resolution =
            [render_area.extent.width, render_area.extent.height];

        let viewport = colorimg.image_region().as_viewport();

        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            })
            .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .image_view(depthview.view)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE);

        let color_attachments = vk::RenderingAttachmentInfo::default()
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.1, 0.2, 0.4, 1.0],
                },
            })
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image_view(colorview.view)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE);

        let render_info = vk::RenderingInfo::default()
            .render_area(render_area)
            .layer_count(1)
            .depth_attachment(&depth_attachment)
            .color_attachments(core::slice::from_ref(&color_attachments));

        //start rendering
        unsafe {
            device
                .inner
                .cmd_begin_rendering(*command_buffer, &render_info);
            device.inner.cmd_bind_pipeline(
                *command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline.pipeline,
            );

            //set the viewport to always be _the whole image_
            device
                .inner
                .cmd_set_viewport(*command_buffer, 0, &[viewport]);
        }

        for batch in &self.batches {
            //setup the scissors for this call
            //TODO: actually do that?
            let mut scissors = batch.bound.clone();
            //NOTE: we constrain the scissors to the render area.
            scissors.extent.width = scissors.extent.width.min(render_area.extent.width);
            scissors.extent.height = scissors.extent.height.min(render_area.extent.height);

            //notify layer
            self.push.get_content_mut().layer_depth = batch.layer_depth;
            //update push const
            self.push.get_content_mut().cmd_buffer = batch.resource_handle.unwrap();

            unsafe {
                device
                    .inner
                    .cmd_set_scissor(*command_buffer, 0, &[scissors]);

                device.inner.cmd_push_constants(
                    *command_buffer,
                    self.pipeline.layout.layout,
                    vk::ShaderStageFlags::ALL,
                    0,
                    self.push.content_as_bytes(),
                );

                device
                    .inner
                    .cmd_draw(*command_buffer, 6, batch.count as u32, 0, 0);
            }
        }

        //end rendering
        unsafe {
            device.inner.cmd_end_rendering(*command_buffer);
        }
    }
}
