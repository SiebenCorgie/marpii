use std::sync::Arc;

use crate::context::Device;

#[cfg(feature = "shader_reflection")]
pub(crate) mod shader_interface;

pub struct DescriptorSetLayout {
    pub device: Arc<Device>,
    pub inner: ash::vk::DescriptorSetLayout,
}

impl DescriptorSetLayout {
    ///Generates a descriptor set layout from a set of bindings. The easiest way to optain those is to use
    /// [reflection](shader_interface::Reflection). Or by hand creating them.
    pub fn new(
        device: Arc<Device>,
        bindings: &[ash::vk::DescriptorSetLayoutBinding],
        extend: Option<
            &mut dyn FnMut(
                ash::vk::DescriptorSetLayoutCreateInfoBuilder,
            ) -> ash::vk::DescriptorSetLayoutCreateInfoBuilder,
        >,
    ) -> Result<Self, ash::vk::Result> {
        let mut info = ash::vk::DescriptorSetLayoutCreateInfo::builder().bindings(bindings);

        if let Some(ext) = extend {
            info = ext(info);
        }

        let layout = unsafe { device.inner.create_descriptor_set_layout(&info, None)? };

        Ok(DescriptorSetLayout {
            device,
            inner: layout,
        })
    }
}

impl Drop for DescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .inner
                .destroy_descriptor_set_layout(self.inner, None)
        }
    }
}
