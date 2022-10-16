//! # Egui integration
//!
//! Renders an user defined egui to its inner target image. Note that the `target` image uses an alpha channel. Therefore, the image can easily be
//! rendered on top of an existing image.
//!
//! Have a look at the egui example for an in depth integration.
//!

use crate::{DynamicImage, DynamicBuffer};
use crate::egui::{TextureId, ClippedPrimitive, TexturesDelta};
use egui_winit::egui::Pos2;
use egui_winit::winit::window::Window;
use egui_winit::winit::event_loop::EventLoopWindowTarget;
use fxhash::FxHashMap;
use marpii::ash::vk::{Offset3D, Extent3D, Rect2D};
use marpii::util::ImageRegion;
use marpii::{ash::vk, resources::{ImgDesc, ImageType, ShaderModule, ShaderStage, GraphicsPipeline, PushConstant, PipelineLayout, BufDesc}, context::Device, util::OoS, offset_of};
use marpii_rmg::{Rmg, RmgError, Task, ImageHandle};
use marpii_rmg_task_shared::{EGuiPush, ResourceHandle};
use std::sync::Arc;
use std::time::Instant;





///Single EGui primitve draw command
struct EGuiPrimDraw{
    ///Offset into the vertex buffer
    vertex_offset: u32,
    ///Offest into the index buffer
    index_offset: u32,

    vertex_buffer_size: u32,
    index_buffer_size: u32,

    clip: Rect2D
}

///Wrapper around the render task, takes care of winit events and let's you define
/// the UI for each frame.
pub struct EGuiWinitIntegration{
    //translation state
    state: egui_winit::State,
    start: Instant,
    egui_context: crate::egui::Context,
    renderer: EGuiRender,
}

impl EGuiWinitIntegration {
    pub fn new<T>(rmg: &mut Rmg, event_loop: &EventLoopWindowTarget<T>) -> Result<Self, RmgError>{

        Ok(EGuiWinitIntegration{
            state: egui_winit::State::new(event_loop),
            start: Instant::now(),
            egui_context: crate::egui::Context::default(),
            renderer: EGuiRender::new(rmg)?
        })
    }
    pub fn handle_event<T>(&mut self, event: &egui_winit::winit::event::Event<T>){
        if let egui_winit::winit::event::Event::WindowEvent { window_id, event } = event{
            let _is_exclusive = self.state.on_event(&self.egui_context, event);
        }
    }

    pub fn target_image(&self) -> &ImageHandle{
        &self.renderer.target_image
    }

    pub fn renderer(&mut self) -> &mut EGuiRender{
        &mut self.renderer
    }

    ///runs `run_ui` on this context. Use the closure to encode the next "to be rendered" egui.
    pub fn run(&mut self, rmg: &mut Rmg, window: &Window, run_ui: impl FnOnce(&egui_winit::egui::Context)) -> Result<(), RmgError>{

        let mut raw_input = self.state.take_egui_input(window);
        raw_input.time = Some(self.start.elapsed().as_secs_f64());
        let resolution = vk::Extent2D{
            width: window.inner_size().width,
            height: window.inner_size().height
        };
        raw_input.screen_rect = Some(egui_winit::egui::Rect{
            min: Pos2::ZERO,
            max: Pos2::new(resolution.width as f32, resolution.height as f32)
        });

        self.state.set_pixels_per_point(window.scale_factor() as f32);
        self.renderer.px_per_point = window.scale_factor() as f32;


        let output = self.egui_context.run(raw_input, run_ui);

        self.state.handle_platform_output(window, &self.egui_context, output.platform_output);
        let primitives = self.egui_context.tessellate(output.shapes);
        self.renderer.set_resolution(rmg, resolution)?;
        self.renderer.set_primitives(rmg, primitives)?;
        self.renderer.set_texture_deltas(rmg, output.textures_delta)?;
        Ok(())
    }
}

///Egui render task. Make sure to supply the renderer with all `winit` events that should be taken into account, or use [EGuiIntegration] instead.
pub struct EGuiRender{
    //NOTE: egui uses three main resources to render its interface. A texture atlas, and a vertex/index buffer changing at a high rate
    //      we take our own DynamicBuffer and DynamicImage for those tasks.
    atlas: FxHashMap<TextureId, DynamicImage>,
    vertex_buffer: DynamicBuffer<crate::egui::epaint::Vertex>,
    index_buffer: DynamicBuffer<u32>,

    commands: Vec<EGuiPrimDraw>,

    px_per_point: f32,

    //target_image
    target_image: ImageHandle,
    pipeline: Arc<GraphicsPipeline>,
    push: PushConstant<EGuiPush>,
}


impl EGuiRender{

