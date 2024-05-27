use ahash::AHashMap;
use marpii::{
    ash::vk,
    context::Device,
    resources::{
        Buffer, DescriptorPool, DescriptorSet, DescriptorSetLayout, ImageView, PipelineLayout,
        Sampler,
    },
    DescriptorError, DeviceError, MarpiiError, OoS,
};
use std::{collections::VecDeque, sync::Arc};

///Low-level handle type. The two least segnificant bits describe the handles type, all higher bits describe the handles
/// position in its descriptor set. Therefore, after checking the type the index can be calculated by shifting the handle down two bits.
#[derive(Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Debug)]
pub struct ResourceHandle(pub u32);

impl ResourceHandle {
    pub const SAMPLED_IMAGE_TYPE: u32 = 0x0;
    pub const STORAGE_IMAGE_TYPE: u32 = 0x1;
    pub const STORAGE_BUFFER_TYPE: u32 = 0x2;
    pub const ACCELERATION_STRUCTURE_TYPE: u32 = 0x3;

    ///Reserved *Undefined* handle.a
    pub const UNDEFINED_HANDLE: u32 = 0xff_ff_ff_ff;

    const TY_MASK: u32 = 0b0000_0000_0000_0000_0000_0000_0000_0011;

    pub fn new_handle(ty: vk::DescriptorType, index: u32) -> Self {
        assert!(
            index <= 2u32.pow(29),
            "ResourceHandle index was too big, {} > {}",
            index,
            2u32.pow(29)
        );
        let ty = match ty {
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER => Self::SAMPLED_IMAGE_TYPE,
            vk::DescriptorType::STORAGE_IMAGE => Self::STORAGE_IMAGE_TYPE,
            vk::DescriptorType::STORAGE_BUFFER => Self::STORAGE_BUFFER_TYPE,
            vk::DescriptorType::ACCELERATION_STRUCTURE_KHR => Self::ACCELERATION_STRUCTURE_TYPE,
            _ => panic!("Unknown handle type: {:?}", ty),
        };
        assert!(
            ty < 4,
            "ResourceHandleType was too big, was {}, max is 3",
            ty
        );

        ResourceHandle(index << 2 | ty)
    }

    pub fn ty(&self) -> vk::DescriptorType {
        match self.0 & Self::TY_MASK {
            Self::SAMPLED_IMAGE_TYPE => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            Self::STORAGE_IMAGE_TYPE => vk::DescriptorType::STORAGE_IMAGE,
            Self::STORAGE_BUFFER_TYPE => vk::DescriptorType::STORAGE_BUFFER,
            Self::ACCELERATION_STRUCTURE_TYPE => vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            _ => {
                //NOTE: This can't happen, but for compleatness we add it
                #[cfg(feature = "logging")]
                log::error!("Found unknown Resource handle, returning SampledImage");

                vk::DescriptorType::SAMPLED_IMAGE
            }
        }
    }

    pub fn index(&self) -> u32 {
        self.0 >> 2
    }
}

#[derive(Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Debug)]
pub struct SampledImageHandle(pub ResourceHandle);
#[derive(Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Debug)]
pub struct StorageImageHandle(pub ResourceHandle);
#[derive(Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Debug)]
pub struct StorageBufferHandle(pub ResourceHandle);
#[derive(Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Debug)]
pub struct AccelerationStructureHandle(pub ResourceHandle);

///Manages the descriptors for a single set
struct SetManagment<T> {
    ///Collects free'd indices that can be used
    free: VecDeque<ResourceHandle>,
    stored: AHashMap<ResourceHandle, T>,
    //maximum index that can be bound
    max_idx: u32,
    //biggest index that was allocated until now,
    head_idx: u32,

    ty: vk::DescriptorType,
    layout: DescriptorSetLayout,
    descriptor_set: Arc<DescriptorSet>,
}

