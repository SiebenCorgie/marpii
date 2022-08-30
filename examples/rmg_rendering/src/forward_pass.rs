use glam::{Quat, Vec3};
use marpii::ash;
use marpii::{
    allocator::{Allocator, MemoryUsage},
    ash::vk::{self, Extent2D, Extent3D, PipelineColorBlendAttachmentState},
    context::{Ctx, Device},
    offset_of,
    resources::{
        GraphicsPipeline, Image, ImageType, ImageView, ImgDesc, PipelineLayout, PushConstant,
        SafeImageView, ShaderModule, ShaderStage,
    },
};
use marpii_commands::buffer_from_data;
use marpii_descriptor::bindless::{ResourceHandle, SampledImageHandle};
use marpii_rmg::resources::BufferHdl;
use std::sync::{Arc, Mutex};

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct ForwardPush {
    rotation: [f32; 4],
    location: [f32; 4],
    texture_indices: [u32; 4],
}
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub tex_coords: [f32; 2],
}

pub struct Mesh {
    index_count: u32,
    vertex_buffer: BufferHdl<Vertex>,
    index_buffer: BufferHdl<u32>,

    albedo_texture: Option<SampledImageHandle>,
    normal_texture: Option<SampledImageHandle>,
    roughness_metallness_texture: Option<SampledImageHandle>,
}

impl Mesh {
/*
    ///Uploads vertex and index buffer
    pub fn from_vertex_index_buffers<A: Allocator + Send + Sync + 'static>(
        ctx: &Ctx<A>,
        vertex_buffer: &[easy_gltf::model::Vertex],
        index_buffer: &[usize],
        albedo_texture: Option<SampledImageHandle>,
        normal_texture: Option<SampledImageHandle>,
        roughness_metallness_texture: Option<SampledImageHandle>,
    ) -> Self {
        let vertex_buffer: Vec<Vertex> = vertex_buffer
            .into_iter()
            .map(|v| Vertex {
                position: v.position.into(),
                normal: v.normal.into(),
                tangent: v.tangent.into(),
                tex_coords: v.tex_coords.into(),
            })
            .collect();
        let index_buffer: Vec<u32> = index_buffer.into_iter().map(|c| *c as u32).collect();
        let index_count = index_buffer.len() as u32;
        let upload_queue = ctx
            .device
            .first_queue_for_attribute(true, false, true)
            .unwrap();

        let vertex_buffer = buffer_from_data(
            &ctx.device,
            &ctx.allocator,
            &upload_queue,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            Some("VertexBuffer"),
            None,
            &vertex_buffer,
        )
        .unwrap();

        let index_buffer = buffer_from_data(
            &ctx.device,
            &ctx.allocator,
            &upload_queue,
            vk::BufferUsageFlags::INDEX_BUFFER,
            Some("VertexBuffer"),
            None,
            &index_buffer,
        )
        .unwrap();

        let vertex_buffer = StBuffer::shared(
            Arc::new(vertex_buffer),
            upload_queue.family_index,
            vk::AccessFlags::TRANSFER_WRITE,
        );
        let index_buffer = StBuffer::shared(
            Arc::new(index_buffer),
            upload_queue.family_index,
            vk::AccessFlags::TRANSFER_WRITE,
        );

        Mesh {
            index_count,
            index_buffer,
            vertex_buffer,
            albedo_texture,
            normal_texture,
            roughness_metallness_texture,
        }
    }

    fn get_texture_inidces(&self) -> [u32; 4] {
        [
            self.albedo_texture
                .map(|i| i.0 .0)
                .unwrap_or(ResourceHandle::UNDEFINED_HANDLE),
            self.normal_texture
                .map(|i| i.0 .0)
                .unwrap_or(ResourceHandle::UNDEFINED_HANDLE),
            self.roughness_metallness_texture
                .map(|i| i.0 .0)
                .unwrap_or(ResourceHandle::UNDEFINED_HANDLE),
            ResourceHandle::UNDEFINED_HANDLE,
        ]
    }
*/
}

