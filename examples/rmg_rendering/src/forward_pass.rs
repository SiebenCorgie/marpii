use easy_gltf::Scene;
use marpii::{
    allocator::MemoryUsage,
    ash::vk,
    context::Device,
    offset_of,
    resources::{
        BufDesc, GraphicsPipeline, Image, ImageType, ImgDesc, PipelineLayout, PushConstant,
        ShaderModule, ShaderStage,
    },
    util::Timestamps,
    OoS,
};
use marpii_rmg::{
    BufferHandle, CtxRmg, ImageHandle, RecordError, ResourceRegistry, Resources, Rmg, RmgError,
    Task,
};
use marpii_rmg_tasks::UploadBuffer;
use shared::{ResourceHandle, SimObj, Ubo, Vertex};
use std::sync::Arc;

use crate::{model_loading::load_model, OBJECT_COUNT};

const SHADER_VS: &[u8] = include_bytes!("../resources/forward_vs.spv");
const SHADER_FS: &[u8] = include_bytes!("../resources/forward_fs.spv");

pub struct ForwardPass {
    //    attdesc: AttachmentDescription,
    pub color_image: ImageHandle,
    depth_image: ImageHandle,

    pub sim_src: Option<BufferHandle<SimObj>>,

    /// the framebuffer extent that should be used.
    pub target_img_ext: vk::Extent2D,

    pipeline: Arc<GraphicsPipeline>,
    push: PushConstant<shared::ForwardPush>,

    ///VertexBuffer we are using to draw objects
    vertex_buffer: BufferHandle<Vertex>,
    index_buffer: BufferHandle<u32>,
    index_buffer_size: u32,

    //Camera data used
    ubo_buffer: BufferHandle<Ubo>,

    //Using this to query performance at runtime
    timestamps: Timestamps,
}

impl ForwardPass {
    pub fn new(rmg: &mut Rmg, ubo: BufferHandle<Ubo>, gltf: &[Scene]) -> Result<Self, RmgError> {
        let push = PushConstant::new(
            shared::ForwardPush {
                ubo: ResourceHandle::new(0, 0),
                sim: ResourceHandle::new(0, 0),
                pad: [0u32; 2],
            },
            vk::ShaderStageFlags::COMPUTE,
        );

        let (vertex_buffer_data, index_buffer_data) = load_model(gltf);

        let index_buffer_size = index_buffer_data.len() as u32;

        let mut ver_upload = UploadBuffer::new_with_buffer(
            rmg,
            &vertex_buffer_data,
            BufDesc::for_data::<Vertex>(vertex_buffer_data.len()).with(|b| {
                b.usage = vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::VERTEX_BUFFER
            }),
        )?;
        let mut ind_upload = UploadBuffer::new_with_buffer(
            rmg,
            &index_buffer_data,
            BufDesc::for_data::<u32>(index_buffer_data.len()).with(|b| {
                b.usage = vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::INDEX_BUFFER
            }),
        )?;
        rmg.record()
            .add_task(&mut ver_upload)
            .unwrap()
            .add_task(&mut ind_upload)
            .unwrap()
            .execute()?;

        let vertex_buffer = ver_upload.buffer;
        let index_buffer = ind_upload.buffer;

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
            ImgDesc {
                extent: vk::Extent3D {
                    width: 1,
                    height: 1,
                    depth: 1,
                },
                format: color_format,
                img_type: ImageType::Tex2d,
                tiling: vk::ImageTiling::OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::STORAGE,
                ..Default::default()
            },
            Some("target img"),
        )?;
        let mut depth_desc = ImgDesc::depth_attachment_2d(1, 1, depth_format);
        depth_desc.usage |= vk::ImageUsageFlags::SAMPLED;
        let depth_image = rmg.new_image_uninitialized(depth_desc, None)?;

        //No additional descriptors for us
        let layout = rmg.resources.bindless_layout();

        let shader_module_vert = ShaderModule::new_from_bytes(&rmg.ctx.device, SHADER_VS).unwrap();

