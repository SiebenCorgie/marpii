use std::{sync::Arc, collections::VecDeque};
use marpii::{context::Device, resources::{Buffer, Image, Sampler, ImageView, DescriptorSetLayout, DescriptorSet, DescriptorPool, PipelineLayout}, ash::vk};

///By definition when interpreted as big endian the highest byte is the handle type and the lower bytes are the actual index.
///
/// Note that the descriptor set index is the same as the type
#[derive(Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Debug)]
pub struct ResourceHandle(pub u32);

impl ResourceHandle{
    pub const TYPE_STORAGE_BUFFER: u8 = 0x0;
    pub const TYPE_STORAGE_IMAGE: u8 = 0x1;
    pub const TYPE_SAMPLED_IMAGE: u8 = 0x2;
    pub const TYPE_SAMPLER: u8 = 0x3;
    pub const TYPE_ACCELERATION_STRUCTURE: u8 = 0x4;

    ///Returns the handle type bits of this handle.
    pub fn handle_type(&self) -> u8{
        self.0.to_be_bytes()[core::mem::size_of::<u64>() - 1]
    }

    pub fn descriptor_ty(&self) -> vk::DescriptorType {
        match self.handle_type() {
            Self::TYPE_SAMPLED_IMAGE => vk::DescriptorType::SAMPLED_IMAGE,
            Self::TYPE_STORAGE_IMAGE => vk::DescriptorType::STORAGE_IMAGE,
            Self::TYPE_STORAGE_BUFFER => vk::DescriptorType::STORAGE_BUFFER,
            Self::TYPE_SAMPLER => vk::DescriptorType::SAMPLER,
            Self::TYPE_ACCELERATION_STRUCTURE => vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            _ => {
                //NOTE: This can't happen, but for compleatness we add it
                #[cfg(feature = "logging")]
                log::error!("Found unknown Resource handle, returning SampledImage");

                vk::DescriptorType::SAMPLED_IMAGE
            }
        }
    }

    ///Returns the index of this handle into its own descriptor.
    pub fn index(&self) -> u32{
        let mut bytes = self.0.to_be_bytes();
        bytes[core::mem::size_of::<u32>() - 1] = 0;
        u32::from_be_bytes(bytes)
    }

    pub fn new_from_desc_ty(ty: vk::DescriptorType, index: u32) -> Self{
        let ty = match ty{
            vk::DescriptorType::SAMPLED_IMAGE => Self::TYPE_SAMPLED_IMAGE,
            vk::DescriptorType::STORAGE_IMAGE => Self::TYPE_STORAGE_IMAGE,
            vk::DescriptorType::STORAGE_BUFFER => Self::TYPE_STORAGE_BUFFER,
            vk::DescriptorType::SAMPLER => Self::TYPE_SAMPLER,
            vk::DescriptorType::ACCELERATION_STRUCTURE_KHR => Self::TYPE_ACCELERATION_STRUCTURE,
            _ => {
                #[cfg(feature = "logging")]
                log::error!("Descriptor type {:?} unsupported, using STORAGE_BUFFER instead", ty);

                Self::TYPE_STORAGE_BUFFER
            }
        };

        Self::new(ty, index)
    }

    ///Creates a new handle, panics if the type is outside the defined types, or the index exceeds (2^56)-1.
    pub fn new(ty: u8, index: u32) -> Self{
        assert!(ty < Self::TYPE_ACCELERATION_STRUCTURE);
        assert!(index < 2u32.pow(24));

        let mut bytes = index.to_be_bytes();
        *bytes.last_mut().unwrap() = ty;

        ResourceHandle(u32::from_be_bytes(bytes))
    }
}


struct SetManager<T>{
    ///Collects free'd indices that can be used
    free: VecDeque<ResourceHandle>,
    stored: fxhash::FxHashMap<ResourceHandle, T>,
    //maximum index that can be bound
    max_idx: u32,
    //biggest index that was allocated until now,
    head_idx: u32,

    ty: vk::DescriptorType,
    layout: DescriptorSetLayout,
    descriptor_set: Arc<DescriptorSet<Arc<DescriptorPool>>>,
}

impl<T> SetManager<T> {
    fn allocate_handle(&mut self) -> Option<ResourceHandle> {
        if let Some(hdl) = self.free.pop_back() {
            Some(hdl)
        } else {
            if self.head_idx >= self.max_idx {
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
                Some(ResourceHandle::new_from_desc_ty(self.ty, new_idx))
            }
        }
    }

    #[allow(dead_code)]
    fn free_handle(&mut self, hdl: ResourceHandle) {
        assert!(hdl.descriptor_ty() == self.ty);
        self.free.push_front(hdl);
    }