impl<T> SetManagment<T> {
    fn allocate_handle(&mut self) -> Option<ResourceHandle> {
        if let Some(hdl) = self.free.pop_back() {
            Some(hdl)
        } else if self.head_idx >= self.max_idx {
            #[cfg(feature = "logging")]
            log::error!(
                "Reached max index for bindless set of type: {:?} = {}",
                self.ty,
                self.max_idx
            );
            None
        } else {
            let new_idx = self.head_idx;
            self.head_idx += 1;
            Some(ResourceHandle::new_handle(self.ty, new_idx))
        }
    }

    #[allow(dead_code)]
    fn free_handle(&mut self, hdl: ResourceHandle) {
        assert!(hdl.ty() == self.ty);
        self.free.push_front(hdl);
    }

    fn new(
        device: &Arc<Device>,
        pool: OoS<DescriptorPool>,
        ty: vk::DescriptorType,
        max_count: u32,
        binding_id: u32,
    ) -> Result<Self, MarpiiError> {
        let binding_layout = vk::DescriptorSetLayoutBinding {
            binding: binding_id,
            descriptor_type: ty,
            descriptor_count: max_count,
            stage_flags: vk::ShaderStageFlags::ALL,
            p_immutable_samplers: core::ptr::null(),
        };

        #[cfg(feature = "logging")]
        log::info!("Allocating @ {} {:?} size={}", binding_id, ty, max_count);

        let flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND
            | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT
            | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND; 1];

        let mut ext_flags =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&flags);

        #[cfg(feature = "logging")]
        log::info!("    {:#?}", binding_layout);
        let layout = unsafe {
            device
                .inner
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(core::slice::from_ref(&binding_layout))
                        .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                        .push_next(&mut ext_flags),
                    None,
                )
                .map_err(|e| MarpiiError::from(DescriptorError::from(e)))?
        };

        //wrap into the marpii wrapper
        let layout = DescriptorSetLayout {
            device: device.clone(),
            inner: layout,
        };

        //NOTE: we can not use the descriptor-set allocate trait, since we need to specify some additional info.
        //      we use it however, to track lifetime etc.
        let mut allocate_count_info =
            vk::DescriptorSetVariableDescriptorCountAllocateInfo::default()
                .descriptor_counts(core::slice::from_ref(&max_count));

        let descriptor_set_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool.inner)
            .push_next(&mut allocate_count_info)
            .set_layouts(core::slice::from_ref(&layout.inner));

        let mut descriptor_set = unsafe {
            layout
                .device
                .inner
                .allocate_descriptor_sets(&descriptor_set_info)
                .map_err(|e| DescriptorError::VkError(e))?
        };
        assert!(
            descriptor_set.len() == 1,
            "Should have allocated 1 descriptor set, allocated {}",
            descriptor_set.len()
        );
        let descriptor_set = DescriptorSet {
            parent_pool: pool,
            is_freed: false,
            inner: descriptor_set.remove(0),
        };

        Ok(SetManagment {
            ty,
            stored: AHashMap::default(),
            free: VecDeque::with_capacity(10), //NOTE: seems sane. But IDK, maybe its overkill
            max_idx: max_count,
            head_idx: 0,
            layout,
            descriptor_set: Arc::new(descriptor_set),
        })
    }

    ///Binds `dta` and returns the resource handle on success. If not succeeded (usually when all descriptors are in use), the data is returned.
    fn bind(
        &mut self,
        dta: T,
        mut write_instruction: vk::WriteDescriptorSetBuilder,
    ) -> Result<ResourceHandle, T> {
        let hdl = if let Some(hdl) = self.allocate_handle() {
            hdl
        } else {
            return Err(dta);
        };

        assert!(
            !self.stored.contains_key(&hdl),
            "Allocated handle was in use!"
        );
        //allocated handle, and is not in use, we can bind!
        write_instruction = write_instruction
            .dst_set(self.descriptor_set.inner)
            .dst_binding(0)
            .dst_array_element(hdl.0);

        assert!(write_instruction.descriptor_count == 1);

        //Manual write
        //FIXME: Make thread safe. Currently this could be unsafe...
        unsafe {
            self.layout
                .device
                .inner
                .update_descriptor_sets(core::slice::from_ref(&write_instruction), &[]);
        }

        self.stored.insert(hdl, dta);

        Ok(hdl)
    }

    #[allow(dead_code)]
    fn free_binding(&mut self, hdl: ResourceHandle) -> Option<T> {
        if let Some(res) = self.stored.remove(&hdl) {
            self.free_handle(hdl); //free handle since we are safely removing
                                   //TODO do we need to unsubscribe in the descriptor set?
            Some(res)
        } else {
            #[cfg(feature = "logging")]
            log::warn!("Tried to free handle {} in bindless descriptor set of type {:?}, but there was nothing stored at the given id.", hdl.0, self.ty);

            None
        }
    }
}

