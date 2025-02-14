//! Textrendering uses [cosmic-text](https://crates.io/crates/cosmic-text) and [etager](https://crates.io/crates/etagere).
//!
//! cosmic-text handels font loading, shaping etc. etager is used to build per-font texture atlases. The actual rendering just blit
//! rectangles (similar to the quad-drawer), but fills them with the given part of a texture-atlas.

mod cache;
mod glue;

use std::sync::Arc;

use cache::glyph_content_to_type;
use cosmic_text::FontSystem;
use iced::{Rectangle, Transformation};
use iced_graphics::Settings;
use iced_marpii_shared::{GlyphInstance, TextPush};
use marpii::{
    ash::vk,
    resources::{GraphicsPipeline, PushConstant, ShaderModule, ShaderStage},
    OoS,
};
use marpii_rmg::{BufferHandle, ImageHandle, Rmg, SamplerHandle, Task};
use marpii_rmg_tasks::DynamicBuffer;

pub type Batch = Vec<iced_graphics::Text>;

pub struct TextLayer {
    instance_buffer_offset: u32,
    instance_count: u32,
    clip_bound: Rectangle,
}

pub struct TextPass {
    color_image: ImageHandle,
    depth_image: ImageHandle,
    instance_data: BufferHandle<GlyphInstance>,
    glyph_atlas_alpha: ImageHandle,
    glyph_atlas_color: ImageHandle,
    glyph_sampler: SamplerHandle,

    pipeline: Arc<GraphicsPipeline>,
    push: PushConstant<TextPush>,

    pub layer: Vec<TextLayer>,
}

impl TextPass {
    const SHADER_SOURCE: &'static [u8] = include_bytes!("../shaders/spirv/shader-text.spv");

    pub fn new(
        rmg: &mut Rmg,
        _settings: &Settings,
        color_image: ImageHandle,
        depth_image: ImageHandle,
        instance_data: BufferHandle<GlyphInstance>,
        glyph_atlas_color: ImageHandle,
        glyph_atlas_alpha: ImageHandle,
    ) -> Self {
        let push = PushConstant::new(TextPush::default(), vk::ShaderStageFlags::ALL_GRAPHICS);

        let sampler = rmg
            .new_sampler(
                &vk::SamplerCreateInfo::default()
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .min_filter(vk::Filter::LINEAR)
                    .mag_filter(vk::Filter::LINEAR),
            )
            .unwrap();

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

        let pipeline = Self::text_pipeline(
            rmg,
            &[vertex_shader_stage, fragment_shader_stage],
            color_image.format(),
            depth_image.format(),
        );

        Self {
            color_image,
            depth_image,
            glyph_atlas_color,
            glyph_atlas_alpha,
            glyph_sampler: sampler,
            pipeline,
            push,
            instance_data,
            layer: Vec::new(),
        }
    }

    fn text_pipeline(
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

    pub fn set_glyph_atlas(&mut self, color: ImageHandle, alpha: ImageHandle) {
        self.glyph_atlas_color = color;
        self.glyph_atlas_alpha = alpha;
    }

    pub fn resize(&mut self, color_buffer: ImageHandle, depth_buffer: ImageHandle) {
        self.color_image = color_buffer;
        self.depth_image = depth_buffer;
        let width = self.color_image.extent_2d().width;
        let height = self.color_image.extent_2d().height;
        self.push.get_content_mut().resolution = [width, height];
    }
}

impl Task for TextPass {
    fn name(&self) -> &'static str {
        "IcedText"
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_buffer(
                &self.instance_data,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                vk::AccessFlags2::SHADER_STORAGE_READ,
            )
            .unwrap();

        registry.register_asset(self.pipeline.clone());
        registry
            .request_image(
                &self.color_image,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE | vk::AccessFlags2::COLOR_ATTACHMENT_READ,
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
        registry
            .request_image(
                &self.glyph_atlas_alpha,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::READ_ONLY_OPTIMAL,
            )
            .unwrap();
        registry
            .request_image(
                &self.glyph_atlas_color,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::READ_ONLY_OPTIMAL,
            )
            .unwrap();
        registry.request_sampler(&self.glyph_sampler).unwrap();
    }