    ///Default vertex buffer size (in vertices).
    pub const DEFAULT_BUF_SIZE: usize = 1024;
    pub fn new(rmg: &mut Rmg) -> Result<Self, RmgError>{

        let target_format = rmg
            .ctx
            .device
            .select_format(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_SRC,
                vk::ImageTiling::OPTIMAL,
                &[
                    vk::Format::R8G8B8A8_UNORM,
                    vk::Format::B8G8R8A8_UNORM,
                    vk::Format::R16G16B16A16_SFLOAT,
                    vk::Format::R32G32B32A32_SFLOAT,
                ],
            )
            .unwrap();
        let target_image = rmg.new_image_uninitialized(
            ImgDesc {
                extent: vk::Extent3D {
                    width: 1,
                    height: 1,
                    depth: 1,
                },
                format: target_format,
                img_type: ImageType::Tex2d,
                tiling: vk::ImageTiling::OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::STORAGE,
                ..Default::default()
            },
            Some("egui target img"),
        )?;

        //Pipeline layout
        let layout = rmg.resources().bindless_layout();

        let shader_module = Arc::new(
            ShaderModule::new_from_bytes(&rmg.ctx.device, &crate::SHADER_RUST).unwrap(),
        );

        let vertex_shader_stage = ShaderStage::from_shared_module(
            shader_module.clone(),
            vk::ShaderStageFlags::VERTEX,
            "egui_vs".to_owned(),
        );

        let fragment_shader_stage = ShaderStage::from_shared_module(
            shader_module.clone(),
            vk::ShaderStageFlags::FRAGMENT,
            "egui_fs".to_owned(),
        );

        let push = PushConstant::new(
            EGuiPush {
                texture: ResourceHandle::INVALID,
                pad0: [ResourceHandle::INVALID; 3],
                screen_size: [1.0,1.0],
                pad1: [0.0; 2]
            },
            vk::ShaderStageFlags::ALL,
        );

        let pipeline = Arc::new(
            Self::pipeline(
                &rmg.ctx.device,
                layout,
                &[vertex_shader_stage, fragment_shader_stage],
                &[target_format],
            )
                .unwrap(),
        );

        let default_vertex_buffer = vec![crate::egui::epaint::Vertex::default(); Self::DEFAULT_BUF_SIZE];
        let default_index_buffer = vec![0u32; Self::DEFAULT_BUF_SIZE];
        let vertex_buffer = DynamicBuffer::new_with_buffer(
            rmg,
            &default_vertex_buffer,
            BufDesc::for_slice(&default_vertex_buffer)
                .add_usage(vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER),
            None
        )?;
        let index_buffer = DynamicBuffer::new_with_buffer(
            rmg,
            &default_index_buffer,
            BufDesc::for_slice(&default_index_buffer)
                .add_usage(vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER),
            None
        )?;

        Ok(EGuiRender {
            atlas: FxHashMap::default(),
            vertex_buffer,
            index_buffer,
            commands: Vec::new(),

            px_per_point: 1.0,
            target_image,
            pipeline,
            push
        })
    }

