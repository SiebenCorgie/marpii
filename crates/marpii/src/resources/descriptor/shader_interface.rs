///Reflection interface for some shader source.
pub struct Reflection {
    inner: rspirv_reflect::Reflection,
}

fn descriptor_count_from_binding(binding: rspirv_reflect::BindingCount) -> u32 {
    match binding {
        rspirv_reflect::BindingCount::One => 1,
        rspirv_reflect::BindingCount::StaticSized(n) => n as u32,
        rspirv_reflect::BindingCount::Unbounded => {
            #[cfg(feature = "logging")]
            log::error!("Found unbound decriptor set, can't reflect correct size. Using 1 instead");
            1
        }
    }
}

//TODO handle bindless gracefully. Happens if we have an unbound number of descriptors. for a binding layout.
impl Reflection {
    pub fn new_from_code(shader_code: &[u8]) -> Result<Self, rspirv_reflect::ReflectError> {
        Ok(Reflection {
            inner: rspirv_reflect::Reflection::new_from_spirv(shader_code)?,
        })
    }

    ///Generates binding layouts for each descriptor set. Sets the shader `stage_flags` of each binding with the supplied ones.
    pub fn get_bindings(
        &self,
        stage_flags: ash::vk::ShaderStageFlags,
    ) -> Result<Vec<(u32, Vec<ash::vk::DescriptorSetLayoutBinding>)>, rspirv_reflect::ReflectError>
    {
        #[cfg(feature = "shader_reflection_verbose")]
        log::info!("Reflection:");

        Ok(self
            .inner
            .get_descriptor_sets()?
            .into_iter()
            .map(|(set_idx, descriptor_set)| {
                #[cfg(feature = "shader_reflection_verbose")]
                log::info!("  Set {}", set_idx);

                //Generate the binding information
                let set_bindings = descriptor_set
                    .into_iter()
                    .map(|(binding_id, binding)| {
                        #[cfg(feature = "shader_reflection_verbose")]
                        log::info!("    Binding {} = {:?}", binding_id, binding);

                        ash::vk::DescriptorSetLayoutBinding {
                            binding: binding_id,
                            descriptor_count: descriptor_count_from_binding(binding.binding_count),
                            descriptor_type: ash::vk::DescriptorType::from_raw(i32::from_be_bytes(
                                binding.ty.0.to_be_bytes(),
                            )),
                            stage_flags,
                            ..Default::default()
                        }
                    })
                    .collect::<Vec<_>>();
                (set_idx, set_bindings)
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_impl_all;

    #[test]
    fn impl_send_sync() {
        assert_impl_all!(Reflection: Send, Sync);
    }
}
