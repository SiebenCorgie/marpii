//! The bindless descriptor implementation of RMG.
//!
//! It creates one descriptor-set for each "type" of descriptor. Each set
//! has a big (usually the maximum) number of descriptors allocated. At runtime
//! they are updated to contain resources whenever they are bound.
//!
//! This resource management happens only once before rendering a frame.
//!
//! Loosely based on:
//! - <https://blog.traverseresearch.nl/bindless-rendering-setup-afeb678d77fc>
//! - <https://vincent-p.github.io/posts/vulkan_bindless_descriptors/>
//!
//! Does not (yet) use byte addressable buffers.

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
use std::{collections::VecDeque, fmt::Debug, sync::Arc};

//Re-export of the handle type.
pub use marpii_rmg_shared::ResourceHandle;

///Handles one descriptor set of type T.
struct SetManager<T> {
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

impl<T> Debug for SetManager<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "SetManager[{:#?}]:", self.ty)?;
        for k in self.stored.keys() {
            writeln!(f, "    {}@{}", k.index(), k.handle_type())?
        }

        Ok(())
    }
}

impl<T> SetManager<T> {
    fn allocate_handle(&mut self) -> Option<ResourceHandle> {
        if let Some(mut hdl) = self.free.pop_back() {
            //Possibly resettig shared usage
            hdl = ResourceHandle::new(ResourceHandle::descriptor_type_to_u8(self.ty), hdl.index());
            #[cfg(feature = "logging")]
            log::info!("Reusing handle {:?} for descty {:#?}", hdl, self.ty);
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
            #[cfg(feature = "logging")]
            log::info!(
                "Allocating new handle {:?} for descty {:#?}",
                new_idx,
                self.ty
            );
            self.head_idx += 1;
            Some(ResourceHandle::new_from_desc_ty(self.ty, new_idx))
        }
    }

    fn allocate_common<O>(&mut self, other: &mut SetManager<O>) -> Option<ResourceHandle> {
        //Similar to allocate_handle, but we do that in parallel for both sets
        // to find a common descriptor index.

        //try to resuse one in both, otherwise jump to the back
        let mut free_index = None;
        for (fidx, f) in self.free.iter().enumerate() {
            for (ofidx, of) in other.free.iter().enumerate() {
                if of.index() == f.index() {
                    free_index = Some(((fidx, ofidx), of.index()));
                    break;
                }
            }
        }

        if let Some(reuse) = free_index {
            #[cfg(feature = "logging")]
            log::trace!(
                "Found common free index: {} @ {} / {}",
                reuse.1,
                reuse.0 .0,
                reuse.0 .1
            );
            //remove both from free list
            let this_hdl = self.free.remove(reuse.0 .0).unwrap();
            let other_hdl = other.free.remove(reuse.0 .1).unwrap();

            assert!(this_hdl.index() == other_hdl.index());

            //build new common handle
            let hdl = ResourceHandle::new(
                this_hdl.handle_type() | other_hdl.handle_type(),
                this_hdl.index(),
            );
            Some(hdl)
        } else {
            #[cfg(feature = "logging")]
            log::info!(
                "Did not found common index for {:#?} and {:#?}",
                self.ty,
                other.ty
            );
            let max_idx = self.head_idx.max(other.head_idx);
            //Mark whole region till that index free for both sets
            #[cfg(feature = "logging")]
            {
                log::info!("Marking {:#?} {}..{} free", self.ty, self.head_idx, max_idx);
                log::info!(
                    "Marking {:#?} {}..{} free",
                    other.ty,
                    other.head_idx,
                    max_idx
                );
            }
            for idx in self.head_idx..max_idx {
                self.free
                    .push_back(ResourceHandle::new_from_desc_ty(self.ty, idx));
            }
            for idx in other.head_idx..max_idx {
                other
                    .free
                    .push_back(ResourceHandle::new_from_desc_ty(other.ty, idx));
            }
            //now increase both since we want to use that handle
            self.head_idx = max_idx + 1;
            other.head_idx = max_idx + 1;

            //finally build new, combinde resource handle
            Some(ResourceHandle::new(
                ResourceHandle::descriptor_type_to_u8(self.ty)
                    | ResourceHandle::descriptor_type_to_u8(other.ty),
                max_idx as u32,
            ))
        }
    }

    fn free_handle(&mut self, hdl: ResourceHandle) {
        assert!(
            hdl.contains_type(ResourceHandle::descriptor_type_to_u8(self.ty)),
            "Handle type {:b} & {:b}",
            ResourceHandle::descriptor_type_to_u8(self.ty),
            hdl.handle_type()
        );
        self.free.push_front(hdl);
    }

