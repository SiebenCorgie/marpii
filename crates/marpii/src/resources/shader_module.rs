use crate::context::Device;
use std::{ffi::CString, mem::size_of, path::Path, sync::Arc};

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
            let layout = DescriptorSetLayout::new(self.device.clone(), &bindings)?;
            layouts.push((*setid, layout));
        }

        Ok(layouts)
    }

    ///Creates shader stage from module. Panics if the entry_name is not utf8
    pub fn into_shader_stage(
        self,
        stage: ash::vk::ShaderStageFlags,
        entry_name: String,
    ) -> ShaderStage {
        ShaderStage {
            module: self,
            stage,
            entry_name: CString::new(entry_name).unwrap(),
        }
    }
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_shader_module(self.module, None) }
    }
}

///Build from a [ShaderModule] this type knows its entry point name as well as the shader stage at which it is executed.
pub struct ShaderStage {
    ///Keeps the referenced shader module alive until the stage is dropped.
    pub module: ShaderModule,
    pub stage: ash::vk::ShaderStageFlags,
    pub entry_name: CString,
}

impl ShaderStage {
    pub fn as_create_info<'a>(
        &'a self,
        specialization_info: Option<&'a ash::vk::SpecializationInfo>,
    ) -> ash::vk::PipelineShaderStageCreateInfoBuilder<'a> {
        let mut builder = ash::vk::PipelineShaderStageCreateInfo::builder()
            .stage(self.stage)
            .module(self.module.module)
            .name(self.entry_name.as_c_str());

        if let Some(special) = specialization_info {
            builder = builder.specialization_info(special)
        }

        builder
    }
}
