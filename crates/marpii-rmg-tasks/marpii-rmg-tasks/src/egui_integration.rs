//! # Egui integration
//!
//! Renders an user defined egui to its inner target image. Note that the `target` image uses an alpha channel. Therefore, the image can easily be
//! rendered on top of an existing image.
//!
//! Have a look at the egui example for an in depth integration.
//!

use crate::egui::{ClippedPrimitive, TextureId, TexturesDelta};
use crate::{DynamicBuffer, DynamicImage, RmgTaskError};
use ahash::AHashMap;
use egui::{Color32, Pos2, ViewportId};
use egui_winit::winit::raw_window_handle::HasDisplayHandle;
use egui_winit::winit::window::Window;
use marpii::ash::vk::{ImageUsageFlags, Rect2D};
use marpii::resources::SharingMode;
use marpii::util::ImageRegion;
use marpii::MarpiiError;
use marpii::{
    ash::vk,
    context::Device,
    offset_of,
    resources::{
        BufDesc, GraphicsPipeline, ImageType, ImgDesc, PipelineLayout, PushConstant, ShaderModule,
        ShaderStage,
    },
    OoS,
};
use marpii_rmg::recorder::task::MetaTask;
use marpii_rmg::{BufferHandle, ImageHandle, Rmg, RmgError, SamplerHandle, Task};
use marpii_rmg_task_shared::{EGuiPush, ResourceHandle};
use std::borrow::Cow;
use std::collections::hash_map::Values;
use std::sync::Arc;
use std::time::Instant;

//NOTE: There is a (buggy) glsl implementation. Keeping it here, but we use rust-gpu actually
const EGUI_SHADER_VERT: &'static [u8] = include_bytes!("../resources/eguivert.spv");
const EGUI_SHADER_FRAG: &'static [u8] = include_bytes!("../resources/eguifrag.spv");

///Single EGui primitive draw command
struct EGuiPrimDraw {
    ///Offset into the vertex buffer
    vertex_offset: u32,
    ///Offest into the index buffer
    index_offset: u32,

    #[allow(dead_code)]
    vertex_buffer_size: u32,
    index_buffer_size: u32,

    sampler: ResourceHandle,
    texture: ResourceHandle,

    clip: Rect2D,
}

///Wrapper around the render task, takes care of winit events and let's you define
/// the UI for each frame. Note that you can configure the renderer. Use that to setup a source image for instance
/// that serves as "background". This would typically be your scene.
pub struct EGuiWinitIntegration {
    //translation state
    winit_state: egui_winit::State,
    start: Instant,
    egui_context: egui::Context,
    task: EGuiTask,
}

impl EGuiWinitIntegration {
    pub fn new(rmg: &mut Rmg, event_loop: &dyn HasDisplayHandle) -> Result<Self, RmgTaskError> {
        #[cfg(feature = "logging")]
        log::trace!("Setting up egui context");
        let egui_context: egui::Context = Default::default();
        egui_context.set_pixels_per_point(1.0);

        #[cfg(feature = "logging")]
        log::trace!("Setting up winit state");

        let mut winit_state = egui_winit::State::new(
            egui_context.clone(),
            ViewportId::ROOT,
            event_loop,
            None,
            None,
            Some(EGuiTask::MAX_TEXTURE_SIDE as usize),
        );
        winit_state.set_max_texture_side(EGuiTask::MAX_TEXTURE_SIDE as usize);

        Ok(EGuiWinitIntegration {
            winit_state,
            start: Instant::now(),
            egui_context,
            task: EGuiTask::new(rmg)?,
        })
    }
    pub fn handle_event<T>(&mut self, window: &Window, event: &egui_winit::winit::event::Event<T>) {
        match event {
            egui_winit::winit::event::Event::WindowEvent {
                window_id: _,
                event,
            } => {
                let _is_exclusive = self.winit_state.on_window_event(window, event);
            }
            egui_winit::winit::event::Event::DeviceEvent {
                event: egui_winit::winit::event::DeviceEvent::MouseMotion { delta },
                device_id: _,
            } => {
                let _ = self.winit_state.on_mouse_motion(*delta);
            }
            _ => {}
        }
    }

