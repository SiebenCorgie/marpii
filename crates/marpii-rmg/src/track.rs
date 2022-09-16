use std::sync::Arc;
use fxhash::FxHashMap;
use marpii::{ash::vk, context::Device, sync::Semaphore, resources::{CommandPool, CommandBufferAllocator, CommandBuffer}};

use crate::{resources::res_states::{Guard, AnyRes}, RecordError};


///Execution track. Basically a DeviceQueue and some associated data.
pub(crate) struct Track {
    pub(crate) queue_idx: u32,
    pub(crate) flags: vk::QueueFlags,
    pub(crate) sem: Arc<Semaphore>,

    pub(crate) command_buffer_pool: Arc<CommandPool>,
}

impl Track {

    pub fn new(device: &Arc<Device>, queue_idx: u32, flags: vk::QueueFlags) -> Self{
        Track {
            queue_idx,
            flags,
            sem: Semaphore::new(device, 0)
                .expect("Could not create Track's semaphore"),
            command_buffer_pool: Arc::new(CommandPool::new(
                device,
                queue_idx,
                vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER
            ).expect("Failed to create command pool!")),
        }
    }

    pub fn new_command_buffer(&mut self) -> Result<CommandBuffer<Arc<CommandPool>>, RecordError>{
        let cb = self.command_buffer_pool.clone().allocate_buffer(vk::CommandBufferLevel::PRIMARY)?;
        Ok(cb)
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct TrackId(pub vk::QueueFlags);
pub(crate) struct Tracks(pub FxHashMap<TrackId, Track>);

impl Tracks {

    ///Queue capability precedence, from less important to most important.
    //TODO: make nicer, see: https://github.com/ash-rs/ash/pull/549#discussion_r780719676 and https://github.com/ash-rs/ash/pull/549
    const CAP_PRECEDENCE: &'static [vk::QueueFlags] = &[
        vk::QueueFlags::empty(),
        vk::QueueFlags::TRANSFER,
        vk::QueueFlags::COMPUTE,
        vk::QueueFlags::from_raw(vk::QueueFlags::TRANSFER.as_raw() | vk::QueueFlags::COMPUTE.as_raw()),
        vk::QueueFlags::GRAPHICS,
        vk::QueueFlags::from_raw(vk::QueueFlags::GRAPHICS.as_raw() | vk::QueueFlags::TRANSFER.as_raw()),
        vk::QueueFlags::from_raw(vk::QueueFlags::GRAPHICS.as_raw() | vk::QueueFlags::COMPUTE.as_raw()),
        vk::QueueFlags::from_raw(vk::QueueFlags::GRAPHICS.as_raw() | vk::QueueFlags::COMPUTE.as_raw() | vk::QueueFlags::TRANSFER.as_raw())
    ];

    const CAP_MASK: vk::QueueFlags = vk::QueueFlags::from_raw(
        vk::QueueFlags::PROTECTED.as_raw() |
        vk::QueueFlags::RESERVED_7_QCOM.as_raw() |
        vk::QueueFlags::SPARSE_BINDING.as_raw() |
        vk::QueueFlags::VIDEO_DECODE_KHR.as_raw() |
        vk::QueueFlags::VIDEO_ENCODE_KHR.as_raw()
    );

    ///Returns true whenever the guard value was reached. Returns false if not, or the track doesn't exist.
    pub fn guard_finished(&self, guard: &Guard) -> bool {
        if let Some(t) = self.0.get(&guard.track) {
            t.sem.get_value() >= guard.target_value
        } else {
            false
        }
    }

    ///Returns a track that fits `usage` best. This decision is tricky since for instance TRANSFER usage
    /// can usually be done on most queues. But if it is a TRANSFER only usage without GRAPHICS or COMPUTE a pure
    /// transfer queue would fit best to get maximum occupancy.
    pub fn track_for_usage(&self, usage: vk::QueueFlags) -> Option<TrackId>{
        //To get the best track try to find a track that has only *this* usage. If none is found, add more capabilities
        // from less important to more important.

        for add_on_cap in Self::CAP_PRECEDENCE.iter(){
            let target_usage = usage | *add_on_cap;

            for (id, _) in self.0.iter(){
                let masked = id.0.as_raw() & !(id.0.as_raw() & Self::CAP_MASK.as_raw());
                if masked == target_usage.as_raw(){

                    #[cfg(feature="logging")]
                    log::trace!("Using {:#?} for {:#?}", id, usage);
                    return Some(*id);
                }
            }
        }



        #[cfg(feature="logging")]
        log::error!("
Could not find track for usage {:#?}. Following tracks are loaded:
{:#?}
",
        usage,
        self.0.keys());
        None
    }
}

