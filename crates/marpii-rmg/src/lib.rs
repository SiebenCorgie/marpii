#![feature(drain_filter)]

//! # ResourceManagingGraph (RMG)
//!
//! The RMG is a big abstraction layer over raw vulkan. It is therefore much more opinionated then the rest of MarpII.
//!
//! It handles the context creation as well as resource creation and binding. The user (you) primarily interacts in the form of [Task](recorder::Task)s. They can be scheduled
//! in an execution Graph using a [Recorder](recorder::Recorder). The tasks implementation is up to you and has full access to all resources and the Vulkan context.
//!
//! TODO: more docs on how to get started etc.

mod resources;
use fxhash::FxHashMap;
use recorder::Recorder;
pub use resources::{
    res_states::{AnyResKey, BufferKey, ImageKey, ResBuffer, ResImage, ResSampler, SamplerKey},
    ResourceError, Resources,
};

mod recorder;
pub use recorder::{
    task::{ResourceRegistry, Task, AttachmentDescription, AttachmentType},
    RecordError,
};

pub(crate) mod track;

///Pre implemented generic tasks
pub mod tasks;

use marpii::{
    allocator::MemoryUsage,
    ash::vk,
    context::Ctx,
    gpu_allocator::vulkan::Allocator,
    resources::{BufDesc, Buffer, Image, ImgDesc, Sampler, SharingMode},
    surface::Surface,
};
use std::sync::Arc;
use thiserror::Error;
use track::{Track, TrackId, Tracks};

///Top level Error structure.
#[derive(Debug, Error)]
pub enum RmgError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Recording error")]
    RecordingError(#[from] RecordError),

    #[error("Resource error")]
    ResourceError(#[from] ResourceError),
}

pub type CtxRmg = Ctx<Allocator>;

///Main RMG interface.
pub struct Rmg {
    ///Resource management
    pub(crate) res: resources::Resources,

    ///maps a capability pattern to a index in `Device`'s queue list. Each queue type defines a QueueTrack type.
    tracks: Tracks,

    pub ctx: CtxRmg,
}

impl Rmg {
    pub fn new(context: Ctx<Allocator>, surface: &Arc<Surface>) -> Result<Self, RmgError> {
        //Per definition we try to find at least one graphic, compute and transfer queue.
        // We then create the swapchain. It is used for image presentation and the start/end point for frame scheduling.

        //TODO: make the iterator return an error. Currently if track creation fails, everything fails
        let tracks = context.device.queues.iter().enumerate().fold(
            FxHashMap::default(),
            |mut set: FxHashMap<TrackId, Track>, (idx, q)| {
                #[cfg(feature = "logging")]
                log::info!("QueueType: {:#?}", q.properties.queue_flags);
                //Make sure to only add queue, if we don't have a queue with those capabilities yet.
                if !set.contains_key(&TrackId(q.properties.queue_flags)) {
                    set.insert(
                        TrackId(q.properties.queue_flags),
                        Track::new(&context.device, idx as u32, q.properties.queue_flags),
                    );
                }

                set
            },
        );

        let res = Resources::new(&context.device, surface)?;

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
    ) -> Result<ImageKey, RmgError> {
        //patch usage bits

        if !description.usage.contains(vk::ImageUsageFlags::SAMPLED) && !description.usage.contains(vk::ImageUsageFlags::STORAGE){
            return Err(RmgError::from(ResourceError::ImageNoUsageFlags));
        }

        let image = Arc::new(Image::new(
            &self.ctx.device,
            &self.ctx.allocator,
            description,
            MemoryUsage::GpuOnly, //always cpu only, everything else is handled by passes directly
            name,
            None,
        )?);

        Ok(self.res.add_image(image)?)
    }

    pub fn new_buffer_uninitialized(
        &mut self,
        description: BufDesc,
        name: Option<&str>,
    ) -> Result<BufferKey, RmgError> {
        let buffer = Arc::new(Buffer::new(
            &self.ctx.device,
            &self.ctx.allocator,
            description,
            MemoryUsage::GpuOnly,
            name,
            None,
        )?);

        Ok(self.res.add_buffer(buffer)?)
    }

    ///Creates a new (storage)buffer that can hold at max `size` times `T`.
    pub fn new_buffer<T: 'static>(
        &mut self,
        size: usize,
        name: Option<&str>,
    ) -> Result<BufferKey, RmgError> {
        let size = core::mem::size_of::<T>() * size;
        let description = BufDesc {
            size: size.try_into().unwrap(),
            usage: vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST,
            sharing: SharingMode::Exclusive,
        };
        self.new_buffer_uninitialized(description, name)
    }

    pub fn new_sampler(
        &mut self,
        description: &vk::SamplerCreateInfoBuilder,
    ) -> Result<SamplerKey, RmgError> {
        let sampler = Sampler::new(&self.ctx.device, description)?;

        Ok(self.res.add_sampler(Arc::new(sampler))?)
    }

    pub fn record<'rmg>(&'rmg mut self, window_extent: vk::Extent2D) -> Recorder<'rmg> {
        //tick all tracks to free resources
        for (_k, t) in self.tracks.0.iter_mut() {
            t.tick_frame();
        }
        //tick resource manager as well
        self.res.tick_record(&self.tracks);

        Recorder::new(self, window_extent)
    }

    pub fn delete(&mut self, res: impl Into<AnyResKey>) -> Result<(), ResourceError>{
        self.res.remove_resource(res)
    }

    pub fn resources(&self) -> &Resources{
        &self.res
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