/// Bindless descriptor helper. Manages a single big descriptor sset that binds all resources.
/// in the shader there is one binding per descriptor type (Sampled Image, Storage Image, Buffer), each binding is a array of multiple
/// images/buffers that can be indexed.
///
/// # Queue ownership
/// Since the bindless descriptorset does not take care of queue ownership you have to make sure that:
///
/// 1. The descriptor set is used only on the same queue family
/// 2. bound resoureces are owned by this queue family
/// 3. bound resources are in the correct access-mask / image-layout for the intended access
///
/// # In shader usage
/// 👷TODO👷
///
/// # Safety
/// Note that the helper assumes that the resources are used on the correct queue, and with he correct layouts. It does not track queue ownership or layout tranisitons for you.
pub struct BindlessDescriptor {
    sampled_image_set: SetManagment<(Arc<ImageView>, Arc<Sampler>)>,
    storage_image_set: SetManagment<Arc<ImageView>>,
    storage_buffer_set: SetManagment<Arc<Buffer>>,
    accel_structure_set: SetManagment<Arc<Buffer>>,

    ///Safes the actual max push constant size, to verify bound push constants.
    #[allow(dead_code)]
    push_constant_size: u32,

    device: Arc<Device>,
}

impl BindlessDescriptor {
    ///Default maximum number of bound images.
    pub const MAX_BOUND_SAMPLED_IMAGES: u32 = 128;
    ///Default maximum number of bound storage images.
    pub const MAX_BOUND_STORAGE_IMAGES: u32 = 128;
    ///Default maximum number of bound storage buffers.
    pub const MAX_BOUND_STORAGE_BUFFER: u32 = 128;
    ///Default maximum number of bound acceleration structures.
    pub const MAX_BOUND_ACCELERATION_STRUCTURE: u32 = 128;

    ///Default maximum size of a push constant
    pub const MAX_PUSH_CONSTANT_SIZE: u32 = (core::mem::size_of::<u32>() * 16) as u32;

    ///max slot id.
    #[allow(dead_code)]
    const MAX_SLOT: u32 = 2u32.pow(29);

    ///Number of descriptor sets to cover each type
    const NUM_SETS: u32 = 4;

