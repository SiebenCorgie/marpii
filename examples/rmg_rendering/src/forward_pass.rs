use marpii::{
    allocator::MemoryUsage,
    ash::vk,
    context::Device,
    resources::{Image, ImgDesc, PushConstant, ShaderModule, PipelineLayout, ShaderStage, GraphicsPipeline, ImageType}, util::OoS,
};
use marpii_rmg::{CtxRmg, ResourceRegistry, Resources, Rmg, RmgError, Task, BufferHandle, ImageHandle};
use shared::{ResourceHandle, SimObj};
use std::sync::Arc;

use crate::OBJECT_COUNT;

pub struct ForwardPass {
    //    attdesc: AttachmentDescription,

    pub color_image: ImageHandle,
    depth_image: ImageHandle,

    pub sim_src: Option<BufferHandle<SimObj>>,

    target_img_ext: vk::Extent2D,

    pipeline: Arc<GraphicsPipeline>,
    push: PushConstant<shared::ForwardPush>,
}


impl ForwardPass {
    pub fn new(rmg: &mut Rmg) -> Result<Self, RmgError> {
        println!("Setup Forward");
        let push = PushConstant::new(
            shared::ForwardPush {
                buf: ResourceHandle::new(0, 0),
                buffer_size: OBJECT_COUNT as u32,
                pad: [0; 2],
            },
            vk::ShaderStageFlags::COMPUTE,
        );

        let color_format = rmg
            .ctx
            .device
            .select_format(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_SRC,
                vk::ImageTiling::OPTIMAL,
                &[
                    vk::Format::R16G16B16A16_SFLOAT,
                    vk::Format::R32G32B32A32_SFLOAT,
                    vk::Format::R8G8B8A8_UNORM,
                ],
            )
            .unwrap();

        let depth_format = rmg
            .ctx
            .device
            .select_format(
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                vk::ImageTiling::OPTIMAL,
                &[
                    vk::Format::D32_SFLOAT,
                    vk::Format::D24_UNORM_S8_UINT,
                    vk::Format::D16_UNORM,
                ],
            )
            .unwrap();

        let color_image = rmg.new_image_uninitialized(
            ImgDesc{
                extent: vk::Extent3D{width: 1, height: 1, depth: 1},
                format: color_format,
                img_type: ImageType::Tex2d,
                tiling: vk::ImageTiling::OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::STORAGE,
                ..Default::default()
            },
            Some("target img"),
        )?;
        let mut depth_desc = ImgDesc::depth_attachment_2d(1, 1, depth_format);
        depth_desc.usage |= vk::ImageUsageFlags::SAMPLED;
        let depth_image = rmg.new_image_uninitialized(
            depth_desc,
            None
        )?;

        //No additional descriptors for us
        let layout = rmg.resources().bindless_pipeline_layout(&[]);

        let shader_module = Arc::new(
            ShaderModule::new_from_file(&rmg.ctx.device, "resources/rmg_shader.spv")
                .unwrap(),
        );
        let vertex_shader_stage = ShaderStage::from_shared_module(
            shader_module.clone(),
            vk::ShaderStageFlags::VERTEX,
            "main_vs".to_owned(),
        );

        let fragment_shader_stage = ShaderStage::from_shared_module(
            shader_module.clone(),
            vk::ShaderStageFlags::FRAGMENT,
            "main_fs".to_owned(),
        );

        let pipeline = Arc::new(Self::forward_pipeline(
            &rmg.ctx.device,
            layout,
            &[vertex_shader_stage, fragment_shader_stage],
            &[color_format],
            depth_format,
        )
            .unwrap());

        Ok(ForwardPass {
            color_image,
            depth_image,
            sim_src: None,
            target_img_ext: vk::Extent2D::default(),

            pipeline,
            push,
        })
    }


