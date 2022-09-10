use fxhash::FxHashMap;
use graph::Recorder;
use marpii::{context::{Ctx, Queue}, allocator::Allocator, ash::vk::{self, QueueFlags}, sync::Semaphore};
use marpii_descriptor::bindless::BindlessDescriptor;
use thiserror::Error;
use std::sync::Arc;


pub mod graph;
pub mod resources;
pub mod task;


#[derive(Debug, Error)]
pub enum RmgError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error)
}

pub type TrackId = QueueFlags;

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
    res: resources::Resources,

    ///maps a capability pattern to a index in `Device`'s queue list. Each queue type defines a QueueTrack type.
    tracks: FxHashMap<TrackId, Track>
}

impl Rmg{
    pub fn new<A: Allocator + Send + Sync + 'static>(context: &Ctx<A>, window: &winit::window::Window) -> Result<Self, RmgError>{
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


        Err(RmgError::Any(anyhow::anyhow!("unimplemented")))
    }

    pub fn res_ref(&self) -> &resources::Resources{
        &self.res
    }

    pub fn res_mut(&mut self) -> &mut resources::Resources{
        &mut self.res
    }

    ///Records a task graph. Use [present](Recorder::present) tfo present the result on screen, or [execute](Recorder::execute) to execute without
    /// presenting anything.
    ///
    /// Note that the whole Rmg is borrowed while recording. The internal state can therefore not be changed while recording.
    pub fn new_graph<'a>(&'a mut self) -> Recorder<'a>{
        Recorder::new(self)
    }

    pub fn queue_idx_to_trackid(&self, idx: usize) -> Option<TrackId>{
        for t in self.tracks.iter(){
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