    pub fn target_image(&self) -> &ImageHandle {
        &self.task.renderer.target_image
    }

    pub fn renderer_mut(&mut self) -> &mut EGuiTask {
        &mut self.task
    }

    pub fn renderer(&self) -> &EGuiTask {
        &self.task
    }

    ///Sets a gamma value for the output.
    ///
    /// Should be 1.0 if you are using the pass in a linear-context (before tone mapping and gamma correction).
    /// Otherwise it should be 2.2.
    pub fn set_gamma(&mut self, gamma: f32) {
        self.task.renderer.gamma = gamma;
    }

    ///runs `run_ui` on this context. Use the closure to encode the next "to be rendered" egui.
    pub fn run(
        &mut self,
        rmg: &mut Rmg,
        window: &Window,
        run_ui: impl FnMut(&egui::Context),
    ) -> Result<(), RmgTaskError> {
        let mut raw_input = self.winit_state.take_egui_input(window);
        raw_input.time = Some(self.start.elapsed().as_secs_f64());
        let resolution = vk::Extent2D {
            width: window.inner_size().width,
            height: window.inner_size().height,
        };
        raw_input.screen_rect = Some(egui_winit::egui::Rect {
            min: Pos2::ZERO,
            max: Pos2::new(resolution.width as f32, resolution.height as f32),
        });

        self.egui_context
            .set_pixels_per_point(window.scale_factor() as f32);
        self.task.renderer.px_per_point = window.scale_factor() as f32;

        let output = self.egui_context.run(raw_input, run_ui);

        let primitives = self
            .egui_context
            .tessellate(output.shapes, window.scale_factor() as f32);

        self.task.set_resolution(rmg, resolution)?;
        self.task.set_primitives(rmg, primitives)?;
        self.task.set_texture_deltas(rmg, output.textures_delta)?;

        self.winit_state
            .handle_platform_output(window, output.platform_output);
        Ok(())
    }
}

///Data task that is run whenever egui resources for
/// the gpu change. Apart from that encapsulates all gpu data.
struct EGuiData {
    vertex_buffer: DynamicBuffer<crate::egui::epaint::Vertex>,
    index_buffer: DynamicBuffer<u32>,
    //NOTE: egui uses three main resources to render its interface. A texture atlas, and a vertex/index buffer changing at a high rate
    //      we take our own DynamicBuffer and DynamicImage for those tasks.
    atlas: AHashMap<TextureId, DynamicImage>,
    //Deferred free commands for texture_deltas. Basically the *last* free list.
    deferred_free: Vec<TextureId>,
}

///handle data for all data needed to render the next frame.
struct EGuiCall {
    vertex_buffer: BufferHandle<crate::egui::epaint::Vertex>,
    index_buffer: BufferHandle<u32>,
    textures: Vec<ImageHandle>,
}

///Render task of the renderer. Only schedules drawcall submission.
struct EGuiRenderer {
    linear_sampler: SamplerHandle,
    #[allow(dead_code)]
    nearest_sampler: SamplerHandle,

    commands: Vec<EGuiPrimDraw>,

    px_per_point: f32,
    gamma: f32,
    //target_image
    target_image: ImageHandle,
    //true if if the target was overwritten at some point.
    is_overwritten: bool,
    pipeline: Arc<GraphicsPipeline>,
    push: PushConstant<EGuiPush>,

    frame_data: Option<EGuiCall>,
}