    fn pre_record(
        &mut self,
        resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        //bind all resources
        self.push.get_content_mut().instance_data =
            resources.resource_handle_or_bind(self.instance_data.clone())?;
        //todo bind glyph atlas
        self.push.get_content_mut().resolution = [
            self.color_image.extent_2d().width,
            self.color_image.extent_2d().height,
        ];
        self.push.get_content_mut().glyph_atlas_color =
            resources.resource_handle_or_bind(self.glyph_atlas_color.clone())?;
        self.push.get_content_mut().glyph_atlas_alpha =
            resources.resource_handle_or_bind(self.glyph_atlas_alpha.clone())?;
        self.push.get_content_mut().glyph_sampler =
            resources.resource_handle_or_bind(self.glyph_sampler.clone())?;

        self.push.get_content_mut().color_atlas_resolution = [
            self.glyph_atlas_color.extent_2d().width,
            self.glyph_atlas_color.extent_2d().height,
        ];
        self.push.get_content_mut().mask_atlas_resolution = [
            self.glyph_atlas_alpha.extent_2d().width,
            self.glyph_atlas_alpha.extent_2d().height,
        ];

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

        //println!("{} layers", self.layer.len());
        for layer in &self.layer {
            //setup the scissors for this call
            //TODO: actually do that?
            let mut scissors = vk::Rect2D {
                offset: vk::Offset2D {
                    x: layer.clip_bound.x.floor() as i32,
                    y: layer.clip_bound.y.floor() as i32,
                },
                extent: vk::Extent2D {
                    width: layer.clip_bound.width.ceil() as u32,
                    height: layer.clip_bound.height.ceil() as u32,
                },
            };
            //NOTE: we constrain the scissors to the render area.
            scissors.extent.width = scissors.extent.width.min(render_area.extent.width);
            scissors.extent.height = scissors.extent.height.min(render_area.extent.height);

            //update the push constant to this layer
            self.push.get_content_mut().instance_data_offset = layer.instance_buffer_offset;

            unsafe {
                device
                    .inner
                    .cmd_set_scissor(*command_buffer, 0, &[scissors]);
            }

            //now draw _instance_count_ rectangles
            unsafe {
                device.inner.cmd_push_constants(
                    *command_buffer,
                    self.pipeline.layout.layout,
                    vk::ShaderStageFlags::ALL,
                    0,
                    self.push.content_as_bytes(),
                );

                device
                    .inner
                    .cmd_draw(*command_buffer, 6, layer.instance_count, 0, 0);
            }
        }

        //end rendering
        unsafe {
            device.inner.cmd_end_rendering(*command_buffer);
        }
    }
}

pub struct TextRenderer {
    pub glyph_instance_buffer: Vec<GlyphInstance>,
    instance_buffer: DynamicBuffer<GlyphInstance>,
    font_atlas_cache: cache::FontAtlasCache,

    //local text cache for all simple text calls
    textcache: iced_graphics::text::Cache,

    pub renderpass: TextPass,
}

impl TextRenderer {
    pub fn new(
        rmg: &mut Rmg,
        settings: &iced_graphics::Settings,
        color_image: ImageHandle,
        depth_image: ImageHandle,
    ) -> Self {
        let glyph_instance_buffer = vec![GlyphInstance::default(); 512];
        let instance_buffer = DynamicBuffer::new(rmg, &glyph_instance_buffer).unwrap();
        let font_atlas_cache = cache::FontAtlasCache::new(rmg);

        let renderpass = TextPass::new(
            rmg,
            settings,
            color_image,
            depth_image,
            instance_buffer.buffer_handle().clone(),
            font_atlas_cache.glyph_texture_color(),
            font_atlas_cache.glyph_texture_alpha(),
        );

        TextRenderer {
            glyph_instance_buffer,
            instance_buffer,
            font_atlas_cache,
            textcache: iced_graphics::text::Cache::new(),
            renderpass,
        }
    }

    pub fn notify_resize(&mut self, color_buffer: ImageHandle, depth_buffer: ImageHandle) {
        self.renderpass.resize(color_buffer, depth_buffer)
    }

    pub fn new_frame(&mut self) {
        self.glyph_instance_buffer.clear();
        self.renderpass.layer.clear();
        //currently we just overwrite the handle each time
        self.renderpass.set_glyph_atlas(
            self.font_atlas_cache.glyph_texture_color(),
            self.font_atlas_cache.glyph_texture_alpha(),
        );
    }

    pub fn end_frame(&mut self) {
        self.font_atlas_cache.trim();
    }