pub fn forward_pipeline(
    device: &Arc<Device>,
    pipeline_layout: PipelineLayout,
    shader_stages: &[ShaderStage],
    color_formats: &[vk::Format],
    depth_format: vk::Format,
) -> Result<GraphicsPipeline, anyhow::Error> {
    let color_blend_attachments = PipelineColorBlendAttachmentState::builder()
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

    let vertex_binding_desc = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(core::mem::size_of::<Vertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);
    let vertex_attrib_desc = [
        //Description of the Pos attribute
        vk::VertexInputAttributeDescription::builder()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(offset_of!(Vertex, position) as u32)
            .build(),
        //Description of the Normal attribute
        vk::VertexInputAttributeDescription::builder()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(offset_of!(Vertex, normal) as u32)
            .build(),
        //Description of the tangent attribute
        vk::VertexInputAttributeDescription::builder()
            .location(2)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(offset_of!(Vertex, tangent) as u32)
            .build(),
        //Description of the UV attribute
        vk::VertexInputAttributeDescription::builder()
            .location(3)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(offset_of!(Vertex, tex_coords) as u32)
            .build(),
    ];
    let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(core::slice::from_ref(&vertex_binding_desc))
        .vertex_attribute_descriptions(&vertex_attrib_desc);
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

pub struct ForwardPass {
    graphics_pipeline: Arc<GraphicsPipeline>,
    push_constant: Arc<Mutex<PushConstant<ForwardPush>>>,

    dynamic_rendering: ash::extensions::khr::DynamicRendering,

    pub objects: Vec<Mesh>,
}
/*
impl ForwardPass {
    pub fn new<A: Allocator + Send + Sync + 'static>(
        ctx: &Ctx<A>,
        window_ext: Extent2D,
        pipeline_layout: PipelineLayout,
    ) -> Result<Self, anyhow::Error> {
        let target_color = StImage::unitialized(Image::new(
            &ctx.device,
            &ctx.allocator,
            ImgDesc {
                extent: Extent3D {
                    width: window_ext.width,
                    height: window_ext.height,
                    depth: 1,
                },
                format: ctx
                    .device
                    .select_format(
                        vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
                        vk::ImageTiling::OPTIMAL,
                        &[
                            vk::Format::R16G16B16A16_SFLOAT,
                            vk::Format::R32G32B32A32_SFLOAT,
                            vk::Format::R8G8B8A8_UNORM,
                        ],
                    )
                    .unwrap(),
                img_type: ImageType::Tex2d,
                tiling: vk::ImageTiling::OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
                ..Default::default()
            },
            MemoryUsage::GpuOnly,
            Some("TargetImage"),
            None,
        )?);

        let color_view = target_color
            .image()
            .view(&ctx.device, target_color.image().view_all())?;

        let target_depth = StImage::unitialized(Image::new(
            &ctx.device,
            &ctx.allocator,
            ImgDesc {
                extent: Extent3D {
                    width: window_ext.width,
                    height: window_ext.height,
                    depth: 1,
                },
                format: ctx
                    .device
                    .select_format(
                        vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                            | vk::ImageUsageFlags::TRANSFER_SRC,
                        vk::ImageTiling::OPTIMAL,
                        &[
                            vk::Format::D32_SFLOAT,
                            vk::Format::D24_UNORM_S8_UINT,
                            vk::Format::D16_UNORM,
                        ],
                    )
                    .unwrap(),
                img_type: ImageType::Tex2d,
                tiling: vk::ImageTiling::OPTIMAL,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC,
                ..Default::default()
            },
            MemoryUsage::GpuOnly,
            Some("DepthImage"),
            None,
        )?);

        let depth_view = target_depth
            .image()
            .view(&ctx.device, target_depth.image().view_all())?;

        let assumed = [
            AssumedState::Image {
                image: target_color.clone(),
                state: ImageState {
                    access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                },
            },
            AssumedState::Image {
                image: target_depth.clone(),
                state: ImageState {
                    access_mask: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    layout: vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                },
            },
        ];

        //Now setup the graphics pipeline
        let push = Arc::new(Mutex::new(PushConstant::new(
            ForwardPush {
                location: [0.0; 4],
                rotation: [0.0; 4],
                texture_indices: [ResourceHandle::UNDEFINED_HANDLE; 4],
            },
            vk::ShaderStageFlags::ALL,
        )));
        //load shader from file
        let shader_module = Arc::new(
            ShaderModule::new_from_file(&ctx.device, "resources/vertex_graphics_shader.spv")
                .unwrap(),
        );
        /*
        let pipeline_layout = PipelineLayout::from_layout_push(
            &ctx.device,
            &shader_module.create_descriptor_set_layouts()?,
            &push.lock().unwrap(), //no push atm
        )
        .unwrap();
        */
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

        let pipeline = forward_pipeline(
            &ctx.device,
            pipeline_layout,
            &[vertex_shader_stage, fragment_shader_stage],
            &[target_color.image().desc.format],
            target_depth.image().desc.format,
        )
        .unwrap();

        let dynamic_rendering =
            ash::extensions::khr::DynamicRendering::new(&ctx.instance.inner, &ctx.device.inner);

        Ok(ForwardPass {
            dynamic_rendering,
            push_constant: push,
            graphics_pipeline: Arc::new(pipeline),
            assumed,
            target_color,
            color_view: Arc::new(color_view),
            target_depth,
            depth_view: Arc::new(depth_view),
            objects: Vec::new(),
        })
    }

    pub fn push_camera(&self, cam_location: Vec3, cam_rotation: Quat) {
        let aspect_ratio = {
            let ext = self.target_color.image().extent_2d();
            ext.width as f32 / ext.height as f32
        };
        self.push_constant
            .lock()
            .unwrap()
            .get_content_mut()
            .location = cam_location.extend(aspect_ratio).into();
        self.push_constant
            .lock()
            .unwrap()
            .get_content_mut()
            .rotation = cam_rotation.to_array();
    }
}
impl Pass for ForwardPass {
    fn assumed_states(&self) -> &[marpii_command_graph::pass::AssumedState] {
        &self.assumed
    }