impl Task for EGuiRenderer {
    fn name(&self) -> &'static str {
        "EGuiRenderer"
    }

    fn queue_flags(&self) -> marpii::ash::vk::QueueFlags {
        marpii::ash::vk::QueueFlags::GRAPHICS
    }

    fn pre_record(
        &mut self,
        _resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        //setup push
        let width_in_points = self.target_image.extent_2d().width as f32 / self.px_per_point;
        let height_in_points = self.target_image.extent_2d().height as f32 / self.px_per_point;
        self.push.get_content_mut().screen_size = [width_in_points, height_in_points];
        self.push.get_content_mut().gamma = self.gamma;
        Ok(())
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        if let Some(call) = &self.frame_data {
            registry
                .request_buffer(
                    &call.index_buffer,
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    vk::AccessFlags2::INDEX_READ,
                )
                .unwrap();
            registry
                .request_buffer(
                    &call.vertex_buffer,
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    vk::AccessFlags2::VERTEX_ATTRIBUTE_READ,
                )
                .unwrap();
            registry
                .request_image(
                    &self.target_image,
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                )
                .unwrap();

            for img in call.textures.iter() {
                registry
                    .request_image(
                        img,
                        vk::PipelineStageFlags2::ALL_GRAPHICS,
                        vk::AccessFlags2::SHADER_READ,
                        vk::ImageLayout::GENERAL,
                    )
                    .unwrap();
            }

            registry.register_asset(self.pipeline.clone());
        }
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &marpii::ash::vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        if let Some(call) = self.frame_data.take() {
            //after recording updates, schedule all draw commands
            let (targetimg, targetview) = {
                let img_access = resources.get_image_state(&self.target_image);
                (img_access.image.clone(), img_access.view.clone())
            };

            let vertex_buffer_access = resources.get_buffer_state(&call.vertex_buffer);
            let index_buffer_access = resources.get_buffer_state(&call.index_buffer);

            let viewport = targetimg.image_region().as_viewport();

            let color_attachments = vk::RenderingAttachmentInfo::default()
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 0.0],
                    },
                })
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .image_view(targetview.view)
                .load_op(if self.is_overwritten {
                    vk::AttachmentLoadOp::LOAD
                } else {
                    vk::AttachmentLoadOp::CLEAR
                })
                .store_op(vk::AttachmentStoreOp::STORE);

            let render_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.target_image.extent_2d(),
                })
                .layer_count(1)
                .color_attachments(core::slice::from_ref(&color_attachments));

            let commands = std::mem::take(&mut self.commands);
            unsafe {
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

                for cmd in commands {
                    //setup texture and sampler
                    self.push.get_content_mut().sampler = cmd.sampler;
                    self.push.get_content_mut().texture = cmd.texture;
                    device.inner.cmd_push_constants(
                        *command_buffer,
                        self.pipeline.layout.layout,
                        vk::ShaderStageFlags::ALL,
                        0,
                        self.push.content_as_bytes(),
                    );

                    //set scissors to clip space
                    device
                        .inner
                        .cmd_set_scissor(*command_buffer, 0, &[cmd.clip]);

                    device.inner.cmd_draw_indexed(
                        *command_buffer,
                        cmd.index_buffer_size,
                        1,
                        cmd.index_offset,
                        cmd.vertex_offset.try_into().unwrap(),
                        0,
                    );
                }

                //end renderpass
                device.inner.cmd_end_rendering(*command_buffer);
            }
        }
    }
}

///Egui meta task. Make sure to supply the renderer with all `winit` events
/// that should be taken into account, or use [EGuiWinitIntegration] instead.
pub struct EGuiTask {
    ///data manager and update task
    data: EGuiData,
    renderer: EGuiRenderer,
}

impl EGuiTask {
    ///Default vertex buffer size (in vertices).
    pub const DEFAULT_BUF_SIZE: usize = 1024;
    pub const MAX_TEXTURE_SIDE: u32 = 2048;
    pub fn texture_atlas(&self) -> Values<TextureId, DynamicImage> {
        self.data.atlas.values()
    }

    fn default_texture_desc() -> ImgDesc {
        ImgDesc {
            format: vk::Format::R8G8B8A8_UNORM,
            img_type: ImageType::Tex2d,
            usage: ImageUsageFlags::SAMPLED
                | ImageUsageFlags::TRANSFER_DST
                | ImageUsageFlags::TRANSFER_SRC,
            sharing_mode: SharingMode::Exclusive,
            ..ImgDesc::default()
        }
    }

    pub fn new(rmg: &mut Rmg) -> Result<Self, RmgTaskError> {
        let target_format = rmg
            .ctx
            .device
            .select_format(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST,
                vk::ImageTiling::OPTIMAL,
                //NOTE on the format: We don't use any srgb formats.
                //     Since we are in an *engine* context mostly we don't do linear->srgb
                //     Since that will be handled by the engine mostly.
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
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::STORAGE,
                ..Default::default()
            },
            Some("egui target img"),
        )?;

