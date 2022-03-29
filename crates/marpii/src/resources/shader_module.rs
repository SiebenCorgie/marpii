use crate::context::Device;
use std::{mem::size_of, path::Path, sync::Arc};

use super::Reflection;

///Single shader module
pub struct ShaderModule {
    pub device: Arc<Device>,
    pub module: ash::vk::ShaderModule,
    ///saves the descriptor interface of this module where each bindings `shader_stage` is marked as `ALL`.
    /// for best performance those might be optimitzed by the user.
    #[cfg(feature = "shader_reflection")]
    pub descriptor_interface: Vec<(u32, Vec<ash::vk::DescriptorSetLayoutBinding>)>,
}

impl ShaderModule {
    ///Reads file at `path`, checks that it is a spirv file and, if so, tries to create the shader module from it.
    pub fn new_from_file(
        device: &Arc<Device>,
        file: impl AsRef<Path>,
    ) -> Result<Self, anyhow::Error> {
        //try to read the file. Throws an error if it is none-existent etc.
        let mut file = std::fs::File::open(file)?;
        let code = ash::util::read_spv(&mut file)?;

        //Now use the normal new function for the rest
        Self::new(device, &code)
    }
    pub fn new(device: &Arc<Device>, code: &[u32]) -> Result<Self, anyhow::Error> {
        let create_info = ash::vk::ShaderModuleCreateInfo::builder().code(code);

        let module = unsafe { device.inner.create_shader_module(&create_info, None)? };

        #[cfg(feature = "shader_reflection")]
        let descriptor_interface = {
            //cast the code to an u8. Should be save since the create_shader_module would have paniced
            // if the shader code was not /correct/
            let len = code.len() * size_of::<u32>();
            let code = unsafe { core::slice::from_raw_parts(code.as_ptr() as *const u8, len) };
            //FIXME: currently the reflection error can't be cast to anyhow's error. Should be fixed when
            //       https://github.com/Traverse-Research/rspirv-reflect/pull/24 is merged.
            let reflection = Reflection::new_from_code(code)
                .map_err(|e| anyhow::format_err!("Reflection error: {:?}", e))?;

            reflection
                .get_bindings(ash::vk::ShaderStageFlags::ALL)
                .map_err(|e| anyhow::format_err!("Reflection error: {:?}", e))?
        };
        Ok(ShaderModule {
            device: device.clone(),
            module,
            #[cfg(feature = "shader_reflection")]
            descriptor_interface,
        })
    }

    ///Creates a shade stage from this module. Basically a speciallized version of this shader module that knows
    ///when (shader stage) and with what [specializations](https://www.khronos.org/registry/vulkan/specs/1.3-extensions/man/html/VkSpecializationInfo.html).
    pub fn as_stage<'a>(
        &'a self,
        stage: ash::vk::ShaderStageFlags,
        specialization_info: Option<&'a ash::vk::SpecializationInfo>,
        name: Option<&'a str>,
        extend: Option<
            impl FnMut(
                ash::vk::PipelineShaderStageCreateInfoBuilder,
            ) -> ash::vk::PipelineShaderStageCreateInfoBuilder,
        >,
    ) -> ash::vk::PipelineShaderStageCreateInfoBuilder<'a> {
        let mut info = ash::vk::PipelineShaderStageCreateInfo::builder()
            .module(self.module)
            .stage(stage);

        if let Some(n) = name {
            //Gotta check that it really is ASCII
            //TODO: Do we have to add a \0?
            if n.is_ascii() {
                let cstr = unsafe { std::ffi::CStr::from_ptr(n.as_ptr() as *const i8) };
                info = info.name(cstr);
            } else {
                #[cfg(feature = "logging")]
                log::error!("Could not name ShaderStage, name was not ASCII");
            }
        }

        if let Some(si) = specialization_info {
            info = info.specialization_info(si);
        }

        if let Some(mut ext) = extend {
            info = ext(info);
        }
        info
    }

    ///Creates a descriptorset layout for each descriptor set reflection information. The `u32` in the returned list is the set-id of each descriptor set as found in the
    ///reflection information.
    ///
    ///If you need finer control, consider creating the layouts yourself and only refere to the inner `descriptor_interface`.
    #[cfg(feature = "shader_reflection")]
    pub fn create_descriptor_set_layouts(
        &self,
    ) -> Result<Vec<(u32, super::DescriptorSetLayout)>, anyhow::Error> {
        use super::DescriptorSetLayout;

        let mut layouts = Vec::with_capacity(self.descriptor_interface.len());
        for (setid, bindings) in &self.descriptor_interface {
            let layout = DescriptorSetLayout::new(self.device.clone(), &bindings, None)?;
            layouts.push((*setid, layout));
        }

        Ok(layouts)
    }
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_shader_module(self.module, None) }
    }
}