        let shader_module_frag = ShaderModule::new_from_bytes(&rmg.ctx.device, SHADER_FS).unwrap();

        let vertex_shader_stage = ShaderStage::from_module(
            shader_module_vert.into(),
            vk::ShaderStageFlags::VERTEX,
            "main".to_owned(),
        );

        let fragment_shader_stage = ShaderStage::from_module(
            shader_module_frag.into(),
            vk::ShaderStageFlags::FRAGMENT,
            "main".to_owned(),
        );

        let pipeline = Arc::new(
            Self::forward_pipeline(
                &rmg.ctx.device,
                OoS::new_shared(layout),
                &[vertex_shader_stage, fragment_shader_stage],
                &[color_format],
                depth_format,
            )
            .unwrap(),
        );

        let timestamps = Timestamps::new(&rmg.ctx.device, 2)?;

        Ok(ForwardPass {
            color_image,
            depth_image,
            sim_src: None,
            target_img_ext: vk::Extent2D::default(),

            pipeline,
            push,

            index_buffer,
            index_buffer_size,
            vertex_buffer,
            ubo_buffer: ubo,
            timestamps,
        })
    }

    pub fn forward_pipeline(
        device: &Arc<Device>,
        pipeline_layout: impl Into<OoS<PipelineLayout>>,
        shader_stages: &[ShaderStage],
        color_formats: &[vk::Format],
        depth_format: vk::Format,
    ) -> Result<GraphicsPipeline, anyhow::Error> {
        let color_blend_attachments = vk::PipelineColorBlendAttachmentState::default()
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true);

        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .blend_constants([0.0; 4])
            .attachments(core::slice::from_ref(&color_blend_attachments)); //only the color target

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_compare_op(vk::CompareOp::LESS)
            .depth_write_enable(true)
            .depth_test_enable(true)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false);

        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        //no other dynamic state

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .primitive_restart_enable(false)
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

        let vertex_binding_desc = vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(core::mem::size_of::<shared::Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);
        let vertex_attrib_desc = [
            //Description of the Pos attribute
            vk::VertexInputAttributeDescription::default()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(offset_of!(shared::Vertex, position) as u32)
                .build(),
            //Description of the Normal attribute
            vk::VertexInputAttributeDescription::default()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(offset_of!(shared::Vertex, normal) as u32)
                .build(),
            //Description of the uv attribute
            vk::VertexInputAttributeDescription::default()
                .location(2)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(shared::Vertex, uv) as u32)
                .build(),
        ];
        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(core::slice::from_ref(&vertex_binding_desc))
            .vertex_attribute_descriptions(&vertex_attrib_desc);

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
        let color_format = resources.get_image_desc(&self.color_image).format;
        let depth_format = resources.get_image_desc(&self.depth_image).format;

        self.color_image = resources.add_image(Arc::new(
            Image::new(
                &ctx.device,
                &ctx.allocator,
                ImgDesc {
                    extent: vk::Extent3D {
                        width: self.target_img_ext.width,
                        height: self.target_img_ext.height,
                        depth: 1,
                    },

                    format: color_format,
                    img_type: ImageType::Tex2d,
                    tiling: vk::ImageTiling::OPTIMAL,
                    usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::STORAGE,
                    ..Default::default()
                },
                MemoryUsage::GpuOnly,
                None,
            )
            .map_err(|e| RecordError::MarpiiError(e.into()))?,
        ))?;
        self.depth_image = resources.add_image(Arc::new(
            Image::new(
                &ctx.device,
                &ctx.allocator,
                ImgDesc::depth_attachment_2d(
                    self.target_img_ext.width,
                    self.target_img_ext.height,
                    depth_format,
                ),
                MemoryUsage::GpuOnly,
                None,
            )
            .map_err(|e| RecordError::MarpiiError(e.into()))?,
        ))?;

        Ok(())
    }
}