        //Pipeline layout
        let layout = rmg.resources.bindless_layout();

        let shader_module_vert =
            OoS::new(ShaderModule::new_from_bytes(&rmg.ctx.device, EGUI_SHADER_VERT).unwrap());

        let shader_module_frag =
            OoS::new(ShaderModule::new_from_bytes(&rmg.ctx.device, EGUI_SHADER_FRAG).unwrap());

        /*
                let mut shader_module =
                    OoS::new(ShaderModule::new_from_bytes(&rmg.ctx.device, crate::SHADER_RUST).unwrap());
        */
        let vertex_shader_stage = ShaderStage::from_module(
            shader_module_vert,
            vk::ShaderStageFlags::VERTEX,
            "main".to_owned(),
        );

        let fragment_shader_stage = ShaderStage::from_module(
            shader_module_frag,
            vk::ShaderStageFlags::FRAGMENT,
            "main".to_owned(),
        );

        let push = PushConstant::new(
            EGuiPush {
                texture: ResourceHandle::INVALID,
                sampler: ResourceHandle::INVALID,
                pad0: [ResourceHandle::INVALID; 2],
                gamma: 1.0,
                screen_size: [1.0, 1.0],
                pad1: 0.0,
            },
            vk::ShaderStageFlags::ALL,
        );

        let pipeline = Arc::new(
            Self::pipeline(
                &rmg.ctx.device,
                OoS::new_shared(layout),
                &[vertex_shader_stage, fragment_shader_stage],
                &[target_format],
            )
            .unwrap(),
        );

        let default_vertex_buffer =
            vec![crate::egui::epaint::Vertex::default(); Self::DEFAULT_BUF_SIZE];
        let default_index_buffer = vec![0u32; Self::DEFAULT_BUF_SIZE];
        let vertex_buffer = DynamicBuffer::new_with_buffer(
            rmg,
            &default_vertex_buffer,
            BufDesc::for_slice(&default_vertex_buffer).add_usage(
                vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER,
            ),
            None,
        )?;
        let index_buffer = DynamicBuffer::new_with_buffer(
            rmg,
            &default_index_buffer,
            BufDesc::for_slice(&default_index_buffer).add_usage(
                vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER,
            ),
            None,
        )?;

        let linear_sampler = rmg.new_sampler(
            &vk::SamplerCreateInfo::default()
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .min_filter(vk::Filter::LINEAR)
                .mag_filter(vk::Filter::LINEAR),
        )?;

        let nearest_sampler = rmg.new_sampler(
            &vk::SamplerCreateInfo::default()
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .min_filter(vk::Filter::NEAREST)
                .mag_filter(vk::Filter::NEAREST),
        )?;