    //Pushes a layer's text batch, and translates it to GPU executable per-glyph instance data.
    pub fn push_batch(
        &mut self,
        text_batch: &Batch,
        layer_bounds: &Rectangle,
        layer_transformation: Transformation,
        layer_depth: f32,
        font_system: &mut FontSystem,
    ) {
        //safe the current buffer length
        let text_layer_offset = self.glyph_instance_buffer.len();
        //this'll record how many glyphs there are per batch.
        let mut instance_count = 0;

        for text in text_batch.iter() {
            //create the text-area fot this text. If that fails, there might be no overlab.
            //this'll also take care of accessing/unwrapping, and unifying whatever `text` represents.
            let Some(text_area) = glue::TextArea::from_text(
                text,
                *layer_bounds,
                layer_transformation,
                &mut self.textcache,
                font_system,
            ) else {
                continue;
            };

            //now setup the text_area's glyph calls
            for run in text_area.buffer(&self.textcache).layout_runs() {
                for glyph in run.glyphs.iter() {
                    //TODO: Bail gylphs that are not in the text-area

                    let physical_glyph =
                        glyph.physical((text_area.left, text_area.top), text_area.scale);

                    //find the atlas location of the glyph
                    let Some(glyph_entry) = self
                        .font_atlas_cache
                        .find_or_create_glyph(physical_glyph.cache_key, font_system)
                    else {
                        continue;
                    };

                    let pos = [
                        physical_glyph.x as f32 + glyph_entry.placement.left as f32,
                        //NOTE: straight up copied from glyphon.
                        (run.line_y * text_area.scale).round() + physical_glyph.y as f32
                            - glyph_entry.placement.top as f32,
                    ];
                    let size = [
                        glyph_entry.placement.width as f32,
                        glyph_entry.placement.height as f32,
                    ];

                    let clip_offset = [text_area.bounds.x, text_area.bounds.y];
                    let clip_size = [text_area.bounds.width, text_area.bounds.height];

                    //NOTE: we do per-glyph clipping in the shader. This allows us for instance
                    //      to draw _half_ glyphs, when they are just about to drop out of rendering
                    //      range.
                    //      Here we just discard glyphs that are fully outside
                    let clip_rect = Rectangle::new(clip_offset.into(), clip_size.into());
                    let glyph_rect = Rectangle::new(pos.into(), size.into());
                    if clip_rect.intersection(&glyph_rect).is_none() {
                        continue;
                    }

                    let atlas_offset: [i32; 2] = glyph_entry.atlas_allocation.rectangle.min.into();
                    let atlas_size: [u32; 2] =
                        [glyph_entry.placement.width, glyph_entry.placement.height];

                    //Now build the glyph-instance with that knowledge
                    let glyph_instance = GlyphInstance {
                        pos,
                        size,
                        color: text_area.color.into_linear(),
                        atlas_offset: [
                            atlas_offset[0].try_into().unwrap(),
                            atlas_offset[1].try_into().unwrap(),
                        ],
                        atlas_size,
                        clip_offset,
                        clip_size,
                        layer_depth,
                        pad0: [0.0; 3],
                        glyph_type: glyph_content_to_type(glyph_entry.content),
                        pad1: [0; 3],
                    };
                    //push the glyph into the collection, and increase the layer's instance count
                    instance_count += 1;
                    self.glyph_instance_buffer.push(glyph_instance);
                }
            }
        }

        //Now add the text layer to the renderpass in order to schedule
        //enough instances
        self.renderpass.layer.push(TextLayer {
            instance_count,
            instance_buffer_offset: text_layer_offset.try_into().unwrap(),
            clip_bound: layer_bounds.clone(),
        });
    }

    ///Prepares the given text
    pub fn prepare(&mut self, rmg: &mut Rmg) {
        //do nothing, if there is no text
        if self.glyph_instance_buffer.len() == 0 {
            return;
        }

        //upload all glyph data
        if self.instance_buffer.element_count() < self.glyph_instance_buffer.len() {
            //recreate the dynamic buffer with the given elements
            self.instance_buffer = DynamicBuffer::new(rmg, &self.glyph_instance_buffer).unwrap();
            //update the inner pass's reference
            self.renderpass.instance_data = self.instance_buffer.buffer_handle().clone();
        } else {
            //just write them
            self.instance_buffer
                .write(&self.glyph_instance_buffer, 0)
                .unwrap();
        }

        //scheduler the upload
        rmg.record()
            .add_meta_task(&mut self.font_atlas_cache)
            .unwrap()
            .add_task(&mut self.instance_buffer)
            .unwrap()
            .execute()
            .unwrap()
    }
}