impl Task for ForwardPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        if let Some(buf) = &self.sim_src {
            registry
                .request_buffer(
                    buf,
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    vk::AccessFlags2::empty(),
                )
                .unwrap();
        }
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
                vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            )
            .unwrap();
        registry
            .request_buffer(
                &self.vertex_buffer,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                vk::AccessFlags2::VERTEX_ATTRIBUTE_READ,
            )
            .unwrap();
        registry
            .request_buffer(
                &self.index_buffer,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                vk::AccessFlags2::INDEX_READ,
            )
            .unwrap();
        registry
            .request_buffer(
                &self.ubo_buffer,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                vk::AccessFlags2::SHADER_READ,
            )
            .unwrap();
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
        if self.target_img_ext != img_ext {
            self.flip_target_buffer(resources, ctx)?;
        }

        self.push.get_content_mut().ubo =
            resources.resource_handle_or_bind(self.ubo_buffer.clone())?;
        self.push.get_content_mut().sim =
            resources.resource_handle_or_bind(self.sim_src.as_ref().unwrap())?;
        Ok(())
    }

    fn record(
        &mut self,
        device: &Arc<Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        self.timestamps.reset(command_buffer).unwrap();

        self.timestamps
            .write_timestamp(command_buffer, vk::PipelineStageFlags2::TOP_OF_PIPE, 0);

        if self.sim_src.is_none() {
            return;
        }

        //1. transform attachment images
        //2. Schedule draw op
        //3. transform attachments back

        let (colorimg, colorview) = {
            let img_access = resources.get_image_state(&self.color_image);
            (img_access.image.clone(), img_access.view.clone())
        };

        let depthview = resources.get_image_state(&self.depth_image).view.clone();

        let vertex_buffer_access = resources.get_buffer_state(&self.vertex_buffer);
        let index_buffer_access = resources.get_buffer_state(&self.index_buffer);

        let viewport = colorimg.image_region().as_viewport();
        let scissors = colorimg.image_region().as_rect_2d();
        let depth_attachment = vk::RenderingAttachmentInfo::default()
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

        let color_attachments = vk::RenderingAttachmentInfo::default()
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.1, 0.2, 0.4, 1.0],
                },
            })
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image_view(colorview.view)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE);

        let render_info = vk::RenderingInfo::default()
            .depth_attachment(&depth_attachment)
            .render_area(scissors)
            .layer_count(1)
            .color_attachments(core::slice::from_ref(&color_attachments));

        unsafe {
            //DRAW STEP
            device
                .inner
                .cmd_begin_rendering(*command_buffer, &render_info);
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

            device
                .inner
                .cmd_set_viewport(*command_buffer, 0, &[viewport]);
            device
                .inner
                .cmd_set_scissor(*command_buffer, 0, &[scissors]);

            device.inner.cmd_bind_vertex_buffers(
                *command_buffer,
                0,
                &[vertex_buffer_access.buffer.inner],
                &[0],
            );

            device.inner.cmd_bind_index_buffer(
                *command_buffer,
                index_buffer_access.buffer.inner,
                0,
                vk::IndexType::UINT32,
            );

            device.inner.cmd_draw_indexed(
                *command_buffer,
                self.index_buffer_size,
                OBJECT_COUNT as u32,
                0,
                0,
                0,
            );

            device.inner.cmd_end_rendering(*command_buffer);
        }

        self.timestamps
            .write_timestamp(command_buffer, vk::PipelineStageFlags2::BOTTOM_OF_PIPE, 1);
    }

    fn post_execution(
        &mut self,
        _resources: &mut Resources,
        _ctx: &CtxRmg,
    ) -> Result<(), RecordError> {
        if let Ok(result) = self.timestamps.get_timestamps() {
            match (result[0], result[1]) {
                (Some(src), Some(dst)) => {
                    let diff = dst - src;
                    let ms =
                        (diff as f32 * self.timestamps.get_timestamp_increment()) / 1_000_000.0;
                    println!("Forward local: {}ms", ms);
                }
                _ => println!("State: {}, {}", result[0].is_some(), result[1].is_some()),
            }
        }

        Ok(())
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }
    fn name(&self) -> &'static str {
        "ForwardPass"
    }
}