        pub fn pipeline(
        device: &Arc<Device>,
        pipeline_layout: impl Into<OoS<PipelineLayout>>,
        shader_stages: &[ShaderStage],
        color_formats: &[vk::Format],
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
            .depth_write_enable(false)
            .depth_test_enable(false)
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
            .stride(core::mem::size_of::<crate::egui::epaint::Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);
        let vertex_attrib_desc = [
            //Description of the Pos attribute
            vk::VertexInputAttributeDescription::builder()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(crate::egui::epaint::Vertex, pos) as u32)
                .build(),
            //Description of the UV attribute
            vk::VertexInputAttributeDescription::builder()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(crate::egui::epaint::Vertex, uv) as u32)
                .build(),
            //Description of the COLOR attribute
            vk::VertexInputAttributeDescription::builder()
                .location(2)
                .binding(0)
                .format(vk::Format::R8G8B8A8_UNORM)
                .offset(offset_of!(crate::egui::epaint::Vertex, color) as u32)
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
            vk::Format::UNDEFINED,
        )?;
        Ok(pipeline)
    }


    ///Sets the internal primitives. Usually the output of `egui::Context::tesselate`.
    pub fn set_primitives(&mut self, rmg: &mut Rmg, primitives: Vec<ClippedPrimitive>) -> Result<(), RmgError>{
        //gather all vertices and indices into one big buffer. Annotate our dram commands.
        self.commands.clear();
        self.commands.reserve(primitives.len());

        let (vertex_count, index_count) = primitives.iter().fold((0, 0), |(mut vc, mut ic), prim|{
            match &prim.primitive{
                crate::egui::epaint::Primitive::Mesh(mesh) => {
                    vc += mesh.vertices.len();
                    ic += mesh.indices.len();
                }
                crate::egui::epaint::Primitive::Callback(_) => {
                    #[cfg(feature="logging")]
                    log::error!("Primitive callback not implemented");
                }
            }

            (vc, ic)
        });
        let mut new_vertex_buffer = Vec::with_capacity(vertex_count);
        let mut new_index_buffer = Vec::with_capacity(index_count);

        let width_in_pixels = self.target_image.extent_2d().width;
        let height_in_pixels = self.target_image.extent_2d().height;

        for prim in primitives{
            match prim.primitive{
                crate::egui::epaint::Primitive::Mesh(mut mesh) => {
                    // Transform clip rect to physical pixels:
                    let clip_min_x = self.px_per_point * prim.clip_rect.min.x;
                    let clip_min_y = self.px_per_point * prim.clip_rect.min.y;
                    let clip_max_x = self.px_per_point * prim.clip_rect.max.x;
                    let clip_max_y = self.px_per_point * prim.clip_rect.max.y;

                    // Make sure clip rect can fit within a `u32`:
                    let clip_min_x = clip_min_x.clamp(0.0, width_in_pixels as f32);
                    let clip_min_y = clip_min_y.clamp(0.0, height_in_pixels as f32);
                    let clip_max_x = clip_max_x.clamp(clip_min_x, width_in_pixels as f32);
                    let clip_max_y = clip_max_y.clamp(clip_min_y, height_in_pixels as f32);

                    let clip_min_x = clip_min_x.round() as u32;
                    let clip_min_y = clip_min_y.round() as u32;
                    let clip_max_x = clip_max_x.round() as u32;
                    let clip_max_y = clip_max_y.round() as u32;
                    let command = EGuiPrimDraw{
                        index_offset: new_index_buffer.len().try_into().unwrap(),
                        vertex_offset: new_vertex_buffer.len().try_into().unwrap(),
                        index_buffer_size: mesh.indices.len().try_into().unwrap(),
                        vertex_buffer_size: mesh.vertices.len().try_into().unwrap(),
                        clip: vk::Rect2D{
                            offset: vk::Offset2D{x: clip_min_x as i32, y: clip_min_y as i32},
                            extent: vk::Extent2D{width: clip_max_x-clip_min_x, height: clip_max_y-clip_min_y}
                        }
                    };

                    new_vertex_buffer.append(&mut mesh.vertices);
                    new_index_buffer.append(&mut mesh.indices);
                    self.commands.push(command);
                }
                crate::egui::epaint::Primitive::Callback(_) => {
                    #[cfg(feature="logging")]
                    log::error!("Primitive callback not implemented");
                }
            }
        }

        //check if we can just write to the index/vertex buffer, or if we have to grow.
        if self.index_buffer.buffer_handle().count() > new_index_buffer.len(){
            //can overwrite
            self.index_buffer.write(&new_index_buffer, 0)
                             .map_err(|e| anyhow::anyhow!("Uploaded index buffer partially {}/{}", e, new_index_buffer.len()))?;
        }else {
            #[cfg(feature="logging")]
            log::info!("Have to grow index buffer to {}", new_index_buffer.len());

            self.index_buffer = DynamicBuffer::new_with_buffer(
                rmg,
                &new_index_buffer,
                BufDesc::for_slice(&new_index_buffer)
                    .add_usage(vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER),
                None
            )?;
        }

        if self.vertex_buffer.buffer_handle().count() > new_vertex_buffer.len(){
            //can overwrite
            self.vertex_buffer.write(&new_vertex_buffer, 0)
                              .map_err(|e| anyhow::anyhow!("Uploaded vertex buffer partially {}/{}", e, new_vertex_buffer.len()))?;
        }else {
            #[cfg(feature="logging")]
            log::info!("Have to grow vertex buffer to {}", new_vertex_buffer.len());

            self.vertex_buffer = DynamicBuffer::new_with_buffer(
            rmg,
            &new_vertex_buffer,
            BufDesc::for_slice(&new_vertex_buffer)
                .add_usage(vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER),
            None
        )?;
        }

        Ok(())
    }

    pub fn set_texture_deltas(&mut self, rmg: &mut Rmg, deltas: TexturesDelta) -> Result<(), RmgError>{
        println!("Textures not implemented");
        Ok(())
    }

    ///The image the egui output is written to.
    pub fn target_image(&self) -> &ImageHandle{
        &self.target_image
    }

    ///Sets the resolution of the rendertarget
    pub fn set_resolution(&mut self, rmg: &mut Rmg, resolution: vk::Extent2D) -> Result<(), RmgError>{

        if resolution == self.target_image.extent_2d(){
            return Ok(());
        }

        let resolution = vk::Extent2D{
            width: resolution.width.max(1),
            height: resolution.height.max(1)
        };

        //Use same format, different resolution.
        let format = self.target_image.format();
        let description = ImgDesc{
            extent: vk::Extent3D{
                width: resolution.width,
                height: resolution.height,
                depth: 1
            },
            format: *format,
            img_type: ImageType::Tex2d,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::STORAGE,
            ..Default::default()
        };

        self.target_image = rmg.new_image_uninitialized(description, Some("egui target image"))?;
        Ok(())
    }
}


