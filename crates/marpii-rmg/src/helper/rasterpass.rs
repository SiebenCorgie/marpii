use crate::{
    BufferHandle, ImageHandle, ResourceError, ResourceRegistry, Resources, Rmg, RmgError,
    SamplerHandle, Task,
    helper::{BufferUsage, ImageUsage, ResourceRegister},
};
use marpii::{
    MarpiiError, OoS,
    ash::vk,
    resources::{GraphicsPipeline, PushConstant, ShaderModule, ShaderStage},
    util::ImageRegion,
};
use smallvec::SmallVec;
use std::sync::Arc;

///Specialized version of [`GraphicsPipeline`] that is
/// guaranteed to work on just a vertex and fragment shader.
pub struct RasterPipeline {
    pub inner: Arc<GraphicsPipeline>,
    ///Cached color attachment formats to ensure compatibility at runtime
    color_attachments: SmallVec<[vk::Format; 4]>,
    depth_stencil_attachment: Option<vk::Format>,
}

pub enum RasterDrawCall<P: 'static> {
    ///A simple draw call using the given index buffer
    Simple {
        index_buffer: BufferHandle<u32>,
        push_constant: P,
    },
    ///Draws the given index-buffer index-count times using `draw_instanced`
    Instanced {
        index_buffer: BufferHandle<u32>,
        push_constant: P,
        instance_count: u32,
    },
}

impl<P: 'static> RasterDrawCall<P> {
    fn index_buffer(&self) -> &BufferHandle<u32> {
        match self {
            Self::Instanced { index_buffer, .. } | Self::Simple { index_buffer, .. } => {
                index_buffer
            }
        }
    }

    fn push_data(&self) -> &P {
        match self {
            Self::Simple { push_constant, .. } | Self::Instanced { push_constant, .. } => {
                push_constant
            }
        }
    }
}

type AttachmentInfoAnd<T> = (
    ImageHandle,
    ImageUsage,
    vk::AttachmentLoadOp,
    vk::AttachmentStoreOp,
    T,
);

///A generic raster pass that uses a vertex+fragment shader
/// to generate the content of a set of images.
pub struct GenericRasterPass<P: Default + Clone + 'static> {
    pipeline: OoS<RasterPipeline>,
    push: PushConstant<P>,
    name: Option<String>,

    color_attachments: SmallVec<[Option<AttachmentInfoAnd<[f32; 4]>>; 4]>,
    depth_attachment: Option<AttachmentInfoAnd<f32>>,

    storage: ResourceRegister,
    framebuffer_area: ImageRegion,
    drawcalls: SmallVec<[(RasterDrawCall<P>, Option<ImageRegion>); 16]>,
}

impl GenericRasterPass<()> {
    pub fn init(pipeline: impl Into<OoS<RasterPipeline>>) -> Self {
        let pipeline = pipeline.into();
        let color_attachment_count = pipeline.color_attachments.len();
        GenericRasterPass {
            pipeline,
            push: PushConstant::new((), vk::ShaderStageFlags::ALL),
            name: None,
            color_attachments: smallvec::smallvec![None; color_attachment_count],
            depth_attachment: None,
            framebuffer_area: ImageRegion::ZERO,
            storage: ResourceRegister::new(),
            drawcalls: SmallVec::default(),
        }
    }
}

impl<P: Default + Clone + 'static> GenericRasterPass<P> {
    pub fn framebuffer_extent(&self) -> vk::Extent2D {
        vk::Extent2D {
            width: self.framebuffer_area.extent.width,
            height: self.framebuffer_area.extent.height,
        }
    }

    ///Shares the internal raster-pipeline object. Good if you want to create another
    /// pass based on the same pipeline for instance
    pub fn clone_pipeline(&mut self) -> OoS<RasterPipeline> {
        self.pipeline.share()
    }

    ///Allows the reconfiguration of the render-pass. If `keep_attachments` is true, it won't delete knowledge
    /// about used color/depth attachments, i.e. you don't have to re-record those.
    pub fn reconfigure<'rmg>(
        mut self,
        rmg: &'rmg mut Rmg,
        keep_attachments: bool,
    ) -> RasterPassBuilder<'rmg, P> {
        self.storage.reset();
        //reinsert the attachments, if we keep them
        if !keep_attachments {
            //remove all knowledge
            for c in &mut self.color_attachments {
                *c = None;
            }
            self.depth_attachment = None;
        }

        //remove drawcalls
        self.drawcalls.clear();

        RasterPassBuilder {
            rmg,
            task_setup: self,
        }
    }
}