    ///Creates a new instance of a bindless descriptor set. The limits of max bound descriptors per descriptor type can be set. If you don't care, consider using the shorter
    /// [new_default](BindlessDescriptor::new_default) function.
    ///
    /// `push_constant_size` describes how big the biggest push constant used with this set can be.
    ///
    /// # Safety
    /// Assumes that the supplied `max_*` values are within the device limits. Otherwise the function might fail (or panic) while creating the descriptor pool.
    pub fn new(
        device: &Arc<Device>,
        max_sampled_image: u32,
        max_storage_image: u32,
        max_storage_buffer: u32,
        max_acceleration_structure: u32,
        push_constant_size: u32,
    ) -> Result<Self, MarpiiError> {
        //TODO - check that all flags are set
        //     - setup layout
        //     return

        let features = device.get_physical_device_features();
        if features.shader_storage_image_array_dynamic_indexing == 0
            || features.shader_storage_image_array_dynamic_indexing == 0
            || features.shader_storage_buffer_array_dynamic_indexing == 0
            || features.shader_uniform_buffer_array_dynamic_indexing == 0
            || features.shader_sampled_image_array_dynamic_indexing == 0
        {
            #[cfg(feature = "logging")]
            log::error!("Some dynamic indexing features where not supported. Following was supported: {:#?}", features);
            return Err(DeviceError::UnsupportedFeature(String::from(
                "DynamicArrayDescriptorIndexing",
            )))?;
        }
        //check device for all needed features
        let features2 = device.get_feature::<vk::PhysicalDeviceVulkan12Features>();
        if features2.descriptor_indexing == 0
            || features2.descriptor_binding_sampled_image_update_after_bind == 0
            || features2.descriptor_binding_storage_image_update_after_bind == 0
            || features2.descriptor_binding_storage_buffer_update_after_bind == 0
            || features2.descriptor_binding_partially_bound == 0
            || features2.descriptor_binding_variable_descriptor_count == 0
            || features2.shader_storage_buffer_array_non_uniform_indexing == 0
            || features2.shader_storage_image_array_non_uniform_indexing == 0
            || features2.shader_sampled_image_array_non_uniform_indexing == 0
        {
            #[cfg(feature = "logging")]
            log::error!(
                "Some bindless features where not supported. Following was supported: {:#?}",
                features2
            );

            return Err(DeviceError::UnsupportedFeature(String::from(
                "DescriptorUpdateAfterBind, PartiallyBound, VariableCount or NonUniformIndexing",
            )))?;
        }

        if device
            .get_device_properties()
            .properties
            .limits
            .max_bound_descriptor_sets
            < Self::NUM_SETS
        {
            Err(DeviceError::UnsupportedFeature(String::from(format!(
                "Max bound descriptor setst < {}",
                Self::NUM_SETS
            ))))?;
        }

        let descriptor_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: max_sampled_image,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: max_storage_image,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: max_storage_buffer,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                descriptor_count: max_acceleration_structure,
            },
        ];

        let mut desc_pool = OoS::new(DescriptorPool::new(
            device,
            vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND
                | vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET,
            &descriptor_sizes,
            Self::NUM_SETS,
        )?);

        #[allow(unused_variables)]
        let push_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::ALL,
            offset: 0,
            size: push_constant_size,
        };

        let sampled_image_set = SetManagment::new(
            device,
            desc_pool.share(),
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            max_sampled_image,
            0,
        )?;
        let storage_image_set = SetManagment::new(
            device,
            desc_pool.share(),
            vk::DescriptorType::STORAGE_IMAGE,
            max_storage_image,
            1,
        )?;
        let storage_buffer_set = SetManagment::new(
            device,
            desc_pool.share(),
            vk::DescriptorType::STORAGE_BUFFER,
            max_storage_buffer,
            2,
        )?;
        let accel_structure_set = SetManagment::new(
            device,
            desc_pool.share(),
            vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            max_acceleration_structure,
            3,
        )?;

        Ok(BindlessDescriptor {
            sampled_image_set,
            storage_image_set,
            storage_buffer_set,
            accel_structure_set,
            push_constant_size,
            device: device.clone(),
        })
    }

    ///Creates a new instance of the pipeline layout used for bindless descriptors.
    pub fn new_pipeline_layout(&self, push_constant_size: u32) -> PipelineLayout {
        //NOTE: This is the delicate part. We create a link between the descriptor set layouts and this pipeline layout. This is however *safe*
        //      since we keep the sets in memory together with the pipeline layout. On drop the pipeline layout is destried before the descriptorset layouts
        //      which is again *safe*
        let descset_layouts = &[
            self.sampled_image_set.layout.inner,
            self.storage_image_set.layout.inner,
            self.storage_buffer_set.layout.inner,
            self.accel_structure_set.layout.inner,
        ];

        let push_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::ALL,
            offset: 0,
            size: push_constant_size,
        };

        let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(descset_layouts)
            .push_constant_ranges(core::slice::from_ref(&push_range));

        let pipeline_layout = unsafe {
            self.device
                .inner
                .create_pipeline_layout(&pipeline_layout_create_info, None)
                .unwrap()
        };
        PipelineLayout {
            device: self.device.clone(),
            layout: pipeline_layout,
        }
    }

    ///Creates a `BindlessDescriptor` where the maximum numbers of bound descriptors is a sane minimum of the `MAX_*` constants, and the reported upper limits of the device.
    pub fn new_default(device: &Arc<Device>) -> Result<Self, MarpiiError> {
        let limits = device.get_device_properties().properties.limits;

        Self::new(
            device,
            Self::MAX_BOUND_SAMPLED_IMAGES.min(limits.max_descriptor_set_sampled_images),
            Self::MAX_BOUND_STORAGE_IMAGES.min(limits.max_descriptor_set_storage_images),
            Self::MAX_BOUND_STORAGE_BUFFER.min(limits.max_descriptor_set_storage_buffers),
            Self::MAX_BOUND_ACCELERATION_STRUCTURE
                .min(limits.max_descriptor_set_storage_buffers_dynamic), //FIXME: not really the correct one...
            Self::MAX_PUSH_CONSTANT_SIZE,
        )
    }

    ///Tries to bind the image and its sampler. On success returns the handle, on error the data is not bound and returned back to the caller.
    pub fn bind_sampled_image(
        &mut self,
        image: Arc<ImageView>,
        sampler: Arc<Sampler>,
    ) -> Result<SampledImageHandle, (Arc<ImageView>, Arc<Sampler>)> {
        //prepare our write instruction, then submit
        let image_info = vk::DescriptorImageInfo::default()
            .sampler(sampler.inner)
            .image_layout(vk::ImageLayout::GENERAL) //FIXME: works but is suboptimal. Might tag images
            .image_view(image.view);
        let write_instruction = vk::WriteDescriptorSet::default()
            .image_info(core::slice::from_ref(&image_info))
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER);

        let hdl = self
            .sampled_image_set
            .bind((image, sampler), write_instruction)?;
        Ok(SampledImageHandle(hdl)) //wrap handle into correct type and exit
    }

    pub fn clone_descriptor_sets(&self) -> [Arc<DescriptorSet>; Self::NUM_SETS as usize] {
        [
            self.sampled_image_set.descriptor_set.clone(),
            self.storage_image_set.descriptor_set.clone(),
            self.storage_buffer_set.descriptor_set.clone(),
            self.accel_structure_set.descriptor_set.clone(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_handle_access() {
        let sa_img = ResourceHandle::new_handle(vk::DescriptorType::COMBINED_IMAGE_SAMPLER, 42);
        let st_img = ResourceHandle::new_handle(vk::DescriptorType::STORAGE_IMAGE, 43);
        let st_buf = ResourceHandle::new_handle(vk::DescriptorType::STORAGE_BUFFER, 44);
        let acc = ResourceHandle::new_handle(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR, 45);

        assert!(sa_img.index() == 42 && sa_img.ty() == vk::DescriptorType::COMBINED_IMAGE_SAMPLER);
        assert!(st_img.index() == 43 && st_img.ty() == vk::DescriptorType::STORAGE_IMAGE);
        assert!(st_buf.index() == 44 && st_buf.ty() == vk::DescriptorType::STORAGE_BUFFER);
        assert!(acc.index() == 45 && acc.ty() == vk::DescriptorType::ACCELERATION_STRUCTURE_KHR);
    }
}