        Ok(EGuiTask {
            data: EGuiData {
                vertex_buffer,
                index_buffer,
                atlas: AHashMap::default(),
                deferred_free: Vec::new(),
            },

            renderer: EGuiRenderer {
                commands: Vec::new(),

                linear_sampler,
                nearest_sampler,

                is_overwritten: false,
                px_per_point: 1.0,
                target_image,
                pipeline,
                push,
                gamma: 1.0,
                frame_data: None,
            },
        })
    }

    pub fn pipeline(
        device: &Arc<Device>,
        pipeline_layout: impl Into<OoS<PipelineLayout>>,
        shader_stages: &[ShaderStage],
        color_formats: &[vk::Format],
    ) -> Result<GraphicsPipeline, MarpiiError> {
        let color_blend_attachments = vk::PipelineColorBlendAttachmentState::default()
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            //.color_blend_op(vk::BlendOp::ADD)
            //.src_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_DST_ALPHA)
            //.dst_alpha_blend_factor(vk::BlendFactor::ONE)
            //.alpha_blend_op(vk::BlendOp::ADD)
            //.color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true);

        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .blend_constants([0.0; 4])
            .attachments(core::slice::from_ref(&color_blend_attachments)); //only the color target

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_compare_op(vk::CompareOp::ALWAYS)
            .depth_write_enable(false)
            .depth_test_enable(false)
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
            .stride(core::mem::size_of::<crate::egui::epaint::Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);
        let vertex_attrib_desc = [
            //Description of the Pos attribute
            vk::VertexInputAttributeDescription::default()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(crate::egui::epaint::Vertex, pos) as u32),
            //Description of the UV attribute
            vk::VertexInputAttributeDescription::default()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(crate::egui::epaint::Vertex, uv) as u32),
            //Description of the COLOR attribute
            vk::VertexInputAttributeDescription::default()
                .location(2)
                .binding(0)
                .format(vk::Format::R8G8B8A8_UNORM)
                .offset(offset_of!(crate::egui::epaint::Vertex, color) as u32),
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
            vk::Format::UNDEFINED,
        )?;
        Ok(pipeline)
    }

    ///Sets the internal primitives. Usually the output of `egui::Context::tesselate`.
    pub fn set_primitives(
        &mut self,
        rmg: &mut Rmg,
        primitives: Vec<ClippedPrimitive>,
    ) -> Result<(), RmgTaskError> {
        //gather all vertices and indices into one big buffer. Annotate our dram commands.

        //self.commands.clear();
        self.renderer.commands.reserve(primitives.len());

        let (vertex_count, index_count) =
            primitives.iter().fold((0, 0), |(mut vc, mut ic), prim| {
                match &prim.primitive {
                    crate::egui::epaint::Primitive::Mesh(mesh) => {
                        vc += mesh.vertices.len();
                        ic += mesh.indices.len();
                    }
                    crate::egui::epaint::Primitive::Callback(_) => {
                        #[cfg(feature = "logging")]
                        log::error!("Primitive callback not implemented");
                    }
                }

                (vc, ic)
            });
        let mut new_vertex_buffer = Vec::with_capacity(vertex_count);
        let mut new_index_buffer = Vec::with_capacity(index_count);

        let width_in_pixels = self.renderer.target_image.extent_2d().width;
        let height_in_pixels = self.renderer.target_image.extent_2d().height;

        for prim in primitives {
            match prim.primitive {
                crate::egui::epaint::Primitive::Mesh(mut mesh) => {
                    // Transform clip rect to physical pixels:
                    let clip_min_x = self.renderer.px_per_point * prim.clip_rect.min.x;
                    let clip_min_y = self.renderer.px_per_point * prim.clip_rect.min.y;
                    let clip_max_x = self.renderer.px_per_point * prim.clip_rect.max.x;
                    let clip_max_y = self.renderer.px_per_point * prim.clip_rect.max.y;

                    // Make sure clip rect can fit within a `u32`:
                    let clip_min_x = clip_min_x.clamp(0.0, width_in_pixels as f32);
                    let clip_min_y = clip_min_y.clamp(0.0, height_in_pixels as f32);
                    let clip_max_x = clip_max_x.clamp(clip_min_x, width_in_pixels as f32);
                    let clip_max_y = clip_max_y.clamp(clip_min_y, height_in_pixels as f32);

                    let clip_min_x = clip_min_x.round() as u32;
                    let clip_min_y = clip_min_y.round() as u32;
                    let clip_max_x = clip_max_x.round() as u32;
                    let clip_max_y = clip_max_y.round() as u32;

                    let texture = match self.data.atlas.get(&mesh.texture_id) {
                        Some(t) => rmg
                            .resources
                            .resource_handle_or_bind(t.image.clone())
                            .map_err(|e| RmgError::ResourceError(e))?,
                        None => {
                            #[cfg(feature = "logging")]
                            log::error!("No texture={:?} for egui mesh", mesh.texture_id);
                            continue;
                        }
                    };

                    //TODO choose right one?
                    let sampler = rmg
                        .resources
                        .resource_handle_or_bind(self.renderer.linear_sampler.clone())
                        .map_err(|e| RmgError::ResourceError(e))?;

                    let command = EGuiPrimDraw {
                        sampler,
                        texture,
                        index_offset: new_index_buffer.len().try_into().unwrap(),
                        vertex_offset: new_vertex_buffer.len().try_into().unwrap(),
                        index_buffer_size: mesh.indices.len().try_into().unwrap(),
                        vertex_buffer_size: mesh.vertices.len().try_into().unwrap(),
                        clip: vk::Rect2D {
                            offset: vk::Offset2D {
                                x: clip_min_x as i32,
                                y: clip_min_y as i32,
                            },
                            extent: vk::Extent2D {
                                width: clip_max_x - clip_min_x,
                                height: clip_max_y - clip_min_y,
                            },
                        },
                    };

                    new_vertex_buffer.append(&mut mesh.vertices);
                    new_index_buffer.append(&mut mesh.indices);
                    self.renderer.commands.push(command);
                }
                crate::egui::epaint::Primitive::Callback(_) => {
                    #[cfg(feature = "logging")]
                    log::error!("Primitive callback not implemented");
                }
            }
        }

        //check if we can just write to the index/vertex buffer, or if we have to grow.
        if self.data.index_buffer.buffer_handle().count() >= new_index_buffer.len() {
            //can overwrite
            self.data
                .index_buffer
                .write(&new_index_buffer, 0)
                .map_err(|e| MarpiiError::from(e))?;
        } else {
            #[cfg(feature = "logging")]
            log::info!("Have to grow index buffer {}", new_index_buffer.len());

            self.data.index_buffer = DynamicBuffer::new_with_buffer(
                rmg,
                &new_index_buffer,
                BufDesc::for_slice(&new_index_buffer).add_usage(
                    vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER,
                ),
                None,
            )?;
        }

        if self.data.vertex_buffer.buffer_handle().count() >= new_vertex_buffer.len() {
            //can overwrite
            self.data
                .vertex_buffer
                .write(&new_vertex_buffer, 0)
                .map_err(|e| MarpiiError::from(e))?;
        } else {
            #[cfg(feature = "logging")]
            log::info!("Have to grow vertex buffer to {}", new_vertex_buffer.len());
            self.data.vertex_buffer = DynamicBuffer::new_with_buffer(
                rmg,
                &new_vertex_buffer,
                BufDesc::for_slice(&new_vertex_buffer).add_usage(
                    vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER,
                ),
                None,
            )?;
        }

        Ok(())
    }

    pub fn set_texture_deltas(
        &mut self,
        rmg: &mut Rmg,
        deltas: TexturesDelta,
    ) -> Result<(), RmgError> {
        for (id, delta) in deltas.set {
            assert!(
                delta.image.bytes_per_pixel() == 4,
                "EGui image is not rgba8 encoded"
            );

            //delta position
            let pos = if let Some(pos) = delta.pos {
                [pos[0] as u32, pos[0] as u32]
            } else {
                [0u32; 2]
            };

            let ext = delta.image.size();
            let ext = [ext[0] as u32, ext[1] as u32];
            let _is_whole = delta.is_whole();

            let region = ImageRegion {
                offset: vk::Offset3D {
                    x: pos[0] as i32,
                    y: pos[1] as i32,
                    z: 0,
                },
                extent: vk::Extent3D {
                    width: ext[0],
                    height: ext[1],
                    depth: 1,
                },
            };

            //extract texture data as srgb
            let dta = match &delta.image {
                egui_winit::egui::epaint::ImageData::Color(img) => Cow::Borrowed(&img.pixels),
                egui_winit::egui::epaint::ImageData::Font(img) => {
                    Cow::Owned(img.srgba_pixels(None).collect::<Vec<Color32>>())
                }
            };

            let dta: &[u8] = bytemuck::cast_slice(dta.as_slice());

            if let Some(tex) = self.data.atlas.get_mut(&id) {
                tex.write_bytes(rmg, region, dta)?;
                if (ext[0] + pos[0]) > tex.image.extent_2d().width
                    || (ext[1] + pos[1]) > tex.image.extent_2d().height
                {
                    #[cfg(feature = "logging")]
                    log::warn!("Possibly writing egui texture {:?} out of bound", id);
                }
                /*FIXME: egui doesnt seem to repect the texture size. So it can happen that it writes outside
                 *       the texture bounds. We are currently ignoring that
                if (ext[0] + pos[0]) > tex.image.extent_2d().width
                    || (ext[1] + pos[1]) > tex.image.extent_2d().height
                {
                    //need to create a new texture, since the old one can't hold the new image
                    //assert!(is_whole, "Texture didn't fit, but was no whole texture!");
                    *tex = DynamicImage::new(
                        rmg,
                        ImgDesc {
                            //NOTE: must be *new* whole image, therefore pos == 0,0, and ext == image_extent
                            extent: vk::Extent3D {
                                width: ext[0],
                                height: ext[1],
                                depth: 1,
                            },
                            ..Self::default_texture_desc()
                        },
                        None,
                    )?;
                } else {
                    //Fits, update region
                    tex.write_bytes(rmg, region, dta)?;
                }
                 */
            } else {
                #[cfg(feature = "logging")]
                log::info!("Setting up eGui texture {:?}", id);
                let mut tex = DynamicImage::new(
                    rmg,
                    ImgDesc {
                        //NOTE: must be *new* whole image, therefore pos == 0,0, and ext == image_extent
                        extent: vk::Extent3D {
                            width: ext[0],
                            height: ext[1],
                            depth: 1,
                        },
                        ..Self::default_texture_desc()
                    },
                    None,
                )?;
                tex.write_bytes(rmg, region, dta)?;
                assert!(self.data.atlas.insert(id, tex).is_none());
            }
        }

        for free in self.data.deferred_free.drain(0..) {
            if let None = self.data.atlas.remove(&free) {
                #[cfg(feature = "logging")]
                log::error!("Tried removing non existent eGui texture {:?}", free);
            }
        }

        Ok(())
    }

    ///The image the egui output is written to.
    pub fn target_image(&self) -> &ImageHandle {
        &self.renderer.target_image
    }

    ///Sets the resolution of the rendertarget
    pub fn set_resolution(
        &mut self,
        rmg: &mut Rmg,
        resolution: vk::Extent2D,
    ) -> Result<(), RmgError> {
        if resolution == self.renderer.target_image.extent_2d() {
            return Ok(());
        }

        #[cfg(feature = "logging")]
        log::info!("Recreating EGUI resolution");

        let resolution = vk::Extent2D {
            width: resolution.width.max(1),
            height: resolution.height.max(1),
        };

        //Use same format, different resolution.
        let format = self.renderer.target_image.format();
        let description = ImgDesc {
            extent: vk::Extent3D {
                width: resolution.width,
                height: resolution.height,
                depth: 1,
            },
            format: *format,
            img_type: ImageType::Tex2d,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::STORAGE,
            ..Default::default()
        };

        self.renderer.target_image =
            rmg.new_image_uninitialized(description, Some("egui target image"))?;
        Ok(())
    }

    ///Overwrites the source image the pass renders to. Typically you might set that to a rendered scene or some other kind
    /// of background image. If you don't need that, consider not overwriting the source image.
    ///
    ///
    /// Note that the image is reset if [Self::set_resolution] is called.
    ///
    ///
    /// This will set the resolution as well. Note that the image must have the color attachment bit set and must support the
    /// COLOR_ATTACHMENT_OPTIMAL bit.
    pub fn set_source_image(&mut self, image: ImageHandle) {
        assert!(
            image
                .usage_flags()
                .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT),
            "Set image needs to have color attachment flags set"
        );
        self.renderer.is_overwritten = true;
        self.renderer.target_image = image;
    }
}

impl MetaTask for EGuiTask {
    fn record<'a>(
        &'a mut self,
        mut recorder: marpii_rmg::Recorder<'a>,
    ) -> Result<marpii_rmg::Recorder<'a>, marpii_rmg::RecordError> {
        //setup new rendercall for renderer.
        self.renderer.frame_data = Some(EGuiCall {
            vertex_buffer: self.data.vertex_buffer.buffer_handle().clone(),
            index_buffer: self.data.index_buffer.buffer_handle().clone(),
            textures: self
                .data
                .atlas
                .values()
                .map(|tex| tex.image.clone())
                .collect(),
        });

        //Add all data tasks that need to be handled.
        recorder = recorder.add_task(&mut self.data.index_buffer)?;
        recorder = recorder.add_task(&mut self.data.vertex_buffer)?;
        for t in self.data.atlas.values_mut() {
            recorder = recorder.add_task(t)?;
        }

        //now submit the actual rendering
        recorder = recorder.add_task(&mut self.renderer)?;

        Ok(recorder)
    }
}