impl<P: Default + Clone + 'static> Task for GenericRasterPass<P> {
    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("GenericGraphicsPass")
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }

    fn register(&self, registry: &mut ResourceRegistry) {
        for attachment in &self.color_attachments {
            let (image, usage, _, _, _) = attachment.as_ref().unwrap();
            registry
                .request_image(
                    image,
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    usage.into_access_flags(),
                    usage.into_layout(),
                )
                .unwrap();
        }

        if let Some((image, usage, _, _, _)) = &self.depth_attachment {
            registry
                .request_image(
                    image,
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    usage.into_access_flags(),
                    usage.into_layout(),
                )
                .unwrap();
        }

        //do the same for all index buffer
        for (call, _region) in &self.drawcalls {
            registry
                .request_buffer(
                    call.index_buffer(),
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    vk::AccessFlags2::INDEX_READ,
                )
                .unwrap();
        }

        //now enqueue all standard resources and the pipeline
        self.storage.register_all(registry);
        registry.register_asset(self.pipeline.inner.clone());
    }

    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        //1. transform attachment images
        let mut color_attachments: SmallVec<[_; 4]> = SmallVec::default();
        for color_attachment in &self.color_attachments {
            let color_attachment = color_attachment.as_ref().unwrap();
            let colorview = resources.get_image_state(&color_attachment.0).view.clone();

            let ca = vk::RenderingAttachmentInfo::default()
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: color_attachment.4,
                    },
                })
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .image_view(colorview.view)
                .load_op(color_attachment.2)
                .store_op(color_attachment.3);
            color_attachments.push(ca);
        }

        //2. Schedule draw ops
        //3. transform attachments back

        let mut render_info = vk::RenderingInfo::default()
            .render_area(self.framebuffer_area.as_rect_2d())
            .layer_count(1)
            .color_attachments(&color_attachments);

        //set a depth attchment, if it was defined
        let da;
        render_info = if let Some(depth) = &self.depth_attachment {
            let depthview = resources.get_image_state(&depth.0).view.clone();

            da = vk::RenderingAttachmentInfo::default()
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

            render_info.depth_attachment(&da)
        } else {
            render_info
        };

        //Tracks where we rendered, to switch the area on demand
        let mut last_render_area = self.framebuffer_area;

        //setup initial pass state
        unsafe {
            device
                .inner
                .cmd_begin_rendering(*command_buffer, &render_info);

            device.inner.cmd_bind_pipeline(
                *command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline.inner.pipeline,
            );

            device
                .inner
                .cmd_set_viewport(*command_buffer, 0, &[last_render_area.as_viewport()]);
            device
                .inner
                .cmd_set_scissor(*command_buffer, 0, &[last_render_area.as_rect_2d()]);
        }

        for (draw, render_area) in &self.drawcalls {
            let area = render_area.unwrap_or(self.framebuffer_area);
            //change the render_area if it doesn't track
            if area != last_render_area {
                unsafe {
                    device
                        .inner
                        .cmd_set_viewport(*command_buffer, 0, &[area.as_viewport()]);
                    device
                        .inner
                        .cmd_set_scissor(*command_buffer, 0, &[area.as_rect_2d()]);
                }
                //and overwrite
                last_render_area = area;
            }

            //now schedule the drawop itself

            //setup push data
            *self.push.get_content_mut() = draw.push_data().clone();
            unsafe {
                device.inner.cmd_push_constants(
                    *command_buffer,
                    self.pipeline.inner.layout.layout,
                    vk::ShaderStageFlags::ALL,
                    0,
                    self.push.content_as_bytes(),
                );
            }

            //setup the index_buffer
            let index_buffer = draw.index_buffer();
            let index_buffer_size = index_buffer
                .count()
                .try_into()
                .expect("IndexBuffer size exceeds 32bit int");
            unsafe {
                device.inner.cmd_bind_index_buffer(
                    *command_buffer,
                    index_buffer.bufref.inner,
                    0,
                    vk::IndexType::UINT32,
                );
            }

            //now, depending on the type, draw to screen
            match draw {
                RasterDrawCall::Instanced { instance_count, .. } => unsafe {
                    device.inner.cmd_draw_indexed(
                        *command_buffer,
                        index_buffer_size,
                        *instance_count,
                        0,
                        0,
                        0,
                    );
                },
                RasterDrawCall::Simple { .. } => unsafe {
                    device
                        .inner
                        .cmd_draw(*command_buffer, index_buffer_size, 1, 0, 0);
                },
            }
        }

        //now end the pass and return
        unsafe {
            device.inner.cmd_end_rendering(*command_buffer);
        }
    }
}