    fn new(
        device: &Arc<Device>,
        pool: &Arc<DescriptorPool>,
        ty: vk::DescriptorType,
        max_count: u32,
        binding_id: u32,
    ) -> Result<Self, anyhow::Error> {
        let binding_layout = vk::DescriptorSetLayoutBinding {
            binding: binding_id,
            descriptor_type: ty,
            descriptor_count: max_count,
            stage_flags: vk::ShaderStageFlags::ALL,
            p_immutable_samplers: core::ptr::null(),
        };

        println!("Allocating @ {} {:?} size={}", binding_id, ty, max_count);

        let flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND
            | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT
            | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND; 1];

        let mut ext_flags =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::builder().binding_flags(&flags);

        println!("    {:#?}", binding_layout);
        let layout = unsafe {
            device.inner.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::builder()
                    .bindings(core::slice::from_ref(&binding_layout))
                    .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                    .push_next(&mut ext_flags),
                None,
            )?
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
                .allocate_descriptor_sets(&descriptor_set_info)?
        };
        assert!(
            descriptor_set.len() == 1,
            "Should have allocated 1 descriptor set, allocated {}",
            descriptor_set.len()
        );
        let descriptor_set = DescriptorSet {
            parent_pool: pool.clone(),
            is_freed: false,
            inner: descriptor_set.remove(0),
        };

        Ok(SetManager {
            ty,
            stored: fxhash::FxHashMap::default(),
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

///Bindless setup
///
/// Has 5 main descriptor types
///
/// - StorageBuffer
/// - StorageImage
/// - SampledImage (without combined sampler)
/// - Sampler
/// - AccellerationStructure
pub(crate) struct Bindless{
    stbuffer: SetManager<Arc<Buffer>>,
    stimage: SetManager<Arc<ImageView>>,
    saimage: SetManager<Arc<ImageView>>,
    sampler: SetManager<Arc<Sampler>>,
    accel: SetManager<Arc<Buffer>>,

    ///Safes the actual max push constant size, to verify bound push constants.
    #[allow(dead_code)]
    push_constant_size: u32,

    device: Arc<Device>,
}

impl Bindless {
    ///Default maximum number of bound images.
    pub const MAX_BOUND_SAMPLED_IMAGES: u32 = 128;
    ///Default maximum number of bound storage images.
    pub const MAX_BOUND_STORAGE_IMAGES: u32 = 128;
    ///Default maximum number of bound storage buffers.
    pub const MAX_BOUND_STORAGE_BUFFER: u32 = 128;
    ///Default maximum number of bound samplers.
    pub const MAX_BOUND_SAMPLER: u32 = 128;
    ///Default maximum number of bound acceleration structures.
    pub const MAX_BOUND_ACCELERATION_STRUCTURE: u32 = 128;

    ///Default maximum size of a push constant
    pub const MAX_PUSH_CONSTANT_SIZE: u32 = (core::mem::size_of::<u32>() * 16) as u32;

    ///max slot id.
    #[allow(dead_code)]
    const MAX_SLOT: u32 = 2u32.pow(24);

    ///Number of descriptor sets to cover each type
    const NUM_SETS: u32 = 5;

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
        max_sampler: u32,
        max_acceleration_structure: u32,
        push_constant_size: u32,
    ) -> Result<Self, anyhow::Error> {
        //TODO - check that all flags are set
        //     - setup layout
        //     return

        //FIXME: whenever the fix lands that allows us to query loaded extensions at runtime, remove the error and make the check below work.
        //       needed in VkPhysicalDeviceDescriptorIndexingFeatures: shaderInputAttachmentArrayDynamicIndexing, shaderInputAttachmentArrayNonUniformIndexing, descriptorBindingUniformBufferUpdateAfterBind
        #[cfg(feature = "logging")]
        log::error!("Cannot determin load state of needed extensions for bindless support!");

        if let Some(_f) = device.get_extension::<vk::PhysicalDeviceDescriptorIndexingFeatures>() {
            //TODO check that all needed flags are set
        } else {
            //anyhow::bail!("DescriptorIndexingFeature not loaded!")
        }

        if device
            .get_device_properties()
            .properties
            .limits
            .max_bound_descriptor_sets
            < Self::NUM_SETS
        {
            anyhow::bail!("Device does not support {} bound descriptor sets at a time", Self::NUM_SETS);
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
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                descriptor_count: max_acceleration_structure,
            },
        ];

        let desc_pool = Arc::new(DescriptorPool::new(
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

        let saimage = SetManager::new(
            device,
            &desc_pool,
            vk::DescriptorType::SAMPLED_IMAGE,
            max_sampled_image,
            ResourceHandle::TYPE_SAMPLED_IMAGE as u32,
        )?;
        let stimage = SetManager::new(
            device,
            &desc_pool,
            vk::DescriptorType::STORAGE_IMAGE,
            max_storage_image,
            ResourceHandle::TYPE_STORAGE_IMAGE as u32,
        )?;
        let stbuffer = SetManager::new(
            device,
            &desc_pool,
            vk::DescriptorType::STORAGE_BUFFER,
            max_storage_buffer,
            ResourceHandle::TYPE_STORAGE_BUFFER as u32,
        )?;
        let sampler = SetManager::new(
            device,
            &desc_pool,
            vk::DescriptorType::SAMPLER,
            max_sampler,
            ResourceHandle::TYPE_SAMPLER as u32,
        )?;
        let accel = SetManager::new(
            device,
            &desc_pool,
            vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            max_acceleration_structure,
            ResourceHandle::TYPE_ACCELERATION_STRUCTURE as u32,
        )?;

        Ok(Bindless {
            stbuffer,
            stimage,
            saimage,
            sampler,
            accel,

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
            self.stbuffer.layout.inner,
            self.stimage.layout.inner,
            self.saimage.layout.inner,
            self.sampler.layout.inner,
            self.accel.layout.inner,
        ];

        let push_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::ALL,
            offset: 0,
            size: push_constant_size,
        };

        let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::builder()
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
    pub fn new_default(device: &Arc<Device>) -> Result<Self, anyhow::Error> {
        let limits = device.get_device_properties().properties.limits;

        Self::new(
            device,
            Self::MAX_BOUND_SAMPLED_IMAGES.min(limits.max_descriptor_set_sampled_images),
            Self::MAX_BOUND_STORAGE_IMAGES.min(limits.max_descriptor_set_storage_images),
            Self::MAX_BOUND_STORAGE_BUFFER.min(limits.max_descriptor_set_storage_buffers),
            Self::MAX_BOUND_SAMPLER.min(limits.max_descriptor_set_samplers),
            Self::MAX_BOUND_ACCELERATION_STRUCTURE
                .min(limits.max_descriptor_set_storage_buffers_dynamic), //FIXME: not really the correct one...
            Self::MAX_PUSH_CONSTANT_SIZE,
        )
    }

    pub fn bind_storage_buffer(
        &mut self,
        buffer: Arc<Buffer>,
    ) -> Result<ResourceHandle, Arc<Buffer>> {
        //prepare our write instruction, then submit
        let buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(buffer.inner)
            .offset(0)
            .range(vk::WHOLE_SIZE);
        let write_instruction = vk::WriteDescriptorSet::builder()
            .buffer_info(core::slice::from_ref(&buffer_info))
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER);

        let hdl = self
            .stbuffer
            .bind(buffer, write_instruction)?;
        Ok(hdl) //wrap handle into correct type and exit
    }

    pub fn bind_storage_image(
        &mut self,
        image: Arc<ImageView>,
    ) -> Result<ResourceHandle, Arc<ImageView>> {
        //prepare our write instruction, then submit
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::GENERAL) //FIXME: works but is suboptimal. Might tag images
            .image_view(image.view);
        let write_instruction = vk::WriteDescriptorSet::builder()
            .image_info(core::slice::from_ref(&image_info))
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE);

        let hdl = self
            .stimage
            .bind(image, write_instruction)?;
        Ok(hdl) //wrap handle into correct type and exit
    }

    ///Tries to bind the image. On success returns the handle, on error the data is not bound and returned back to the caller.
    pub fn bind_sampled_image(
        &mut self,
        image: Arc<ImageView>,
    ) -> Result<ResourceHandle, Arc<ImageView>> {
        //prepare our write instruction, then submit
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::GENERAL) //FIXME: works but is suboptimal. Might tag images
            .image_view(image.view);
        let write_instruction = vk::WriteDescriptorSet::builder()
            .image_info(core::slice::from_ref(&image_info))
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE);

        let hdl = self
            .saimage
            .bind(image, write_instruction)?;
        Ok(hdl) //wrap handle into correct type and exit
    }

    pub fn bind_sampler(
        &mut self,
        sampler: Arc<Sampler>,
    ) -> Result<ResourceHandle, Arc<Sampler>> {
        //prepare our write instruction, then submit
        let image_info = vk::DescriptorImageInfo::builder()
            .sampler(sampler.inner);
        let write_instruction = vk::WriteDescriptorSet::builder()
            .image_info(core::slice::from_ref(&image_info))
            .descriptor_type(vk::DescriptorType::SAMPLER);

        let hdl = self
            .sampler
            .bind(sampler, write_instruction)?;
        Ok(hdl) //wrap handle into correct type and exit
    }

    pub fn clone_descriptor_sets(
        &self,
    ) -> [Arc<DescriptorSet<Arc<DescriptorPool>>>; Self::NUM_SETS as usize] {
        [
            self.stbuffer.descriptor_set.clone(),
            self.stimage.descriptor_set.clone(),
            self.saimage.descriptor_set.clone(),
            self.sampler.descriptor_set.clone(),
            self.accel.descriptor_set.clone(),
        ]
    }
}
