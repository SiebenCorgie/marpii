use crate::context::Device;
use std::sync::Arc;

use super::{DescriptorSetLayout, PushConstant};

pub(crate) mod compute;
pub mod graphics;

pub struct PipelineLayout {
    pub device: Arc<Device>,
    pub layout: ash::vk::PipelineLayout,
}

impl PipelineLayout {
    ///Highlevel wrapper around `new` that takes the descriptorset layouts and push constants directly.
    pub fn from_layout_push<T: 'static>(
        device: &Arc<Device>,
        descriptor_set_layouts: &[(u32, DescriptorSetLayout)],
        push_constant: &PushConstant<T>,
    ) -> Result<Self, ash::vk::Result> {
        let inner_layouts = descriptor_set_layouts
            .iter()
            .map(|(_idx, ly)| ly.inner)
            .collect::<Vec<_>>();

        Self::new(device, &inner_layouts, &[*push_constant.range()])
    }

    pub fn new(
        device: &Arc<Device>,
        descriptor_set_layouts: &[ash::vk::DescriptorSetLayout],
        push_constant_ranges: &[ash::vk::PushConstantRange],
    ) -> Result<Self, ash::vk::Result> {
        let create_info = ash::vk::PipelineLayoutCreateInfo::default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use compute::ComputePipeline;
    use graphics::GraphicsPipeline;
    use static_assertions::assert_impl_all;

    #[test]
    fn impl_send_sync() {
        assert_impl_all!(PipelineLayout: Send, Sync);
        assert_impl_all!(ComputePipeline: Send, Sync);
        assert_impl_all!(GraphicsPipeline: Send, Sync);
    }
}
