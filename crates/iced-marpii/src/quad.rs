use std::{
    f32,
    hash::{Hash, Hasher},
    sync::Arc,
};

use ahash::AHashMap;
use iced::Rectangle;
use iced_graphics::Settings;
use iced_marpii_shared::{CmdQuad, QuadPush, ResourceHandle};
use marpii::{
    ash::vk,
    resources::{GraphicsPipeline, PushConstant, ShaderModule, ShaderStage},
    OoS,
};
use marpii_rmg::{BufferHandle, ImageHandle, MetaTask, Rmg, Task};
use marpii_rmg_tasks::UploadBuffer;

#[derive(Debug, Default, Hash, Clone)]
pub struct Batch {
    order: Vec<CmdQuad>,
}

impl Batch {
    pub fn add(&mut self, quad: iced_marpii_shared::CmdQuad) {
        self.order.push(quad);
    }

    pub fn clear(&mut self) {
        self.order.clear();
    }
}

enum BufferState {
    Uploading {
        was_enqueued: bool,
        upload: UploadBuffer<CmdQuad>,
    },
    Residing(BufferHandle<CmdQuad>),
}

impl BufferState {
    pub fn is_residing(&self) -> bool {
        if let Self::Residing(_) = self {
            true
        } else {
            false
        }
    }

    pub fn unwrap_handle(&self) -> BufferHandle<CmdQuad> {
        if let Self::Residing(hdl) = self {
            hdl.clone()
        } else {
            panic!("Handle not yet residing")
        }
    }
}

///A cached quad-draw batch.
struct CachedBatch {
    ///A flag that is incremented whenever the batch was not used in a frame.
    ///Allows us to delete buffers that where not used for a set of frames.
    last_use: usize,
    buffer: BufferState,
    batch_size: usize,
    //The bound this batch is drawn in
    bound: Rectangle,
}

impl CachedBatch {
    ///How many frames a buffer can be unused before being deleted.
    const MAX_NO_USE: usize = 10;

    pub fn new(rmg: &mut Rmg, batch: &Batch, bound: Rectangle) -> Self {
        let size = batch.order.len();
        let upload = UploadBuffer::new(rmg, batch.order.as_slice()).unwrap();
        CachedBatch {
            last_use: 0,
            buffer: BufferState::Uploading {
                was_enqueued: false,
                upload,
            },
            batch_size: size,
            bound,
        }
    }
}

///The quad calls that are enqueued.
struct BatchCall {
    buffer: BufferHandle<CmdQuad>,
    resource_handle: Option<ResourceHandle>,
    count: usize,
    bound: vk::Rect2D,
}

///The actual renderpass used to render the quads.
///
/// It uses a vertexbuffer-less DynamicRendering strategy.
///
/// What we do is registering all residing buffer-states
struct QuadPass {
    color_image: ImageHandle,

    pipeline: Arc<GraphicsPipeline>,
    batches: Vec<BatchCall>,
    push: PushConstant<QuadPush>,
}

impl QuadPass {
    const SHADER_SOURCE: &'static [u8] = include_bytes!("../shaders/spirv/shader-quad.spv");

    pub fn new(rmg: &mut Rmg, _settings: &Settings, color_image: ImageHandle) -> Self {
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
        );

        Self {
            color_image,
            pipeline,
            push,
            batches: Vec::new(),
        }
    }

    fn quad_pipeline(
        rmg: &mut Rmg,
        shader_stages: &[ShaderStage],
        color_format: &vk::Format,
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

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default();

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
            None,
        )
        .unwrap();
        Arc::new(pipeline)
    }

    pub fn resize(&mut self, color_buffer: ImageHandle) {
        self.color_image = color_buffer;
        let width = self.color_image.extent_2d().width;
        let height = self.color_image.extent_2d().height;
        self.push.get_content_mut().resolution = [width, height];
    }
}

impl Task for QuadPass {
    fn name(&self) -> &'static str {
        "IcedQuad"
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

        let render_area = colorimg.image_region().as_rect_2d();

        self.push.get_content_mut().resolution =
            [render_area.extent.width, render_area.extent.height];

        let viewport = colorimg.image_region().as_viewport();

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
            .render_area(render_area)
            .layer_count(1)
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

            unsafe {
                device
                    .inner
                    .cmd_set_scissor(*command_buffer, 0, &[scissors]);
            }

            for offset in 0..batch.count {
                //update push const
                self.push.get_content_mut().cmd_buffer = batch.resource_handle.unwrap();
                self.push.get_content_mut().offset = offset as u32;

                unsafe {
                    device.inner.cmd_push_constants(
                        *command_buffer,
                        self.pipeline.layout.layout,
                        vk::ShaderStageFlags::ALL,
                        0,
                        self.push.content_as_bytes(),
                    );

                    device.inner.cmd_draw(*command_buffer, 6, 1, 0, 0);
                }
            }
        }

        //end rendering
        unsafe {
            device.inner.cmd_end_rendering(*command_buffer);
        }
    }
}

