use fxhash::FxHashMap;
use marpii::{
    ash::vk,
    context::Device,
    resources::{CommandBuffer, CommandBufferAllocator, CommandPool},
    sync::Semaphore,
};
use std::{fmt::Display, sync::Arc};

use crate::{recorder::executor::Execution, RecordError};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Guard {
    track: TrackId,
    target_value: u64,
}

impl From<Guard> for TrackId {
    fn from(g: Guard) -> Self {
        g.track
    }
}

impl AsRef<TrackId> for Guard {
    fn as_ref(&self) -> &TrackId {
        &self.track
    }
}

impl Guard {
    pub fn wait_value(&self) -> u64 {
        self.target_value
    }

    pub fn expired(&self, tracks: &Tracks) -> bool {
        tracks.guard_finished(self)
    }

    ///Returns guard, that guards the execution before this one
    #[allow(dead_code)]
    pub fn guard_before(&self) -> Guard {
        Guard {
            track: self.track,
            target_value: self.target_value.checked_sub(1).unwrap_or(0),
        }
    }
}

///Execution track. Basically a DeviceQueue and some associated data.
#[allow(dead_code)]
pub(crate) struct Track {
    pub(crate) queue_idx: u32,
    pub(crate) flags: vk::QueueFlags,
    pub(crate) sem: Arc<Semaphore>,

    pub(crate) command_buffer_pool: Arc<CommandPool>,
    pub(crate) inflight_executions: Vec<Execution>,

    //Latest known value that is going to be signaled eventually.
    pub(crate) latest_signaled_value: u64,
}

impl Track {
    pub fn new(device: &Arc<Device>, queue_idx: u32, flags: vk::QueueFlags) -> Self {
        let sem = Semaphore::new(device, 0).expect("Could not create Track's semaphore");
        //sem.set_value(42).unwrap();
        //assert!(sem.get_value() == 42);
        Track {
            queue_idx,
            flags,
            sem,
            command_buffer_pool: Arc::new(
                CommandPool::new(
                    device,
                    queue_idx,
                    vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
                )
                .expect("Failed to create command pool!"),
            ),
            inflight_executions: Vec::with_capacity(10),
            latest_signaled_value: 0,
        }
    }

    ///Ticks the track. Triggers internal cleanup operations
    pub fn tick_frame(&mut self) {
        let finished_till = self.sem.get_value();
        //Drop all executions (and therefore resources like buffers etc) that have finished till now
        self.inflight_executions
            .retain(|exec| exec.guard.target_value >= finished_till);

        println!("{:?} {} inflight", self.flags, self.inflight_executions.len());
    }

    ///Allocates the next guard for this track.
    pub fn next_guard(&mut self) -> Guard {
        self.latest_signaled_value += 1;
        let g = Guard {
            track: TrackId(self.flags),
            target_value: self.latest_signaled_value,
        };
        g
    }

    pub(crate) fn wait_for_inflights(&mut self) {
        //we need to wait for all executions to finish
        let max = self
            .inflight_executions
            .iter()
            .fold(0, |max, inflight| max.max(inflight.guard.target_value));

        #[cfg(feature = "logging")]
        log::trace!("waiting track, waiting for {} on sem={:?}", max, self.sem);
        self.sem
            .wait(max, u64::MAX)
            .expect("Failed to wait for inflight execution");
        self.inflight_executions.clear();
    }

    pub fn new_command_buffer(&mut self) -> Result<CommandBuffer<Arc<CommandPool>>, RecordError> {
        let cb = self
            .command_buffer_pool
            .clone()
            .allocate_buffer(vk::CommandBufferLevel::PRIMARY)?;

        Ok(cb)
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct TrackId(pub vk::QueueFlags);
impl Display for TrackId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TrackId({:?})", self.0)
    }
}

pub(crate) struct Tracks(pub FxHashMap<TrackId, Track>);

impl Tracks {
    ///Queue capability precedence, from less important to most important.
    //TODO: make nicer, see: https://github.com/ash-rs/ash/pull/549#discussion_r780719676 and https://github.com/ash-rs/ash/pull/549
    const CAP_PRECEDENCE: &'static [vk::QueueFlags] = &[
        vk::QueueFlags::empty(),
        vk::QueueFlags::TRANSFER,
        vk::QueueFlags::COMPUTE,
        vk::QueueFlags::from_raw(
            vk::QueueFlags::TRANSFER.as_raw() | vk::QueueFlags::COMPUTE.as_raw(),
        ),
        vk::QueueFlags::GRAPHICS,
        vk::QueueFlags::from_raw(
            vk::QueueFlags::GRAPHICS.as_raw() | vk::QueueFlags::TRANSFER.as_raw(),
        ),
        vk::QueueFlags::from_raw(
            vk::QueueFlags::GRAPHICS.as_raw() | vk::QueueFlags::COMPUTE.as_raw(),
        ),
        vk::QueueFlags::from_raw(
            vk::QueueFlags::GRAPHICS.as_raw()
                | vk::QueueFlags::COMPUTE.as_raw()
                | vk::QueueFlags::TRANSFER.as_raw(),
        ),
    ];

    const CAP_MASK: vk::QueueFlags = vk::QueueFlags::from_raw(
        vk::QueueFlags::PROTECTED.as_raw()
            | vk::QueueFlags::RESERVED_7_QCOM.as_raw()
            | vk::QueueFlags::SPARSE_BINDING.as_raw()
            | vk::QueueFlags::VIDEO_DECODE_KHR.as_raw()
            | vk::QueueFlags::VIDEO_ENCODE_KHR.as_raw(),
    );

    ///Returns true whenever the guard value was reached or the track doesn't exist (anymore). Returns false if not.
    pub fn guard_finished(&self, guard: &Guard) -> bool {
        if let Some(t) = self.0.get(&guard.track) {
            t.sem.get_value() >= guard.target_value
        } else {
            true
        }
    }

    ///Returns a track that fits `usage` best. This decision is tricky since for instance TRANSFER usage
    /// can usually be done on most queues. But if it is a TRANSFER only usage without GRAPHICS or COMPUTE a pure
    /// transfer queue would fit best to get maximum occupancy.
    pub fn track_for_usage(&self, usage: vk::QueueFlags) -> Option<TrackId> {
        //To get the best track try to find a track that has only *this* usage. If none is found, add more capabilities
        // from less important to more important.

        for add_on_cap in Self::CAP_PRECEDENCE.iter() {
            let target_usage = usage | *add_on_cap;

            for (id, _) in self.0.iter() {
                let masked = id.0.as_raw() & !(id.0.as_raw() & Self::CAP_MASK.as_raw());
                if masked == target_usage.as_raw() {
                    #[cfg(feature = "logging")]
                    log::trace!("Using {:#?} for {:#?}", id, usage);
                    return Some(*id);
                }
            }
        }

        #[cfg(feature = "logging")]
        log::error!(
            "
Could not find track for usage {:#?}. Following tracks are loaded:
{:#?}
",
            usage,
            self.0.keys()
        );
        None
    }
}
