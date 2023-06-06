use ahash::AHashMap;
use marpii::{
    allocator::MemoryUsage,
    ash::vk,
    context::Ctx,
    gpu_allocator::vulkan::Allocator,
    resources::{BufDesc, Buffer, Image, ImgDesc, Sampler, SharingMode},
    MarpiiError,
};
use std::sync::Arc;
use thiserror::Error;

use crate::{
    recorder::Recorder,
    track::{Track, TrackId, Tracks},
    BufferHandle, ImageHandle, RecordError, ResourceError, Resources, SamplerHandle,
};

#[cfg(feature = "debug_marker")]
use marpii::ash::vk::{Handle, ObjectType};

#[cfg(feature = "debug_marker")]
use std::any::type_name;

///Top level Error structure.
#[derive(Debug, Error)]
pub enum RmgError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("MarpII internal error: {0}")]
    MarpiiError(#[from] MarpiiError),

    #[error("Recording error")]
    RecordingError(#[from] RecordError),

    #[error("Resource error")]
    ResourceError(#[from] ResourceError),

    #[error("Missing Vulkan feature, make sure to activate the ones flagged: \n: {0:#?}")]
    MissingFeatures(Vec<String>),
}

pub type CtxRmg = Ctx<Allocator>;

macro_rules! check_feature {
    ($vkf:ident, $name:ident, $missing:ident, $any_needed:ident) => {
        if $vkf.$name == 0 {
            $any_needed = true;
            $missing.push(format!("{}::{}", stringify!($vkf), stringify!($name)));
        }
    };
}

///Main RMG interface.
pub struct Rmg {
    ///Resource management
    pub resources: Resources,

    ///maps a capability pattern to a index in `Device`'s queue list. Each queue type defines a QueueTrack type.
    pub(crate) tracks: Tracks,

    pub ctx: CtxRmg,
}

impl Rmg {
    fn check_features(context: &Ctx<Allocator>) -> Result<(), RmgError> {
        //Right now we are hardcoding all needed features.

        let mut missing = Vec::new();
        let mut any_needed = false;

        let vk10 = context.device.get_physical_device_features();
        let _vk11 = context
            .device
            .get_feature::<vk::PhysicalDeviceVulkan11Features>();
        let vk12 = context
            .device
            .get_feature::<vk::PhysicalDeviceVulkan12Features>();
        let vk13 = context
            .device
            .get_feature::<vk::PhysicalDeviceVulkan13Features>();

        check_feature!(vk10, shader_int16, missing, any_needed);
        check_feature!(vk10, shader_float64, missing, any_needed);
        check_feature!(
            vk10,
            shader_storage_buffer_array_dynamic_indexing,
            missing,
            any_needed
        );
        check_feature!(
            vk10,
            shader_storage_image_array_dynamic_indexing,
            missing,
            any_needed
        );
        check_feature!(
            vk10,
            shader_uniform_buffer_array_dynamic_indexing,
            missing,
            any_needed
        );
        check_feature!(
            vk10,
            shader_sampled_image_array_dynamic_indexing,
            missing,
            any_needed
        );
        check_feature!(vk10, robust_buffer_access, missing, any_needed);

        check_feature!(vk12, shader_int8, missing, any_needed);
        check_feature!(vk12, runtime_descriptor_array, missing, any_needed);
        check_feature!(vk12, timeline_semaphore, missing, any_needed);
        check_feature!(vk12, descriptor_indexing, missing, any_needed);
        check_feature!(
            vk12,
            descriptor_binding_partially_bound,
            missing,
            any_needed
        );
        check_feature!(
            vk12,
            descriptor_binding_sampled_image_update_after_bind,
            missing,
            any_needed
        );
        check_feature!(
            vk12,
            descriptor_binding_storage_image_update_after_bind,
            missing,
            any_needed
        );
        check_feature!(
            vk12,
            descriptor_binding_storage_buffer_update_after_bind,
            missing,
            any_needed
        );
        check_feature!(
            vk12,
            descriptor_binding_variable_descriptor_count,
            missing,
            any_needed
        );
        check_feature!(
            vk12,
            shader_storage_buffer_array_non_uniform_indexing,
            missing,
            any_needed
        );
        check_feature!(
            vk12,
            shader_storage_image_array_non_uniform_indexing,
            missing,
            any_needed
        );
        check_feature!(
            vk12,
            shader_sampled_image_array_non_uniform_indexing,
            missing,
            any_needed
        );
        check_feature!(vk12, vulkan_memory_model, missing, any_needed);

        check_feature!(vk13, maintenance4, missing, any_needed);
        check_feature!(vk13, dynamic_rendering, missing, any_needed);
        check_feature!(vk13, synchronization2, missing, any_needed);

        if any_needed {
            Err(RmgError::MissingFeatures(missing))
        } else {
            Ok(())
        }
    }

    ///Creates a new ResourceManagingGraph for this context. Note that the context must be created for
    /// Vulkan 1.3, since it depends on multiple core-1.3 features and extensions.
    ///
    /// When in doubt, use [Ctx::new_default_from_instance].
    pub fn new(context: Ctx<Allocator>) -> Result<Self, RmgError> {
        //Per definition we try to find at least one graphic, compute and transfer queue.
        // We then create the swapchain. It is used for image presentation and the start/end point for frame scheduling.

        //query context for features
        Self::check_features(&context)?;

        //TODO: make the iterator return an error. Currently if track creation fails, everything fails
        let tracks = context.device.queues.iter().fold(
            AHashMap::default(),
            |mut set: AHashMap<TrackId, Track>, q| {
                #[cfg(feature = "logging")]
                log::info!("QueueType: {:#?}", q.properties.queue_flags);
                //Make sure to only add queue, if we don't have a queue with those capabilities yet.
                if let std::collections::hash_map::Entry::Vacant(e) =
                    set.entry(TrackId(q.properties.queue_flags))
                {
                    e.insert(Track::new(
                        &context.device,
                        q.family_index,
                        q.properties.queue_flags,
                    ));
                }

                set
            },
        );

        let res = Resources::new(&context.device)?;

        Ok(Rmg {
            resources: res,
            tracks: Tracks(tracks),
            ctx: context,
        })
    }

    pub fn new_image_uninitialized(
        &mut self,
        description: ImgDesc,
        name: Option<&str>,
    ) -> Result<ImageHandle, RmgError> {
        //patch usage bits

        if !description.usage.contains(vk::ImageUsageFlags::SAMPLED)
            && !description.usage.contains(vk::ImageUsageFlags::STORAGE)
        {
            return Err(RmgError::from(ResourceError::ImageNoUsageFlags));
        }

        #[cfg(feature = "debug_marker")]
        let dbg_name = std::ffi::CString::new(name.unwrap_or(&format!(
            "Image: {:?} {:#?}",
            description.img_type, description.format
        )))
        .unwrap_or(std::ffi::CString::new("Unnamed Image").unwrap());

        let image = Arc::new(
            Image::new(
                &self.ctx.device,
                &self.ctx.allocator,
                description,
                MemoryUsage::GpuOnly, //always cpu only, everything else is handled by passes directly
                name,
            )
            .map_err(|e| MarpiiError::from(e))?,
        );

        #[cfg(feature = "debug_marker")]
        {
            if let Some(dbg) = self.ctx.device.instance.get_debugger() {
                if let Err(e) = dbg.name_object(
                    &self.ctx.device.inner.handle(),
                    image.inner.as_raw(),
                    ObjectType::IMAGE,
                    &dbg_name,
                ) {
                    #[cfg(feature = "logging")]
                    log::error!("Could not name image: {}", e);
                }
            }
        }

        Ok(self.resources.add_image(image)?)
    }

    ///Creates a buffer that holds `n`-times data of type `T`. Where `n = buffer.size / size_of::<T>()`.
    pub fn new_buffer_uninitialized<T: 'static>(
        &mut self,
        description: BufDesc,
        name: Option<&str>,
    ) -> Result<BufferHandle<T>, RmgError> {
        #[cfg(feature = "debug_marker")]
        let dbg_name = std::ffi::CString::new(name.unwrap_or(&format!("{}", type_name::<T>())))
            .unwrap_or(std::ffi::CString::new("Unnamed Buffer").unwrap());

        let buffer = Arc::new(
            Buffer::new(
                &self.ctx.device,
                &self.ctx.allocator,
                description,
                MemoryUsage::GpuOnly,
                name,
            )
            .map_err(|e| MarpiiError::from(e))?,
        );

        #[cfg(feature = "debug_marker")]
        {
            if let Some(dbg) = self.ctx.device.instance.get_debugger() {
                if let Err(e) = dbg.name_object(
                    &self.ctx.device.inner.handle(),
                    buffer.inner.as_raw(),
                    ObjectType::BUFFER,
                    &dbg_name,
                ) {
                    #[cfg(feature = "logging")]
                    log::error!("Could not name buffer: {}", e);
                }
            }
        }

        Ok(self.resources.add_buffer(buffer)?)
    }

    ///Creates a new (storage)buffer that can hold at max `size` times `T`.
    pub fn new_buffer<T: 'static>(
        &mut self,
        size: usize,
        name: Option<&str>,
    ) -> Result<BufferHandle<T>, RmgError> {
        let size = core::mem::size_of::<T>() * size;
        let description = BufDesc {
            size: size.try_into().unwrap(),
            usage: vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::TRANSFER_DST,
            sharing: SharingMode::Exclusive,
            ..Default::default()
        };
        self.new_buffer_uninitialized(description, name)
    }

    ///Imports the buffer with the given state. Returns an error if a given `queue_family` index has no internal `TrackId`.
    pub fn import_buffer<T: 'static>(
        &mut self,
        buffer: Arc<Buffer>,
        queue_family: Option<u32>,
        access_flags: Option<vk::AccessFlags2>,
    ) -> Result<BufferHandle<T>, ResourceError> {
        self.resources
            .import_buffer(&self.tracks, buffer, queue_family, access_flags)
    }

    ///Imports the image with the given state. Returns an error if a given `queue_family` index has no internal `TrackId`.
    pub fn import_image(
        &mut self,
        image: Arc<Image>,
        queue_family: Option<u32>,
        layout: Option<vk::ImageLayout>,
        access_flags: Option<vk::AccessFlags2>,
    ) -> Result<ImageHandle, ResourceError> {
        self.resources
            .import_image(&self.tracks, image, queue_family, layout, access_flags)
    }

    pub fn new_sampler(
        &mut self,
        description: &vk::SamplerCreateInfoBuilder<'_>,
    ) -> Result<SamplerHandle, RmgError> {
        let sampler =
            Sampler::new(&self.ctx.device, description).map_err(|e| MarpiiError::from(e))?;

        Ok(self.resources.add_sampler(Arc::new(sampler))?)
    }

    pub fn record<'rmg>(&'rmg mut self) -> Recorder<'rmg> {
        //tick all tracks to free resources
        for (_k, t) in self.tracks.0.iter_mut() {
            t.tick_frame();
        }
        //tick resource manager as well
        self.resources.tick_record(&self.tracks);

        Recorder::new(self)
    }

    pub(crate) fn queue_idx_to_trackid(&self, idx: u32) -> Option<TrackId> {
        for t in self.tracks.0.iter() {
            if t.1.queue_idx == idx {
                return Some(*t.0);
            }
        }
        None
    }

    pub(crate) fn trackid_to_queue_idx(&self, id: TrackId) -> u32 {
        self.tracks.0.get(&id).unwrap().queue_idx
    }

    ///waits till the gpu is idle
    pub fn wait_for_idle(&self) -> Result<(), RecordError> {
        unsafe { self.ctx.device.inner.device_wait_idle()? }

        Ok(())
    }
}

impl Drop for Rmg {
    fn drop(&mut self) {
        //make sure all executions have finished, otherwise we could destroy
        // referenced images etc.
        for (_id, t) in self.tracks.0.iter_mut() {
            t.wait_for_inflights()
        }
    }
}
