use fxhash::FxHashMap;
use graph::{Recorder, RecordError};
use marpii::{context::{Ctx, Queue}, allocator::MemoryUsage, ash::vk::{self, QueueFlags}, sync::Semaphore, surface::Surface, resources::{Image, ImgDesc, Buffer, BufDesc, Sampler, SharingMode}, gpu_allocator::vulkan::Allocator, swapchain::{Swapchain, self}};
use marpii_descriptor::bindless::BindlessDescriptor;
use resources::{Resources, ImageHdl, SamplerHdl, BufferHdl, Guard};
use thiserror::Error;
use std::{sync::Arc, convert::TryInto};


pub mod graph;
pub mod resources;
pub mod task;


#[derive(Debug, Error)]
pub enum RmgError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Recording error")]
    RecordingError(#[from] RecordError),

}

pub type TrackId = QueueFlags;
pub(crate) struct Tracks(pub FxHashMap<TrackId, Track>);

impl Tracks{
    ///Returns true whenever the guard value was reached. Returns false if not, or the track doesn't exist.
    pub fn guard_finished(&self, guard: &Guard) -> bool{
        if let Some(t) = self.0.get(&guard.track){
            t.sem.get_value() >= guard.target_val
        }else {
            false
        }
    }
}

///Execution track. Basically a DeviceQueue and some associated data.
pub(crate) struct Track{
    pub(crate) queue_idx: usize,
    pub(crate) flags: QueueFlags,
    pub(crate) sem: Arc<Semaphore>,
    ///last known target of the semaphore's counter
    pub(crate) sem_target: u64,
}

///Main runtime environment that handles resources and frame/pass scheduling.
pub struct Rmg{
    ///bindless management
    bindless: BindlessDescriptor,
    ///Resource management
    pub(crate) res: resources::Resources,

    ///maps a capability pattern to a index in `Device`'s queue list. Each queue type defines a QueueTrack type.
    tracks: Tracks,

    pub ctx: Ctx<Allocator>,

    swapchain: Swapchain,
}

impl Rmg{
    pub fn new(context: Ctx<Allocator>, swapchain_surface: Surface) -> Result<Self, RmgError>{
        //Per definition we try to find at least one graphic, compute and transfer queue.
        // We then create the swapchain. It is used for image presentation and the start/end point for frame scheduling.

        let tracks = context.device.queues.iter().enumerate().fold(FxHashMap::default(), |mut set: FxHashMap<TrackId, Track>, (idx, q)| {

            //Make sure to only add queue, if we don't have a queue with those capabilities yet.
            if !set.contains_key(&q.properties.queue_flags){
                set.insert(
                    q.properties.queue_flags,
                    Track {
                        queue_idx: idx,
                        flags: q.properties.queue_flags,
                        sem: Semaphore::new(&context.device, 0).expect("Could not create Track's semaphore"),
                        sem_target: 0
                    }
                );
            }

            set
        });

        let bindless = BindlessDescriptor::new_default(&context.device)?;
        let res = Resources::new();
        let swapchain = Swapchain::builder(&context.device, &Arc::new(swapchain_surface))?
            .with(move |b| b.create_info.usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .build()?;

        Ok(Rmg{
            bindless,
            res,
            tracks: Tracks(tracks),
            swapchain,
            ctx: context
        })
    }

    pub fn new_image_uninitialized(&mut self, description: ImgDesc, sampler: Option<SamplerHdl>, name: Option<&str>) -> Result<ImageHdl, RmgError>{
        let image = Image::new(
            &self.ctx.device,
            &self.ctx.allocator,
            description,
            MemoryUsage::GpuOnly, //always cpu only, everything else is handled by passes directly
            name,
            None
        )?;

        Ok(self.res.new_image(Arc::new(image), sampler))
    }

    pub fn new_buffer_uninitialized<T: 'static>(&mut self, description: BufDesc, name: Option<&str>) -> Result<BufferHdl<T>, RmgError>{
        let buffer = Buffer::new(
            &self.ctx.device,
            &self.ctx.allocator,
            description,
            MemoryUsage::GpuOnly,
            name,
            None
        )?;

        Ok(self.res.new_buffer(Arc::new(buffer)))
    }

    ///Creates a new (storage)buffer that can hold at max `size` times `T`.
    pub fn new_buffer<T: 'static>(&mut self, size: usize, name: Option<&str>) -> Result<BufferHdl<T>, RmgError>{
        let size = core::mem::size_of::<T>() * size;
        let description = BufDesc { size: size.try_into().unwrap(), usage: vk::BufferUsageFlags::STORAGE_BUFFER, sharing: SharingMode::Exclusive };
        self.new_buffer_uninitialized(description, name)
    }

    pub fn new_sampler(&mut self, description: &vk::SamplerCreateInfoBuilder) -> Result<SamplerHdl, RmgError>{
        let sampler = Sampler::new(&self.ctx.device, description)?;

        Ok(self.res.new_sampler(Arc::new(sampler)))
    }

    ///Records a task graph. Use [present](Recorder::present) tfo present the result on screen, or [execute](Recorder::execute) to execute without
    /// presenting anything.
    ///
    /// Note that the whole Rmg is borrowed while recording. The internal state can therefore not be changed while recording.
    pub fn new_graph<'a>(&'a mut self) -> Recorder<'a>{

        #[cfg(feature="logging")]
        log::info!("New Frame");

        self.res.notify_new_frame();
        Recorder::new(
            self,
            self.swapchain.surface
                          .get_current_extent(&self.ctx.device.physical_device)
                          .unwrap_or(vk::Extent2D{width: 1, height: 1})
        )
    }

    pub fn queue_idx_to_trackid(&self, idx: usize) -> Option<TrackId>{
        for t in self.tracks.0.iter(){
            if t.1.queue_idx == idx{
                return Some(*t.0);
            }
        }

        None
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}



//TODO: Highlevel:
//      - Actual scheduling
//      - Handle pipeline creation/destrucion based on the bindless-provided pipeline layou
//      - swapchain/present handling
//      - a lot of helper functions for image/buffer/sampler creation