    pub fn forward_pipeline(
        device: &Arc<Device>,
        pipeline_layout: impl Into<OoS<PipelineLayout>>,
        shader_stages: &[ShaderStage],
        color_formats: &[vk::Format],
        depth_format: vk::Format,
    ) -> Result<GraphicsPipeline, anyhow::Error> {
        let color_blend_attachments = vk::PipelineColorBlendAttachmentState::builder()
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true);

        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
            .blend_constants([0.0; 4])
            .attachments(core::slice::from_ref(&color_blend_attachments)); //only the color target

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_compare_op(vk::CompareOp::LESS)
            .depth_write_enable(true)
            .depth_test_enable(true)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false);

        let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        //no other dynamic state

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .primitive_restart_enable(false)
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let multisample_state = vk::PipelineMultisampleStateCreateInfo::builder()
            .min_sample_shading(1.0)
            .alpha_to_one_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::builder()
            .cull_mode(vk::CullModeFlags::NONE)
            .depth_bias_enable(false)
            .depth_clamp_enable(false)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0)
            .polygon_mode(vk::PolygonMode::FILL);
        let tesselation_state = vk::PipelineTessellationStateCreateInfo::builder();

        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewport_count(1)
            .scissor_count(1);

        /*
        let vertex_binding_desc = vk::VertexInputBindingDescription::builder();
            .binding(0)
            .stride(core::mem::size_of::<shared::Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);
        let vertex_attrib_desc = [
            //Description of the Pos attribute
            vk::VertexInputAttributeDescription::builder()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(offset_of!(shared::Vertex, position) as u32)
                .build(),
            //Description of the Normal attribute
            vk::VertexInputAttributeDescription::builder()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(offset_of!(shared::Vertex, normal) as u32)
                .build(),
            //Description of the tangent attribute
            vk::VertexInputAttributeDescription::builder()
                .location(2)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(shared::Vertex, uv) as u32)
                .build(),

        ];
        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(core::slice::from_ref(&vertex_binding_desc))
            .vertex_attribute_descriptions(&vertex_attrib_desc);
        */

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo {
            vertex_attribute_description_count: 0,
            vertex_binding_description_count: 0,
            ..Default::default()
        };
        let create_info = vk::GraphicsPipelineCreateInfo::builder()
            .color_blend_state(&color_blend_state)
            .depth_stencil_state(&depth_stencil_state)
            .dynamic_state(&dynamic_state)
            .input_assembly_state(&input_assembly_state)
            .multisample_state(&multisample_state)
            .rasterization_state(&rasterization_state)
            .viewport_state(&viewport_state)
            .tessellation_state(&tesselation_state)
            .vertex_input_state(&vertex_input_state);
        let pipeline = GraphicsPipeline::new_dynamic_pipeline(
            device,
            create_info,
            pipeline_layout,
            shader_stages,
            color_formats,
            depth_format,
        )?;
        Ok(pipeline)
    }

    fn flip_target_buffer(
        &mut self,
        resources: &mut Resources,
        ctx: &CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        println!(
            "Renewing target image for -> {:?}!",
            resources.get_surface_extent()
        );
        let color_format = resources.get_image_desc(&self.color_image).format;
        let depth_format = resources.get_image_desc(&self.depth_image).format;

        self.color_image = resources.add_image(Arc::new(Image::new(
            &ctx.device,
            &ctx.allocator,
            ImgDesc{
                extent: vk::Extent3D{width: self.target_img_ext.width, height: self.target_img_ext.height, depth: 1},
                format: color_format,
                img_type: ImageType::Tex2d,
                tiling: vk::ImageTiling::OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::STORAGE,
                ..Default::default()
            },
            MemoryUsage::GpuOnly,
            None,
            None
        )?))?;
        self.depth_image = resources.add_image(Arc::new(Image::new(
            &ctx.device,
            &ctx.allocator,
            ImgDesc::depth_attachment_2d(
                self.target_img_ext.width,
                self.target_img_ext.height,
                depth_format
            ),
            MemoryUsage::GpuOnly,
            None,
            None
        )?))?;

        Ok(())
    }
}

