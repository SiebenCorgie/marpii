use crate::context::Device;
use std::sync::Arc;

pub struct PipelineLayout {
    pub device: Arc<Device>,
    pub layout: ash::vk::PipelineLayout,
}

impl PipelineLayout {
    pub fn new(
        device: &Arc<Device>,
        descriptor_set_layouts: &[ash::vk::DescriptorSetLayout],
        push_constant_ranges: &[ash::vk::PushConstantRange],
    ) -> Result<Self, ash::vk::Result> {
        let create_info = ash::vk::PipelineLayoutCreateInfo::builder()
            .push_constant_ranges(push_constant_ranges)
            .set_layouts(descriptor_set_layouts);

        let layout = unsafe { device.inner.create_pipeline_layout(&create_info, None)? };

        Ok(PipelineLayout {
            device: device.clone(),
            layout,
        })
    }
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_pipeline_layout(self.layout, None) }
    }
}
