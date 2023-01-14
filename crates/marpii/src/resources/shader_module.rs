use crate::{context::Device, error::ShaderError};
use std::{ffi::CString, mem::size_of, path::Path, sync::Arc};

use super::Reflection;

///Single shader module
pub struct ShaderModule {
    pub device: Arc<Device>,
    pub inner: ash::vk::ShaderModule,
    ///saves the descriptor interface of this module where each bindings `shader_stage` is marked as `ALL`.
    /// for best performance those might be optimised by the user.
    #[cfg(feature = "shader_reflection")]
    pub reflection: Reflection,
}

impl ShaderModule {
    ///Reads file at `path`, checks that it is a spirv file and, if so, tries to create the shader module from it.
    pub fn new_from_file(
        device: &Arc<Device>,
        file: impl AsRef<Path>,
    ) -> Result<Self, ShaderError> {
        //try to read the file. Throws an error if it is none-existent etc.
        let mut file = std::fs::File::open(file)?;
        let code = ash::util::read_spv(&mut file)?;

        //Now use the normal new function for the rest
        Self::new(device, &code)
    }

    pub fn new_from_bytes<'a>(device: &Arc<Device>, bytes: &'a [u8]) -> Result<Self, ShaderError> {
        #[cfg(feature = "logging")]
        log::trace!("read shader module from byte array");
        let words = ash::util::read_spv(&mut std::io::Cursor::new(bytes)).unwrap();
        Self::new(device, &words)
    }

    pub fn new(device: &Arc<Device>, code: &[u32]) -> Result<Self, ShaderError> {
        #[cfg(feature = "logging")]
        log::trace!("Shader Module new");

        let create_info = ash::vk::ShaderModuleCreateInfo::builder().code(code);
        let module = unsafe { device.inner.create_shader_module(&create_info, None)? };
        #[cfg(feature = "shader_reflection")]
        let reflection = {
            #[cfg(feature = "logging")]
            log::trace!("Reflecting shader module");

            //cast the code to an u8. Should be save since the create_shader_module would have paniced
            // if the shader code was not /correct/
            let len = code.len() * size_of::<u32>();
            let code = unsafe { core::slice::from_raw_parts(code.as_ptr() as *const u8, len) };
            //FIXME: currently the reflection error can't be cast to anyhow's error. Should be fixed when
            //       https://github.com/Traverse-Research/rspirv-reflect/pull/24 is merged.
            let reflection = Reflection::new_from_code(code)
                .map_err(|e| ShaderError::ReflectionError(format!("{}", e)))?;
            reflection
        };

        #[cfg(feature = "logging")]
        log::trace!("Building shader module from code");

        Ok(ShaderModule {
            device: device.clone(),
            inner: module,
            #[cfg(feature = "shader_reflection")]
            reflection,
        })
    }

    ///Creates a descriptorset layout for each descriptor set reflection information. The `u32` in the returned list is the set-id of each descriptor set as found in the
    ///reflection information.
    ///
    ///If you need finer control, consider creating the layouts yourself and only refere to the inner `descriptor_interface`.
    #[cfg(feature = "shader_reflection")]
    pub fn create_descriptor_set_layouts(
        &self,
    ) -> Result<Vec<(u32, super::DescriptorSetLayout)>, ShaderError> {
        use super::DescriptorSetLayout;

        let bindings = self
            .reflection
            .get_bindings(ash::vk::ShaderStageFlags::ALL)
            .map_err(|e| ShaderError::ReflectionError(format!("{}", e)))?;

        let mut layouts = Vec::with_capacity(bindings.len());
        for (setid, bindings) in &bindings {
            let layout = DescriptorSetLayout::new(&self.device, &bindings)?;
            layouts.push((*setid, layout));
        }

        Ok(layouts)
    }

    #[cfg(feature = "shader_reflection")]
    pub fn get_bindings(
        &self,
        stage_flags: ash::vk::ShaderStageFlags,
    ) -> Result<Vec<(u32, Vec<ash::vk::DescriptorSetLayoutBinding>)>, rspirv_reflect::ReflectError>
    {
        self.reflection.get_bindings(stage_flags)
    }

    ///Creates shader stage from module. Panics if the entry_name is not utf8
    pub fn into_shader_stage(
        self,
        stage: ash::vk::ShaderStageFlags,
        entry_name: impl Into<String>,
    ) -> ShaderStage {
        ShaderStage {
            module: StageModule::Owned(self),
            stage,
            entry_name: CString::new(entry_name.into()).unwrap(),
        }
    }
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_shader_module(self.inner, None) }
    }
}

enum StageModule {
    Owned(ShaderModule),
    Shared(Arc<ShaderModule>),
}

impl StageModule {
    fn module(&self) -> &ShaderModule {
        match self {
            StageModule::Owned(s) => &s,
            StageModule::Shared(s) => &s,
        }
    }
}
///Build from a [ShaderModule] this type knows its entry point name as well as the shader stage at which it is executed.
pub struct ShaderStage {
    ///Keeps the referenced shader module alive until the stage is dropped.
    module: StageModule,
    pub stage: ash::vk::ShaderStageFlags,
    pub entry_name: CString,
}

impl ShaderStage {
    ///Creates shader stage from a shared module. Panics if the entry_name is not utf8
    pub fn from_shared_module(
        module: Arc<ShaderModule>,
        stage: ash::vk::ShaderStageFlags,
        entry_name: String,
    ) -> Self {
        ShaderStage {
            module: StageModule::Shared(module),
            stage,
            entry_name: CString::new(entry_name).unwrap(),
        }
    }

    pub fn module(&self) -> &ShaderModule {
        self.module.module()
    }
    pub fn inner(&self) -> &ash::vk::ShaderModule {
        &self.module.module().inner
    }

    pub fn as_create_info<'a>(
        &'a self,
        specialization_info: Option<&'a ash::vk::SpecializationInfo>,
    ) -> ash::vk::PipelineShaderStageCreateInfoBuilder<'a> {
        let mut builder = ash::vk::PipelineShaderStageCreateInfo::builder()
            .stage(self.stage)
            .module(self.module.module().inner)
            .name(self.entry_name.as_c_str());

        if let Some(special) = specialization_info {
            builder = builder.specialization_info(special)
        }

        builder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_impl_all;

    #[test]
    fn impl_send_sync() {
        assert_impl_all!(ShaderModule: Send, Sync);
    }
}