///The vertex/index-buffer less quad renderer.
///
/// We use DynamicRendering + PushConstants to setup
/// the quad renderer.
pub struct QuadRenderer {
    ///Identifies a batch by its content's hash.
    batch_cache: AHashMap<u64, CachedBatch>,
    ///Order of batches to render
    order: Vec<u64>,

    pass: QuadPass,
}

impl QuadRenderer {
    pub fn new(rmg: &mut Rmg, settings: &Settings, color_buffer: ImageHandle) -> Self {
        let pass = QuadPass::new(rmg, settings, color_buffer);

        Self {
            batch_cache: AHashMap::default(),
            order: Vec::new(),
            pass,
        }
    }

    pub fn notify_resize(&mut self, color_buffer: ImageHandle) {
        self.pass.resize(color_buffer);
    }

    pub fn push_batch(&mut self, rmg: &mut Rmg, batch: &Batch, bound: Rectangle) {
        //Do not push batches, that are empty
        if batch.order.len() == 0 {
            return;
        }

        let mut hasher = ahash::AHasher::default();
        batch.hash(&mut hasher);
        let hash = hasher.finish();
        if let Some(cached) = self.batch_cache.get_mut(&hash) {
            log::trace!("Reusing quad-batch {hash}");
            //note: must be at least one, otherwise we'd try to reuse a batch twice.
            assert!(cached.last_use != 0, "batch was alredy reused");
            cached.last_use = 0;
            //overwrite bound
            cached.bound = bound;
            self.order.push(hash)
        } else {
            self.batch_cache
                .insert(hash, CachedBatch::new(rmg, batch, bound));
            self.order.push(hash)
        }
    }

    pub fn begin_new_frame(&mut self, viewport: &iced_graphics::Viewport) {
        //setup _general_transform_
        self.pass.push.get_content_mut().transform = viewport.projection().into();
        self.pass.push.get_content_mut().scale = viewport.scale_factor() as f32;

        //last-use flag update
        for batch in self.batch_cache.values_mut() {
            batch.last_use += 1;
        }
        //clear order
        self.order.clear();
    }

    pub fn prepare_data(&mut self, rmg: &mut Rmg) {
        let mut upload_recorder = rmg.record();

        for batch in self.batch_cache.values_mut() {
            if batch.last_use != 0 {
                continue;
            }

            match &mut batch.buffer {
                BufferState::Uploading {
                    was_enqueued,
                    upload,
                } => {
                    if !*was_enqueued {
                        *was_enqueued = true;
                    } else {
                        //ignore, if already enqueued
                        continue;
                    };
                    upload_recorder = upload_recorder.add_task(upload).unwrap();
                }
                BufferState::Residing(_) => {}
            }
        }

        //now upload all batches
        upload_recorder.execute().unwrap();

        //finally transition all to residing
        for batch in self.batch_cache.values_mut() {
            match &mut batch.buffer {
                BufferState::Residing(_buf) => {
                    //if already residing, don't do anything
                }
                BufferState::Uploading {
                    was_enqueued,
                    upload,
                } => {
                    if !*was_enqueued {
                        panic!("quad upload failed");
                    } else {
                        //was already enqueued, so we can transition to residing
                        batch.buffer = BufferState::Residing(upload.buffer.clone());
                    }
                }
            }
        }
    }

    pub fn end_frame(&mut self) {
        //Remove all cached buffer, where the last-use is too long ago
        self.batch_cache
            .retain(|_k, v| v.last_use < CachedBatch::MAX_NO_USE);
    }
}

impl MetaTask for QuadRenderer {
    fn record<'a>(
        &'a mut self,
        recorder: marpii_rmg::Recorder<'a>,
    ) -> Result<marpii_rmg::Recorder<'a>, marpii_rmg::RecordError> {
        self.pass.batches.clear();
        //transform batchen, in-order, into batch calls
        for batch_id in self.order.iter() {
            let batch = self.batch_cache.get(batch_id).unwrap();
            assert!(batch.buffer.is_residing());

            let batch_call = BatchCall {
                bound: vk::Rect2D {
                    offset: vk::Offset2D {
                        x: batch.bound.x as i32,
                        y: batch.bound.y as i32,
                    },
                    extent: vk::Extent2D {
                        width: batch.bound.width as u32,
                        height: batch.bound.height as u32,
                    },
                },
                buffer: batch.buffer.unwrap_handle(),
                resource_handle: None,
                count: batch.batch_size,
            };
            self.pass.batches.push(batch_call);
        }

        recorder.add_task(&mut self.pass)
    }
}