    fn record(
        &mut self,
        command_buffer: &mut marpii_commands::Recorder,
    ) -> Result<(), anyhow::Error> {
        //Since we are using dynamic rendering, drawing is as easy as
        //starting the pass via a vk::RenderingInfo, setting up clears and finishing it of

        let color_view = self.color_view.clone();
        let depth_view = self.depth_view.clone();
        let pipeline = self.graphics_pipeline.clone();
        let push = self.push_constant.clone();
        let viewport = self.target_color.image().image_region().as_viewport();
        let scissors = self.target_color.image().image_region().as_rect_2d();

        let meshes = self.objects.clone();
        //Not correct, but working.
        let dyn_rendering = self.dynamic_rendering.clone();

        command_buffer.record(move |dev, cmd| {
            let depth_attachment = vk::RenderingAttachmentInfo::builder()
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                })
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .image_view(depth_view.view)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE);

            let color_attachments = vk::RenderingAttachmentInfo::builder()
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.1, 0.2, 0.4, 1.0],
                    },
                })
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .image_view(color_view.view)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE);

            let render_info = vk::RenderingInfoKHR::builder()
                .depth_attachment(&depth_attachment)
                .render_area(scissors)
                .layer_count(1)
                .color_attachments(core::slice::from_ref(&color_attachments));

            unsafe {
                dyn_rendering.cmd_begin_rendering(*cmd, &render_info);
                //dev.cmd_begin_rendering(*cmd, &render_info);
                //bind pipline and push consts

                dev.cmd_set_viewport(*cmd, 0, &[viewport]);
                dev.cmd_set_scissor(*cmd, 0, &[scissors]);

                dev.cmd_bind_pipeline(*cmd, vk::PipelineBindPoint::GRAPHICS, pipeline.pipeline);
            }

            let mut push_lock = push.lock().unwrap();

            for mesh in meshes.iter() {
                //Setup push constant
                push_lock.get_content_mut().texture_indices = mesh.get_texture_inidces();
                unsafe {
                    dev.cmd_push_constants(
                        *cmd,
                        pipeline.layout.layout,
                        vk::ShaderStageFlags::ALL,
                        0,
                        push_lock.content_as_bytes(),
                    );

                    dev.cmd_bind_index_buffer(
                        *cmd,
                        mesh.index_buffer.buffer().inner,
                        0,
                        vk::IndexType::UINT32,
                    );
                    dev.cmd_bind_vertex_buffers(
                        *cmd,
                        0,
                        &[mesh.vertex_buffer.buffer().inner],
                        &[0],
                    );
                    dev.cmd_draw_indexed(*cmd, mesh.index_count, 1, 0, 0, 0);
                }
            }

            //dev.cmd_end_rendering(*cmd);
            unsafe { dyn_rendering.cmd_end_rendering(*cmd) };
        });

        Ok(())
    }
}
*/
