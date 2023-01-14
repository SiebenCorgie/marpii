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
}

pub type CtxRmg = Ctx<Allocator>;

///Main RMG interface.
pub struct Rmg {
    ///Resource management
    pub(crate) res: Resources,

    ///maps a capability pattern to a index in `Device`'s queue list. Each queue type defines a QueueTrack type.
    pub(crate) tracks: Tracks,

    pub ctx: CtxRmg,
}

impl Rmg {
    pub fn new(context: Ctx<Allocator>) -> Result<Self, RmgError> {
        //Per definition we try to find at least one graphic, compute and transfer queue.
        // We then create the swapchain. It is used for image presentation and the start/end point for frame scheduling.

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
            res,
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

        let image = Arc::new(
            Image::new(
                &self.ctx.device,
                &self.ctx.allocator,
                description,
                MemoryUsage::GpuOnly, //always cpu only, everything else is handled by passes directly
                name,
                None,
            )
            .map_err(|e| MarpiiError::from(e))?,
        );

        Ok(self.res.add_image(image)?)
    }

    ///Creates a buffer that holds `n`-times data of type `T`. Where `n = buffer.size / size_of::<T>()`.
    pub fn new_buffer_uninitialized<T: 'static>(
        &mut self,
        description: BufDesc,
        name: Option<&str>,
    ) -> Result<BufferHandle<T>, RmgError> {
        let buffer = Arc::new(
            Buffer::new(
                &self.ctx.device,
                &self.ctx.allocator,
                description,
                MemoryUsage::GpuOnly,
                name,
                None,
            )
            .map_err(|e| MarpiiError::from(e))?,
        );

        Ok(self.res.add_buffer(buffer)?)
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
        self.res
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
        self.res
            .import_image(&self.tracks, image, queue_family, layout, access_flags)
    }

    pub fn new_sampler(
        &mut self,
        description: &vk::SamplerCreateInfoBuilder<'_>,
    ) -> Result<SamplerHandle, RmgError> {
        let sampler =
            Sampler::new(&self.ctx.device, description).map_err(|e| MarpiiError::from(e))?;

        Ok(self.res.add_sampler(Arc::new(sampler))?)
    }

    pub fn record<'rmg>(&'rmg mut self) -> Recorder<'rmg> {
        //tick all tracks to free resources
        for (_k, t) in self.tracks.0.iter_mut() {
            t.tick_frame();
        }
        //tick resource manager as well
        self.res.tick_record(&self.tracks);

        Recorder::new(self)
    }

    pub fn resources(&self) -> &Resources {
        &self.res
    }

    pub fn resources_mut(&mut self) -> &mut Resources {
        &mut self.res
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