    fn new(
        device: &Arc<Device>,
        pool: OoS<DescriptorPool>,
        ty: vk::DescriptorType,
        max_count: u32,
        //binding_id: u32,
    ) -> Result<Self, MarpiiError> {
        let binding_layout = vk::DescriptorSetLayoutBinding {
            binding: 0,
            descriptor_type: ty,
            descriptor_count: max_count,
            stage_flags: vk::ShaderStageFlags::ALL,
            p_immutable_samplers: core::ptr::null(),
        };

        #[cfg(feature = "logging")]
        log::trace!("Allocating @ {:?} size={}", ty, max_count);

        let flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND
            | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT
            | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND; 1];

        let mut ext_flags =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::builder().binding_flags(&flags);

        #[cfg(feature = "logging")]
        log::trace!("    {:#?}", binding_layout);

        let layout = unsafe {
            device
                .inner
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::builder()
                        .bindings(core::slice::from_ref(&binding_layout))
                        .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                        .push_next(&mut ext_flags),
                    None,
                )
                .map_err(|e| DescriptorError::VkError(e))?
        };

        //wrap into the marpii wrapper
        let layout = DescriptorSetLayout {
            device: device.clone(),
            inner: layout,
        };

        //NOTE: we can not use the descriptor-set allocate trait, since we need to specify some additional info.
        //      we use it however, to track lifetime etc.
        let mut allocate_count_info =
            vk::DescriptorSetVariableDescriptorCountAllocateInfo::builder()
                .descriptor_counts(core::slice::from_ref(&max_count));

        let descriptor_set_info = vk::DescriptorSetAllocateInfo::builder()
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

        Ok(SetManager {
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
        mut write_instruction: vk::WriteDescriptorSetBuilder<'_>,
        allocated_slot: Option<ResourceHandle>,
    ) -> Result<ResourceHandle, T> {
        let hdl = if let Some(h) = allocated_slot {
            h
        } else {
            if let Some(hdl) = self.allocate_handle() {
                hdl
            } else {
                return Err(dta);
            }
        };

        assert!(
            !self.stored.contains_key(&hdl),
            "{:?}: Allocated handle {} was in use, \nnew_handle: {:?},\nstore: {:?}!",
            self.ty,
            hdl.index(),
            hdl,
            self.stored.keys()
        );

        #[cfg(feature = "logging")]
        log::trace!("Binding {:?} to {:?}", self.ty, hdl.index());

        //allocated handle, and is not in use, we can bind!
        write_instruction = write_instruction
            .dst_set(self.descriptor_set.inner)
            .dst_binding(0)
            .dst_array_element(hdl.index());

        assert!(write_instruction.descriptor_count == 1);

        assert!(
            write_instruction.p_buffer_info != core::ptr::null()
                || write_instruction.p_image_info != core::ptr::null()
                || write_instruction.p_texel_buffer_view != core::ptr::null()
        );

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

    fn unbind_handle(&mut self, hdl: ResourceHandle) -> Option<T> {
        if let Some(res) = self.stored.remove(&hdl) {
            self.free_handle(hdl); //free handle since we are safely removing
                                   //TODO do we need to unsubscribe in the descriptor set?
            Some(res)
        } else {
            #[cfg(feature = "logging")]
            log::warn!("Tried to free handle {:?} in bindless descriptor set of type {:?}, but there was nothing stored at the given id.", hdl, self.ty);

            None
        }
    }
}

///Bindless setup
///
/// Has 5 main descriptor types
///
/// - `StorageBuffer`
/// - `StorageImage`
/// - `SampledImage` (without combined sampler)
/// - `Sampler`
/// - `AccellerationStructure`
///
//TODO: Check if VK_EXT_mutable_descriptor_type works even better. We could put everything into one desc pool
pub(crate) struct Bindless {
    stbuffer: SetManager<Arc<Buffer>>,
    stimage: SetManager<Arc<ImageView>>,
    saimage: SetManager<Arc<ImageView>>,
    sampler: SetManager<Arc<Sampler>>,
    #[cfg(feature = "ray-tracing")]
    accel: SetManager<Arc<Buffer>>,

    ///Safes the actual max push constant size, to verify bound push constants.
    push_constant_size: u32,

    device: Arc<Device>,
}

impl Debug for Bindless {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "
storage buffer:
{:#?}
storage image:
{:#?}
sampled image:
{:#?}
sampler:
{:#?}
",
            self.stbuffer, self.stimage, self.saimage, self.sampler
        )
    }
}

impl Bindless {
    ///Default maximum number of bound images.
    /// NOTE that this is the theoretical maximum of 2^24, since the ResHandle safes the
    /// descriptor type in the lowest byte.
    pub const MAX_BOUND_SAMPLED_IMAGES: u32 = 1 << 24;
    ///Default maximum number of bound storage images.
    /// NOTE that this is the theoretical maximum of 2^24, since the ResHandle safes the
    /// descriptor type in the lowest byte.
    pub const MAX_BOUND_STORAGE_IMAGES: u32 = 1 << 24;
    ///Default maximum number of bound storage buffers.
    /// NOTE that this is the theoretical maximum of 2^24, since the ResHandle safes the
    /// descriptor type in the lowest byte.
    pub const MAX_BOUND_STORAGE_BUFFER: u32 = 1 << 24;
    ///Default maximum number of bound samplers.
    /// NOTE that this is the theoretical maximum of 2^24, since the ResHandle safes the
    /// descriptor type in the lowest byte.
    pub const MAX_BOUND_SAMPLER: u32 = 1 << 24;
    ///Default maximum number of bound acceleration structures.
    /// NOTE that this is the theoretical maximum of 2^24, since the ResHandle safes the
    /// descriptor type in the lowest byte.
    #[cfg(feature = "ray-tracing")]
    pub const MAX_BOUND_ACCELERATION_STRUCTURE: u32 = 1 << 24;

    ///max slot id.
    #[allow(dead_code)]
    const MAX_SLOT: u32 = 2u32.pow(24);

    ///Number of descriptor sets to cover each type
    #[cfg(not(feature = "ray-tracing"))]
    const NUM_SETS: u32 = 4;

    #[cfg(feature = "ray-tracing")]
    const NUM_SETS: u32 = 5;

    ///Creates a new instance of a bindless descriptor set. The limits of max bound descriptors per descriptor type can be set. If you don't care, consider using the shorter
    /// [`new_default`](BindlessDescriptor::new_default) function.
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
        max_sampler: u32,
        #[cfg(feature = "ray-tracing")] max_acceleration_structure: u32,
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
                ty: vk::DescriptorType::SAMPLED_IMAGE,
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
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: max_sampler,
            },
            #[cfg(feature = "ray-tracing")]
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

        //always maximum size
        let push_constant_size = device
            .get_device_properties()
            .properties
            .limits
            .max_push_constants_size;
        #[cfg(feature = "logging")]
        log::info!(
            "Creating Bindless layout with max push_constant_size={}",
            push_constant_size
        );

        let saimage = SetManager::new(
            device,
            desc_pool.share(),
            vk::DescriptorType::SAMPLED_IMAGE,
            max_sampled_image,
        )?;
        let stimage = SetManager::new(
            device,
            desc_pool.share(),
            vk::DescriptorType::STORAGE_IMAGE,
            max_storage_image,
        )?;
        let stbuffer = SetManager::new(
            device,
            desc_pool.share(),
            vk::DescriptorType::STORAGE_BUFFER,
            max_storage_buffer,
        )?;
        let sampler = SetManager::new(
            device,
            desc_pool.share(),
            vk::DescriptorType::SAMPLER,
            max_sampler,
        )?;

        #[cfg(feature = "ray-tracing")]
        let accel = SetManager::new(
            device,
            desc_pool.share(),
            vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            max_acceleration_structure,
        )?;

        Ok(Bindless {
            stbuffer,
            stimage,
            saimage,
            sampler,
            #[cfg(feature = "ray-tracing")]
            accel,

            push_constant_size,
            device: device.clone(),
        })
    }

    ///Creates a new instance of the pipeline layout used for bindless descriptors. Note that bindless takes the sets 0..4, afterwards
    /// the supplied additional sets can be added.
    pub fn new_pipeline_layout(
        &self,
        additional_descriptor_sets: &[DescriptorSetLayout],
    ) -> PipelineLayout {
        //NOTE: This is the delicate part. We create a link between the descriptor set layouts and this pipeline layout. This is however *safe*
        //      since we keep the sets in memory together with the pipeline layout. On drop the pipeline layout is destried before the descriptorset layouts
        //      which is again *safe*
        let mut descset_layouts = vec![
            self.stbuffer.layout.inner,
            self.stimage.layout.inner,
            self.saimage.layout.inner,
            self.sampler.layout.inner,
            #[cfg(feature = "ray-tracing")]
            self.accel.layout.inner,
        ];

        for additional in additional_descriptor_sets {
            descset_layouts.push(additional.inner)
        }

        let push_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::ALL,
            offset: 0,
            size: self.push_constant_size,
        };

        let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&descset_layouts)
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
    #[cfg(feature = "ray-tracing")]
    pub fn new_default(device: &Arc<Device>) -> Result<Self, MarpiiError> {
        let limits = device.get_device_properties().properties.limits;
        Self::new(
            device,
            Self::MAX_BOUND_SAMPLED_IMAGES.min(limits.max_descriptor_set_sampled_images),
            Self::MAX_BOUND_STORAGE_IMAGES.min(limits.max_descriptor_set_storage_images),
            Self::MAX_BOUND_STORAGE_BUFFER.min(limits.max_descriptor_set_storage_buffers),
            Self::MAX_BOUND_SAMPLER.min(limits.max_descriptor_set_samplers),
            Self::MAX_BOUND_ACCELERATION_STRUCTURE
                .min(limits.max_descriptor_set_storage_buffers_dynamic), //FIXME: not really the correct one...
        )
    }

    ///Creates a `BindlessDescriptor` where the maximum numbers of bound descriptors is a sane minimum of the `MAX_*` constants, and the reported upper limits of the device.
    #[cfg(not(feature = "ray-tracing"))]
    pub fn new_default(device: &Arc<Device>) -> Result<Self, MarpiiError> {
        let limits = device.get_device_properties().properties.limits;
        Self::new(
            device,
            Self::MAX_BOUND_SAMPLED_IMAGES.min(limits.max_descriptor_set_sampled_images),
            Self::MAX_BOUND_STORAGE_IMAGES.min(limits.max_descriptor_set_storage_images),
            Self::MAX_BOUND_STORAGE_BUFFER.min(limits.max_descriptor_set_storage_buffers),
            Self::MAX_BOUND_SAMPLER.min(limits.max_descriptor_set_samplers),
        )
    }

    pub fn bind_storage_buffer(
        &mut self,
        buffer: Arc<Buffer>,
    ) -> Result<ResourceHandle, Arc<Buffer>> {
        #[cfg(feature = "logging")]
        log::trace!("Binding storage buffer!");

        //prepare our write instruction, then submit
        let buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(buffer.inner)
            .offset(0)
            .range(vk::WHOLE_SIZE);
        let write_instruction = vk::WriteDescriptorSet::builder()
            .buffer_info(core::slice::from_ref(&buffer_info))
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER);

        let hdl = self.stbuffer.bind(buffer, write_instruction, None)?;
        Ok(hdl) //wrap handle into correct type and exit
    }

    pub fn remove_storage_buffer(&mut self, handle: ResourceHandle) {
        assert!(self.stbuffer.unbind_handle(handle).is_some());
    }

    pub fn bind_storage_image(
        &mut self,
        image: Arc<ImageView>,
    ) -> Result<ResourceHandle, Arc<ImageView>> {
        if !image
            .src_img
            .desc
            .usage
            .contains(vk::ImageUsageFlags::STORAGE)
        {
            #[cfg(feature = "logging")]
            log::error!("Tried to bind as storage image, but has no storage usage!");
            return Err(image);
        }

        if image
            .src_img
            .desc
            .usage
            .contains(vk::ImageUsageFlags::SAMPLED)
        {
            #[cfg(feature = "logging")]
            log::warn!("Tried to bind as storage image, that has also the SAMPLED bit set");
        }

        #[cfg(feature = "logging")]
        log::trace!("Binding storage image!");

        //prepare our write instruction, then submit
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::GENERAL) //FIXME: works but is suboptimal. Might tag images
            .image_view(image.view);
        let write_instruction = vk::WriteDescriptorSet::builder()
            .image_info(core::slice::from_ref(&image_info))
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE);

        let hdl = self.stimage.bind(image, write_instruction, None)?;
        Ok(hdl) //wrap handle into correct type and exit
    }

    pub fn remove_storage_image(&mut self, handle: ResourceHandle) {
        assert!(self.stimage.unbind_handle(handle).is_some());
    }

    ///Tries to bind the image. On success returns the handle, on error the data is not bound and returned back to the caller.
    pub fn bind_sampled_image(
        &mut self,
        image: Arc<ImageView>,
    ) -> Result<ResourceHandle, Arc<ImageView>> {
        if !image
            .src_img
            .desc
            .usage
            .contains(vk::ImageUsageFlags::SAMPLED)
        {
            #[cfg(feature = "logging")]
            log::error!("Tried to bind as sampled image, but has no sample usage!");
            return Err(image);
        }

        if image
            .src_img
            .desc
            .usage
            .contains(vk::ImageUsageFlags::STORAGE)
        {
            #[cfg(feature = "logging")]
            log::warn!("Tried to bind as sampled image, that has also the STORAGE bit set");
        }

        #[cfg(feature = "logging")]
        log::trace!("Binding sampled image!");

        //prepare our write instruction, then submit
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::GENERAL) //FIXME: works but is suboptimal. Might tag images
            .image_view(image.view);
        let write_instruction = vk::WriteDescriptorSet::builder()
            .image_info(core::slice::from_ref(&image_info))
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE);

        let hdl = self.saimage.bind(image, write_instruction, None)?;
        Ok(hdl) //wrap handle into correct type and exit
    }

    pub fn bind_sampled_storage_image(
        &mut self,
        image: Arc<ImageView>,
    ) -> Result<ResourceHandle, Arc<ImageView>> {
        if !image
            .src_img
            .desc
            .usage
            .contains(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::STORAGE)
        {
            #[cfg(feature = "logging")]
            log::error!(
                "Tried to bind as sampled&storage image, but lacks one of those usages: {:#?}!",
                image.src_img.desc.usage
            );
            return Err(image);
        }

        #[cfg(feature = "logging")]
        log::trace!("Binding sampled+storage image!");

        //prepare our write instruction, then submit
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::GENERAL) //FIXME: works but is suboptimal. Might tag images
            .image_view(image.view);

        let allocated_hdl =
            if let Some(pre_fetched_hdl) = self.saimage.allocate_common(&mut self.stimage) {
                pre_fetched_hdl
            } else {
                #[cfg(feature = "logging")]
                log::error!("Failed to pre-allocate handle for common sampled + storage image!");
                return Err(image);
            };

        let write_instruction_sampled = vk::WriteDescriptorSet::builder()
            .image_info(core::slice::from_ref(&image_info))
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE);

        let write_instruction_storage = vk::WriteDescriptorSet::builder()
            .image_info(core::slice::from_ref(&image_info))
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE);

        let hdl_sa = self.saimage.bind(
            image.clone(),
            write_instruction_sampled,
            Some(allocated_hdl),
        )?;

        let hdl_st = self
            .stimage
            .bind(image, write_instruction_storage, Some(allocated_hdl))?;

        assert!(
            hdl_sa.index() == hdl_st.index(),
            "Failed to allocate commonly-indexed, dual bound storage + sampled image handle!"
        );

        Ok(allocated_hdl) //wrap handle into correct type and exit
    }

    pub fn remove_sampled_image(&mut self, handle: ResourceHandle) {
        assert!(self.saimage.unbind_handle(handle).is_some());
    }

    pub fn bind_sampler(&mut self, sampler: Arc<Sampler>) -> Result<ResourceHandle, Arc<Sampler>> {
        #[cfg(feature = "logging")]
        log::trace!("Binding sampler!");

        //prepare our write instruction, then submit
        let image_info = vk::DescriptorImageInfo::builder().sampler(sampler.inner);
        let write_instruction = vk::WriteDescriptorSet::builder()
            .image_info(core::slice::from_ref(&image_info))
            .descriptor_type(vk::DescriptorType::SAMPLER);

        let hdl = self.sampler.bind(sampler, write_instruction, None)?;
        Ok(hdl) //wrap handle into correct type and exit
    }

    pub fn remove_sampler(&mut self, handle: ResourceHandle) {
        assert!(self.sampler.unbind_handle(handle).is_some());
    }

    #[allow(dead_code)]
    pub fn clone_descriptor_sets(&self) -> [Arc<DescriptorSet>; Self::NUM_SETS as usize] {
        [
            self.stbuffer.descriptor_set.clone(),
            self.stimage.descriptor_set.clone(),
            self.saimage.descriptor_set.clone(),
            self.sampler.descriptor_set.clone(),
            #[cfg(feature = "ray-tracing")]
            self.accel.descriptor_set.clone(),
        ]
    }

    pub fn clone_raw_descriptor_sets(&self) -> [vk::DescriptorSet; Self::NUM_SETS as usize] {
        [
            self.stbuffer.descriptor_set.inner,
            self.stimage.descriptor_set.inner,
            self.saimage.descriptor_set.inner,
            self.sampler.descriptor_set.inner,
            #[cfg(feature = "ray-tracing")]
            self.accel.descriptor_set.inner,
        ]
    }
}
