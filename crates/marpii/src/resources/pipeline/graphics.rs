use std::sync::Arc;

use crate::context::Device;

use super::PipelineLayout;

///Pipeline that manages its own lifetime and keeps resources alive needed for its correct execution.
///
/// Note that marpii preferes the [dynamic-rendering](https://lesleylai.info/en/vk-khr-dynamic-rendering/) based graphics pipeline creation. Therefore `renderpass` will usually be None.
pub struct GraphicsPipeline {
    pub device: Arc<Device>,
    pub pipeline: ash::vk::Pipeline,
    pub layout: PipelineLayout,
    /// if not using dynamic-rendering, this is some renderpass which is kept alive.
    //TODO: Change to marpii renderpass if such a thing is created at some point.
    pub renderpass: Option<ash::vk::RenderPass>,
}

impl GraphicsPipeline {
    ///Simplest graphics pipeline wrapper. Assumes that `create_info` is valid. Sets the `layout` and if provided the `renderpass`
    /// on the `create_info` before executing. Might fail if validation is activated and an error is found.
    pub fn new(
        device: &Arc<Device>,
        create_info: ash::vk::GraphicsPipelineCreateInfoBuilder,
        layout: PipelineLayout,
        renderpass: Option<ash::vk::RenderPass>,
    ) -> Result<Self, anyhow::Error> {
        let mut create_info = create_info.layout(layout.layout);
        if let Some(rp) = renderpass {
            create_info = create_info.render_pass(rp);
        }

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
            anyhow::bail!("Pipeline count wasn't 1, was {}", pipelines.len());
        }

        let pipeline = pipelines.remove(0);

        Ok(GraphicsPipeline {
            device: device.clone(),
            pipeline,
            layout,
            renderpass,
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