impl Task for EGuiRender{
    fn name(&self) -> &'static str {
        "EGuiRender"
    }

    fn queue_flags(&self) -> marpii::ash::vk::QueueFlags {
        self.vertex_buffer.queue_flags() | self.index_buffer.queue_flags() | marpii::ash::vk::QueueFlags::GRAPHICS
    }

    fn pre_record(&mut self, resources: &mut marpii_rmg::Resources, ctx: &marpii_rmg::CtxRmg) -> Result<(), marpii_rmg::RecordError> {
        self.index_buffer.pre_record(resources, ctx)?;
        self.vertex_buffer.pre_record(resources, ctx)?;

        //setup push
        let width_in_points = self.target_image.extent_2d().width as f32 / self.px_per_point;
        let height_in_points = self.target_image.extent_2d().height as f32 / self.px_per_point;
        self.push.get_content_mut().screen_size = [width_in_points, height_in_points];

        Ok(())
    }

    fn post_execution(
        &mut self,
        resources: &mut marpii_rmg::Resources,
        ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        self.index_buffer.post_execution(resources, ctx)?;
        self.vertex_buffer.post_execution(resources, ctx)?;
        Ok(())
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        self.index_buffer.register(registry);
        self.vertex_buffer.register(registry);

        registry.request_buffer(self.index_buffer.buffer_handle());
        registry.request_buffer(self.vertex_buffer.buffer_handle());
        registry.register_asset(self.pipeline.clone());
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &marpii::ash::vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        self.index_buffer.record(device, command_buffer, resources);
        self.vertex_buffer.record(device, command_buffer, resources);


        //after recording updates, schedule all draw commands
        let (target_before_access, target_before_layout, targetimg, targetview) = {
            let img_access = resources.get_image_state(&self.target_image);
            (
                img_access.mask,
                img_access.layout,
                img_access.image.clone(),
                img_access.view.clone(),
            )
        };

        let vertex_buffer_access = resources.get_buffer_state(&self.vertex_buffer.buffer_handle());
        let index_buffer_access = resources.get_buffer_state(&self.index_buffer.buffer_handle());

        let viewport = targetimg.image_region().as_viewport();

        let color_attachments = vk::RenderingAttachmentInfo::builder()
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
            })
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image_view(targetview.view)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE);

        let render_info = vk::RenderingInfo::builder()
            .render_area(vk::Rect2D{
                offset: vk::Offset2D {x: 0, y: 0},
                extent: self.target_image.extent_2d()
            })
            .layer_count(1)
            .color_attachments(core::slice::from_ref(&color_attachments));

        //transfer image to render attachment
        unsafe {
            device.inner.cmd_pipeline_barrier2(
                *command_buffer,
                &vk::DependencyInfo::builder().image_memory_barriers(&[
                    //src image
                    *vk::ImageMemoryBarrier2::builder()
                        .image(targetimg.inner)
                        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .src_access_mask(target_before_access)
                        .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                        .subresource_range(targetimg.subresource_all())
                        .old_layout(target_before_layout)
                        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                ]),
            );
        }


        unsafe{
            //setup draw state (binding / viewport etc)
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

            for cmd in self.commands.iter(){

                device
                    .inner
                    .cmd_set_scissor(*command_buffer, 0, &[cmd.clip]);
                device.inner.cmd_draw_indexed(
                    *command_buffer,
                    cmd.index_buffer_size,
                    1,
                    cmd.index_offset,
                    cmd.vertex_offset.try_into().unwrap(),
                    0
                );
            }

            //end renderpass
            device.inner.cmd_end_rendering(*command_buffer);
        }


        //transfer target back into initial layout
        unsafe {
            device.inner.cmd_pipeline_barrier2(
                *command_buffer,
                &vk::DependencyInfo::builder().image_memory_barriers(&[
                    //src image
                    *vk::ImageMemoryBarrier2::builder()
                        .image(targetimg.inner)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .dst_access_mask(target_before_access)
                        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                        .subresource_range(targetimg.subresource_all())
                        .new_layout(target_before_layout)
                        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                ]),
            );
        }
    }
}
