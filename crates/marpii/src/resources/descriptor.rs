use ahash::AHashMap;
use oos::OoS;

use crate::{context::Device, error::DescriptorError};
use std::sync::Arc;

#[cfg(feature = "shader_reflection")]
use super::ShaderModule;

#[cfg(feature = "shader_reflection")]
pub(crate) mod shader_interface;

/// Wrapped descriptor set layout. Can either be created through [new](DescriptorSetLayout::new), or by filling
/// the struct. Handles on-drop destruction of the resource.
pub struct DescriptorSetLayout {
    pub device: Arc<Device>,
    pub inner: ash::vk::DescriptorSetLayout,
}

impl DescriptorSetLayout {
    ///Generates a descriptor set layout from a set of bindings. The easiest way to optain those is to use
    /// [reflection](shader_interface::Reflection). Or by hand creating them.
    pub fn new(
        device: &Arc<Device>,
        bindings: &[ash::vk::DescriptorSetLayoutBinding],
    ) -> Result<Self, ash::vk::Result> {
        let info = ash::vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings);

        let layout = unsafe { device.inner.create_descriptor_set_layout(&info, None)? };

        Ok(DescriptorSetLayout {
            device: device.clone(),
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

pub struct DescriptorPool {
    pub device: Arc<Device>,
    ///actual inner set
    pub inner: ash::vk::DescriptorPool,
    ///Allocatable sizes
    pub sizes: AHashMap<ash::vk::DescriptorType, u32>,

    ///True if descriptor sets can be freed for this pool
    pub can_free: bool,
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_descriptor_pool(self.inner, None) }
    }
}

impl DescriptorPool {
    ///Simple [ash::vk::DescriptorPool](ash::vk::DescriptorPool) creation wrapper. Expects that `sizes` has at maximum one size per descriptor type.
    pub fn new(
        device: &Arc<Device>,
        flags: ash::vk::DescriptorPoolCreateFlags,
        sizes: &[ash::vk::DescriptorPoolSize],
        set_count: u32,
    ) -> Result<Self, DescriptorError> {
        let create_info = ash::vk::DescriptorPoolCreateInfo::default()
            .flags(flags)
            .max_sets(set_count)
            .pool_sizes(sizes);

        let pool = unsafe { device.inner.create_descriptor_pool(&create_info, None)? };

        let can_free = flags.contains(ash::vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);

        Ok(DescriptorPool {
            can_free,
            device: device.clone(),
            inner: pool,
            sizes: sizes.iter().fold(AHashMap::default(), |mut map, size| {
                if let Some(count) = map.get_mut(&size.ty) {
                    *count += size.descriptor_count;
                } else {
                    map.insert(size.ty, size.descriptor_count);
                }

                map
            }),
        })
    }

    ///Creates a new pool that allocates exactly enough descriptors that `module` can be bound `count` times
    #[cfg(feature = "shader_reflection")]
    pub fn new_for_module(
        device: &Arc<Device>,
        flags: ash::vk::DescriptorPoolCreateFlags,
        module: &ShaderModule,
        count: u32,
    ) -> Result<Self, DescriptorError> {
        //first step is to sort out our sizes.
        let mut map = AHashMap::default();

        //FIXME: currently the reflection error can't be cast to anyhow's error. Should be fixed when
        //       https://github.com/Traverse-Research/rspirv-reflect/pull/24 is merged.
        for (_set, set_bindings) in module.get_bindings(ash::vk::ShaderStageFlags::ALL).unwrap() {
            for binding in set_bindings.iter() {
                if let Some(count) = map.get_mut(&binding.descriptor_type) {
                    *count += binding.descriptor_count;
                } else {
                    map.insert(binding.descriptor_type, binding.descriptor_count);
                }
            }
        }

        //collect into sizes
        let sizes = map
            .into_iter()
            .map(|(ty, descriptor_count)| ash::vk::DescriptorPoolSize {
                descriptor_count,
                ty,
            })
            .collect::<Vec<_>>();

        //now we can use the default create function
        Self::new(device, flags, &sizes, count)
    }

    fn free(&self, set: &ash::vk::DescriptorSet) -> Result<(), DescriptorError> {
        if self.can_free {
            unsafe {
                self.device
                    .inner
                    .free_descriptor_sets(self.inner, core::slice::from_ref(set))
                    .map_err(|e| e.into())
            }
        } else {
            Err(DescriptorError::UnFreeable)
        }
    }

    fn device(&self) -> &Arc<Device> {
        &self.device
    }
}

pub trait DescriptorAllocator {
    fn allocate(
        self,
        layout: &ash::vk::DescriptorSetLayout,
    ) -> Result<DescriptorSet, DescriptorError>;
}

impl DescriptorAllocator for OoS<DescriptorPool> {
    fn allocate(
        self,
        layout: &ash::vk::DescriptorSetLayout,
    ) -> Result<DescriptorSet, DescriptorError> {
        let create_info = ash::vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.inner)
            .set_layouts(core::slice::from_ref(layout));

        let mut sets = unsafe { self.device.inner.allocate_descriptor_sets(&create_info)? };

        if sets.len() == 0 {
            return Err(DescriptorError::Allocation {
                requested: 1,
                count: 0,
            });
        }

        #[cfg(feature = "logging")]
        if sets.len() > 1 {
            log::warn!(
                "Allocate too many descriptor sets, expected 1 got {}",
                sets.len()
            );
        }

        let set = sets.remove(0);

        Ok(DescriptorSet {
            inner: set,
            is_freed: false,
            parent_pool: self,
        })
    }
}

///Simple wrapper around [ash::vk::DescriptorSet](ash::vk::DescriptorSet). On its own it does only implement tracking of it internal `freed` state. If true this means an implementation of [DescriptorAllocator] might have freed `self.inner` at some point.
pub struct DescriptorSet {
    ///The pool this set was allocated from. Is used when dropping `self` if the pool implements freeing allocations.
    pub parent_pool: OoS<DescriptorPool>,
    pub is_freed: bool,
    pub inner: ash::vk::DescriptorSet,
}

impl DescriptorSet {
    ///Executes the write operation on the descriptor set. Does no checking agains the descriptor sets layout. If validation is
    ///activated this might fail.
    ///
    /// the `set` field of `write` is update with this descriptor set's handle before execution.
    ///
    /// # Performance
    /// Note the vulkan `update_descriptor_sets` function can update multiple descriptor bindings at once. If this is what you need,
    /// consider writing this function yourself for the special usecase using the `inner` vulkan handle of this descriptor set.
    pub fn write<'a>(&'a mut self, write: ash::vk::WriteDescriptorSet<'a>) {
        let write = write.dst_set(self.inner);

        unsafe {
            self.parent_pool
                .device()
                .inner
                .update_descriptor_sets(core::slice::from_ref(&write), &[])
        }
    }
}

impl Drop for DescriptorSet {
    fn drop(&mut self) {
        self.is_freed = true;
        #[allow(unused_variables)]
        if let Err(e) = self.parent_pool.free(&self.inner) {
            #[cfg(feature = "logging")]
            log::error!("Failed to free descriptor set: {}", e);
        }
    }
}