pub struct RasterPassBuilder<'rmg, P: Default + Clone + 'static> {
    rmg: &'rmg mut Rmg,
    task_setup: GenericRasterPass<P>,
}

impl<'rmg, P: Default + Clone + 'static> RasterPassBuilder<'rmg, P> {
    /// Changes the push constant definition for the pass. Removes any already recorded draw-calls,
    /// since those depend on a correct push-constant definition.
    pub fn with_push_constant<PC: Default + Clone + 'static>(self) -> RasterPassBuilder<'rmg, PC> {
        assert!(
            std::mem::size_of::<PC>()
                <= self.rmg.config().limit.limits.max_push_constants_size as usize,
            "Push constant size exceeds limit"
        );

        let GenericRasterPass {
            pipeline,
            push: _,
            name,
            color_attachments,
            depth_attachment,
            storage,
            framebuffer_area,
            drawcalls: _,
        } = self.task_setup;

        let new_push_constant = PushConstant::new(PC::default(), vk::ShaderStageFlags::ALL);

        RasterPassBuilder {
            rmg: self.rmg,
            task_setup: GenericRasterPass {
                pipeline,
                push: new_push_constant,
                name,
                color_attachments,
                depth_attachment,
                framebuffer_area,
                storage,
                drawcalls: SmallVec::default(),
            },
        }
    }

    pub fn on_rmg<T>(&mut self, func: impl Fn(&mut Rmg) -> T) -> T {
        func(self.rmg)
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.task_setup.name = Some(name.into());
        self
    }

    /// Signals that the pass will use this image as defined by `usage`.
    /// Iff usage is
    pub fn use_image(mut self, image: ImageHandle, usage: ImageUsage) -> Result<Self, RmgError> {
        match usage {
            ImageUsage::ColorAttachment {
                attachment_index,
                load_op,
                store_op,
                clear_color,
            } => {
                //Make sure the format matches the expectation
                if let Some(expected) = self
                    .task_setup
                    .pipeline
                    .color_attachments
                    .get(attachment_index)
                {
                    if expected != image.format() {
                        return Err(RmgError::ResourceError(ResourceError::FormatMissmatch(
                            *expected,
                            *image.format(),
                        )));
                    }
                } else {
                    return Err(RmgError::ResourceError(
                        ResourceError::InvalidAttachmentIndex(attachment_index),
                    ));
                }
                //passed checks, inserting there
                self.task_setup.color_attachments[attachment_index] =
                    Some((image, usage, load_op, store_op, clear_color));
            }
            ImageUsage::DepthStencilAttachment {
                load_op,
                store_op,
                clear_depth,
            } => {
                if let Some(expected) = &self.task_setup.pipeline.depth_stencil_attachment {
                    if expected != image.format() {
                        return Err(RmgError::ResourceError(ResourceError::FormatMissmatch(
                            *expected,
                            *image.format(),
                        )));
                    }
                } else {
                    return Err(RmgError::ResourceError(
                        ResourceError::UnexpectedDepthAttachment,
                    ));
                }
                //passe checks, setting
                self.task_setup.depth_attachment =
                    Some((image, usage, load_op, store_op, clear_depth));
            }
            _ => {
                self.task_setup.storage.register_image(
                    image,
                    //NOTE we use this in order to not hickup the scheduler's barrier generation...
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    usage.into_access_flags(),
                    usage.into_layout(),
                );
            }
        }

        Ok(self)
    }

    /// Signals all images with the same usage. Nice if you have a set of textures for instance
    /// where you want to mark all as sampled.
    pub fn use_images(
        mut self,
        images: &[ImageHandle],
        usage: ImageUsage,
    ) -> Result<Self, RmgError> {
        if usage.is_attachment() {
            for image in images {
                self = self.use_image(image.clone(), usage)?;
            }
        } else {
            for image in images {
                self.task_setup.storage.register_image(
                    image.clone(),
                    //NOTE we use this in order to not hickup the scheduler's barrier generation...
                    vk::PipelineStageFlags2::ALL_GRAPHICS,
                    usage.into_access_flags(),
                    usage.into_layout(),
                );
            }
        }

        Ok(self)
    }

    /// Signals that the pass will write to this resource
    pub fn use_buffer<T: 'static>(mut self, buffer: BufferHandle<T>, usage: BufferUsage) -> Self {
        let _ = self.task_setup.storage.register_buffer(
            buffer,
            vk::PipelineStageFlags2::ALL_GRAPHICS,
            usage.into_access_flags(),
        );
        self
    }

    /// Signals that the pass will use the sampler
    pub fn use_sampler(mut self, sampler: SamplerHandle) -> Self {
        let _ = self.task_setup.storage.register_sampler(sampler);
        self
    }

    /// Pushes the draw call and draws it to the full framebuffer.
    pub fn draw(self, draw: RasterDrawCall<P>) -> Self {
        self.draw_inner(draw, None)
    }

    /// Pushes the draw call and draws it to `region` of the framebuffer.
    pub fn draw_at(self, draw: RasterDrawCall<P>, region: ImageRegion) -> Self {
        self.draw_inner(draw, Some(region))
    }

    fn draw_inner(mut self, mut draw: RasterDrawCall<P>, region: Option<ImageRegion>) -> Self {
        if let RasterDrawCall::Instanced { instance_count, .. } = &mut draw {
            //make sure we are in the limits, otherwise reduce and throw error
            let limit = self.rmg.config().limit.limits.max_draw_indexed_index_value;

            if limit <= *instance_count {
                #[cfg(feature = "log")]
                log::error!(
                    "Instance count {instance_count} exceeds the limit of {limit}, reducing to limits"
                );
                *instance_count = limit;
            }
        }

        //check that the drawcall's index buffer has the usage set
        assert!(
            draw.index_buffer()
                .buf_desc()
                .usage
                .contains(vk::BufferUsageFlags::INDEX_BUFFER)
        );
        self.task_setup.drawcalls.push((draw, region));

        self
    }

    pub fn finish(mut self) -> Result<GenericRasterPass<P>, RmgError> {
        //Check that all color and depth attachments are used
        // then set the framebuffer area.

        assert_eq!(
            self.task_setup.color_attachments.len(),
            self.task_setup.pipeline.color_attachments.len()
        );
        for idx in 0..self.task_setup.pipeline.color_attachments.len() {
            if self.task_setup.color_attachments[idx].is_none() {
                return Err(RmgError::ResourceError(
                    ResourceError::InvalidAttachmentIndex(idx),
                ));
            }
        }

        //Make sure either both are set, or unset
        match (
            &self.task_setup.depth_attachment,
            &self.task_setup.pipeline.depth_stencil_attachment,
        ) {
            (Some(_), Some(_)) | (None, None) => {}
            (None, Some(_)) => {
                return Err(RmgError::ResourceError(
                    ResourceError::UnexpectedDepthAttachment,
                ));
            }
            (Some(_), None) => {
                return Err(RmgError::ResourceError(ResourceError::NoDepthAttachment));
            }
        }

        //make sure all framebuffer use the same extent
        let Some(reference_size) = self
            .task_setup
            .color_attachments
            .first()
            .map(|i| i.as_ref().unwrap().0.extent_2d())
            .or(self
                .task_setup
                .depth_attachment
                .as_ref()
                .map(|i| i.0.extent_2d()))
        else {
            return Err(RmgError::ResourceError(ResourceError::NoAttachments));
        };
        for extent in self.task_setup.color_attachments[1..]
            .iter()
            .map(|i| i.as_ref().unwrap().0.extent_2d())
            .chain(
                self.task_setup
                    .depth_attachment
                    .iter()
                    .map(|i| i.0.extent_2d()),
            )
        {
            if extent != reference_size {
                return Err(RmgError::ResourceError(
                    ResourceError::AttachmentExtentMissmatch(reference_size, extent),
                ));
            }
        }
        //can set the framebuffer size
        self.task_setup.framebuffer_area = ImageRegion {
            extent: vk::Extent3D {
                width: reference_size.width,
                height: reference_size.height,
                depth: 1,
            },
            ..ImageRegion::ZERO
        };

        //Passed all validation, return the finished thing
        Ok(self.task_setup)
    }
}

