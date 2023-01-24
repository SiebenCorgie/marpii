use std::sync::Arc;

use crate::{context::Device, error::PipelineError, resources::shader_module::ShaderStage, OoS};

use super::PipelineLayout;

//TODO: simple, resource tracking compute pipeline.
//      catch resources via closure, that passes each resource
//      as dyn Any into a resource-map...chache...thingy

///Pipeline that manages its own lifetime and keeps resources alive needed for its correct execution.
pub struct ComputePipeline {
    pub device: Arc<Device>,
    pub pipeline: ash::vk::Pipeline,
    pub layout: OoS<PipelineLayout>,
}

impl ComputePipeline {
    pub fn new<'a, L: 'static>(
        device: &Arc<Device>,
        stage: &'a ShaderStage,
        specialization_info: Option<&'a ash::vk::SpecializationInfo>,
        layout: L,
    ) -> Result<Self, PipelineError>
    where
        L: Into<OoS<PipelineLayout>>,
    {
        let layout = layout.into();
        let create_info = ash::vk::ComputePipelineCreateInfo::builder()
            .stage(*stage.as_create_info(specialization_info))
            .layout(layout.layout);

        let mut pipelines = unsafe {
            match device.inner.create_compute_pipelines(
                ash::vk::PipelineCache::null(),
                &[*create_info],
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

        Ok(ComputePipeline {
            device: device.clone(),
            pipeline,
            layout,
        })
    }
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_pipeline(self.pipeline, None) }
    }
}