impl Task for ForwardPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        if let Some(buf) = &self.sim_src {
            registry.request_buffer(buf);
        }
        registry.request_image(&self.color_image);
        registry.request_image(&self.depth_image);
        registry.register_asset(self.pipeline.clone());
    }

    fn pre_record(
        &mut self,
        resources: &mut Resources,
        ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        let img_ext = {
            let desc = resources.get_image_desc(&self.color_image);
            vk::Extent2D {
                width: desc.extent.width,
                height: desc.extent.height,
            }
        };
        if resources.get_surface_extent() != img_ext {
            self.target_img_ext = resources.get_surface_extent();
            self.flip_target_buffer(resources, ctx)?;
        }

        self.push.get_content_mut().buf = resources.get_resource_handle(self.sim_src.as_ref().unwrap())?;
        Ok(())
    }

    fn post_execution(
        &mut self,
        resources: &mut Resources,
        ctx: &CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        self.flip_target_buffer(resources, ctx)
    }

    fn record(
        &mut self,
        device: &Arc<Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {

        //1. transform attachment images
        //2. Schedule draw op
        //3. transform attachments back

        let (color_before_access, color_before_layout, colorimg, colorview) = {
            let img_access = resources.get_image_state(&self.color_image);
            (img_access.mask, img_access.layout, img_access.image.clone(), img_access.view.clone())
        };

        let (depth_before_access, depth_before_layout, depthimg, depthview) = {
            let img_access = resources.get_image_state(&self.depth_image);
            (img_access.mask, img_access.layout, img_access.image.clone(), img_access.view.clone() )
        };


        let viewport = colorimg.image_region().as_viewport();
        let scissors = colorimg.image_region().as_rect_2d();
        let depth_attachment = vk::RenderingAttachmentInfo::builder()
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                })
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .image_view(depthview.view)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE);

            let color_attachments = vk::RenderingAttachmentInfo::builder()
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.1, 0.2, 0.4, 1.0],
                    },
                })
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .image_view(colorview.view)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE);

            let render_info = vk::RenderingInfo::builder()
                .depth_attachment(&depth_attachment)
                .render_area(scissors)
                .layer_count(1)
                .color_attachments(core::slice::from_ref(&color_attachments));

        unsafe {
            device.inner.cmd_pipeline_barrier2(
                *command_buffer,
                &vk::DependencyInfo::builder().image_memory_barriers(&[
                    //src image
                    *vk::ImageMemoryBarrier2::builder()
                        .image(colorimg.inner)
                        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .src_access_mask(color_before_access)
                        .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                        .subresource_range(colorimg.subresource_all())
                        .old_layout(color_before_layout)
                        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
                    //swapchain image
                    *vk::ImageMemoryBarrier2::builder()
                        .image(depthimg.inner)
                        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .src_access_mask(depth_before_access)
                        .dst_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
                        .subresource_range(depthimg.subresource_all())
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL),
                ]),
            );


            //DRAW STEP
            device.inner.cmd_begin_rendering(*command_buffer, &render_info);
            device.inner.cmd_bind_pipeline(
                *command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline.pipeline,
            );
            device.inner.cmd_push_constants(
                *command_buffer,
                self.pipeline.layout.layout,
                vk::ShaderStageFlags::ALL,
                0,
                self.push.content_as_bytes(),
            );

            device.inner.cmd_set_viewport(*command_buffer, 0, &[viewport]);
            device.inner.cmd_set_scissor(*command_buffer, 0, &[scissors]);

            device.inner.cmd_draw(*command_buffer, 3, OBJECT_COUNT as u32, 0, 0);

            device.inner.cmd_end_rendering(*command_buffer);

            //TRANFORM BACK into initial state
            device.inner.cmd_pipeline_barrier2(
                *command_buffer,
                &vk::DependencyInfo::builder().image_memory_barriers(&[
                    //src image
                    *vk::ImageMemoryBarrier2::builder()
                        .image(colorimg.inner)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .dst_access_mask(color_before_access)
                        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                        .subresource_range(colorimg.subresource_all())
                        .new_layout(color_before_layout)
                        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
                    //swapchain image
                    *vk::ImageMemoryBarrier2::builder()
                        .image(depthimg.inner)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .dst_access_mask(depth_before_access)
                        .src_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
                        .subresource_range(depthimg.subresource_all())
                        .new_layout(depth_before_layout)
                        .old_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL),
                ]),
            );
        }
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }
    fn name(&self) -> &'static str {
        "ForwardPass"
    }
}