impl Rmg {
    pub fn new_raster_pass<'rmg>(
        &'rmg mut self,
        pipeline: impl Into<OoS<RasterPipeline>>,
    ) -> RasterPassBuilder<'rmg, ()> {
        RasterPassBuilder {
            rmg: self,
            task_setup: GenericRasterPass::init(pipeline),
        }
    }

    ///Creates a new [`RasterPipeline`] for the given vertex and fragment shaders.
    ///
    /// Use the `configure_pipeline` call to reconfigure the graphics pipeline.
    ///
    /// By default it is a vertex-buffer less pipeline that uses default alpha blending for color attachments,
    /// and the _less-test_ for depth-tests, if a depth-attchment is provided.
    /// The color attachments are assumed to not be multisampled.
    /// For more information see the source code, or just configure the whole thing to your liking.
    ///
    /// # Important
    ///
    /// The [`GenericRasterPass`] assumes that the scissors and viewport are dynamic state, and that the pipeline uses no
    /// vertex-inputs (i.e. no vertex buffer is supplied to the draw command, only a index buffer). Everything else
    /// can be configured. When in doubt, use validation-layers as always.
    #[allow(clippy::too_many_arguments)]
    pub fn new_raster_pipeline(
        &self,
        vertex_entry_point: &str,
        vertex_shader: impl Into<OoS<ShaderModule>>,
        fragment_entry_point: &str,
        fragment_shader: impl Into<OoS<ShaderModule>>,
        color_attachment_formats: impl Into<SmallVec<[vk::Format; 4]>>,
        depth_attachment_format: Option<vk::Format>,
        configure_pipeline: impl Fn(
            vk::GraphicsPipelineCreateInfo<'_>,
        ) -> vk::GraphicsPipelineCreateInfo<'_>,
    ) -> Result<RasterPipeline, RmgError> {
        let color_attachments = color_attachment_formats.into();
        let depth_stencil_attachment = depth_attachment_format;

        let vertex_shader_stage = ShaderStage::from_module(
            vertex_shader.into(),
            vk::ShaderStageFlags::VERTEX,
            vertex_entry_point.to_owned(),
        );

        let fragment_shader_stage = ShaderStage::from_module(
            fragment_shader.into(),
            vk::ShaderStageFlags::FRAGMENT,
            fragment_entry_point.to_owned(),
        );

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

        let depth_stencil_state = if depth_stencil_attachment.is_some() {
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_compare_op(vk::CompareOp::LESS)
                .depth_write_enable(true)
                .depth_test_enable(true)
                .depth_bounds_test_enable(false)
                .stencil_test_enable(false)
        } else {
            vk::PipelineDepthStencilStateCreateInfo::default()
        };

        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        //no other dynamic state

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .primitive_restart_enable(false)
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .cull_mode(vk::CullModeFlags::NONE)
            .depth_bias_enable(false)
            .depth_clamp_enable(false)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0);

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
            .vertex_input_state(&vertex_input_state);

        //Call the handler, if set
        let create_info = configure_pipeline(create_info);

        let pipeline = GraphicsPipeline::new_dynamic_pipeline(
            &self.ctx.device,
            create_info,
            self.resources.bindless_layout(),
            &[vertex_shader_stage, fragment_shader_stage],
            &color_attachments,
            depth_attachment_format,
        )
        .map_err(MarpiiError::from)?;

        Ok(RasterPipeline {
            inner: Arc::new(pipeline),
            color_attachments,
            depth_stencil_attachment,
        })
    }
}
