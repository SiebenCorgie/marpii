use std::sync::Arc;

use super::PipelineLayout;
use crate::ash::vk;
use crate::context::Device;
use crate::error::PipelineError;
use crate::resources::ShaderStage;
use oos::OoS;

///Renderpass describing the order of shader invocation. Note that this is only a thin wrapper over the creation and destruction process. If possible try to use the dnamic_rendering extension which
/// integrates much better with marpii.
pub struct RenderPass {
    device: Arc<Device>,
    pub inner: vk::RenderPass,
}

impl RenderPass {
    pub fn new(
        device: &Arc<Device>,
        create_info: vk::RenderPassCreateInfo,
    ) -> Result<Self, vk::Result> {
        let renderpass = unsafe { device.inner.create_render_pass(&create_info, None)? };
        Ok(RenderPass {
            device: device.clone(),
            inner: renderpass,
        })
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_render_pass(self.inner, None) }
    }
}

///Pipeline that manages its own lifetime and keeps resources alive needed for its correct execution.
///
/// Note that marpii prefers the [dynamic-rendering](https://lesleylai.info/en/vk-khr-dynamic-rendering/) based graphics pipeline creation. Therefore `renderpass` will usually be None.
pub struct GraphicsPipeline {
    pub device: Arc<Device>,
    pub pipeline: ash::vk::Pipeline,
    pub layout: OoS<PipelineLayout>,
    /// if not using dynamic-rendering, this is some renderpass which is kept alive.
    //TODO: Change to marpii renderpass if such a thing is created at some point.
    pub renderpass: Option<RenderPass>,
}

impl GraphicsPipeline {
    ///Simplest graphics pipeline wrapper. Assumes that `create_info` is valid. Sets the `layout` and if provided the `renderpass`
    /// on the `create_info` before executing. Might fail if validation is activated and an error is found.
    pub fn new(
        device: &Arc<Device>,
        create_info: ash::vk::GraphicsPipelineCreateInfo,
        layout: impl Into<OoS<PipelineLayout>>,
        renderpass: RenderPass,
    ) -> Result<Self, PipelineError> {
        let layout = layout.into();
        let mut create_info = create_info.layout(layout.layout);
        create_info = create_info.render_pass(renderpass.inner);

        let mut pipelines = unsafe {
            match device.inner.create_graphics_pipelines(
                ash::vk::PipelineCache::null(),
                core::slice::from_ref(&*create_info),
                None,
            ) {
                Ok(p) => p,
                Err((_plines, err)) => {
                    return Err(err.into());
                }
            }
        };

        if pipelines.len() != 1 {
            return Err(PipelineError::Allocation);
        }

        let pipeline = pipelines.remove(0);

        Ok(GraphicsPipeline {
            device: device.clone(),
            pipeline,
            layout,
            renderpass: Some(renderpass),
        })
    }

    ///Creates a new `DynamicRendering` pipeline where the attachment images
    /// are defined through the order in `color_formats` and `depth_format`.
    pub fn new_dynamic_pipeline(
        device: &Arc<Device>,
        create_info: ash::vk::GraphicsPipelineCreateInfo,
        layout: impl Into<OoS<PipelineLayout>>,
        shader_stages: &[ShaderStage],
        color_formats: &[vk::Format],
        depth_format: vk::Format,
    ) -> Result<Self, PipelineError> {
        let layout = layout.into();
        assert!(
            device.extension_enabled_cstr(ash::khr::dynamic_rendering::NAME),
            "DynamicRenderingKHR extension not activated!"
        );

        let mut pipline_rendering_create_info = vk::PipelineRenderingCreateInfo::default()
            .depth_attachment_format(depth_format)
            .color_attachment_formats(&color_formats);

        let stages = shader_stages
            .iter()
            .map(|s| *s.as_create_info(None))
            .collect::<Vec<_>>();

        //make renderpass nullptr
        let create_info = create_info
            .stages(&stages)
            .render_pass(vk::RenderPass::null())
            .layout(layout.layout)
            //now push dynamic extension
            .push_next(&mut pipline_rendering_create_info);

        let mut pipelines = unsafe {
            match device.inner.create_graphics_pipelines(
                ash::vk::PipelineCache::null(),
                core::slice::from_ref(&*create_info),
                None,
            ) {
                Ok(p) => p,
                Err((_plines, err)) => {
                    return Err(err.into());
                }
            }
        };

        if pipelines.len() != 1 {
            return Err(PipelineError::Allocation);
        }

        let pipeline = pipelines.remove(0);

        Ok(GraphicsPipeline {
            device: device.clone(),
            pipeline,
            layout,
            renderpass: None,
        })
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_pipeline(self.pipeline, None) }
    }
}

//TODO: Expose khr-dynamic-rendering based graphics pipeline. Makes stuff alot easier compared to the old
//      graphics pipelines.
