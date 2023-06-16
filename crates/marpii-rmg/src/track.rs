use ahash::AHashMap;
use marpii::{
    ash::vk,
    context::Device,
    resources::{CommandBuffer, CommandBufferAllocator, CommandPool},
    sync::Semaphore,
    OoS,
};

#[cfg(feature = "timestamps")]
use marpii::util::Timestamps;

use std::{fmt::Display, sync::Arc};

use crate::{recorder::Execution, RecordError, Rmg};

#[cfg(feature = "timestamps")]
use tinyvec::{Array, TinyVec};

#[derive(Debug, Clone, Copy)]
pub struct Guard {
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

    pub(crate) fn expired(&self, tracks: &Tracks) -> bool {
        tracks.guard_finished(self)
    }

    //Returns true if the gurad was passed on the gpu
    pub fn is_expired(&self, rmg: &Rmg) -> bool {
        if let Some(t) = rmg.tracks.0.get(&self.track) {
            t.latest_signaled_value > self.target_value
        } else {
            #[cfg(feature = "logging")]
            log::warn!("Queried guard for none existent track.");
            false
        }
    }

    ///Waits for the guard to expire. Fails if that is not possible
    pub fn wait(&self, rmg: &Rmg, timeout: u64) -> Result<(), vk::Result> {
        if let Some(t) = rmg.tracks.0.get(&self.track) {
            t.sem.wait(self.target_value, timeout)
        } else {
            Err(vk::Result::ERROR_UNKNOWN)
        }
    }

    ///Returns guard, that guards the execution before this one
    #[allow(dead_code)]
    pub fn guard_before(&self) -> Guard {
        Guard {
            track: self.track,
            target_value: self.target_value.saturating_sub(1),
        }
    }
}

#[cfg(feature = "timestamps")]
#[derive(Hash, PartialEq, Eq)]
pub(crate) struct TimestampRegion {
    from: u32,
    till: u32,
    is_ended: bool,
    name: String,
}

#[cfg(feature = "timestamps")]
#[derive(Clone, Debug, PartialEq)]
pub struct TaskTiming {
    ///Name of the task.
    pub name: String,
    ///Timing in nanoseconds this task used for all operations.
    ///
    /// this means everything that happens in `Task::record`.
    pub timing: f32,
}

#[cfg(feature = "timestamps")]
impl Default for TaskTiming {
    fn default() -> Self {
        TaskTiming {
            name: String::new(),
            timing: 0.0,
        }
    }
}

#[cfg(feature = "timestamps")]
pub(crate) struct TimestampTable {
    timestamps: Timestamps,
    //Keeps track of the range -> pass-name tracking
    // keyed by the start index for that region
    table: AHashMap<u32, TimestampRegion>,
    head: u32,
}

#[cfg(feature = "timestamps")]
impl TimestampTable {
    pub const TIMESTAMP_COUNT: u32 = 128;

    pub fn reset(&mut self, command_buffer: &vk::CommandBuffer) {
        self.timestamps.pool.reset(command_buffer).unwrap();
        self.head = 0;
        self.table.clear();
    }

    ///If there is space for a new region, allocates it, and returns its identification index.
    pub fn start_region(&mut self, command_buffer: &vk::CommandBuffer, name: &str) -> Option<u32> {
        let index = self.head;
        if self.head >= (Self::TIMESTAMP_COUNT - 1) {
            #[cfg(feature = "logging")]
            log::warn!("Could not allocate an new timestamp region.");
            return None;
        }
        self.head += 2;

        let former = self.table.insert(
            index,
            TimestampRegion {
                from: index,
                till: index + 1,
                is_ended: false,
                name: name.to_owned(),
            },
        );

        if former.is_some() {
            #[cfg(feature = "logging")]
            log::error!(
                "Reusing already allocated timestamp index {}. This will result in wrong timings",
                index
            );
        }

        //now actually insert
        self.timestamps.write_timestamp(
            command_buffer,
            vk::PipelineStageFlags2::TOP_OF_PIPE,
            index,
        );

        Some(index)
    }

    ///Marks the region identified by `index` as ended.
    pub fn end_region(&mut self, index: u32, command_buffer: &vk::CommandBuffer) {
        if let Some(region) = self.table.get_mut(&index) {
            self.timestamps.write_timestamp(
                command_buffer,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                region.till,
            );
            region.is_ended = true;
        } else {
            #[cfg(feature = "logging")]
            log::error!("No timestamp region found for index {}", index);
        }
    }
}

///Execution track. Basically a `DeviceQueue` and some associated data.
pub(crate) struct Track {
    pub(crate) queue_idx: u32,
    pub(crate) flags: vk::QueueFlags,
    pub(crate) sem: Arc<Semaphore>,

    pub(crate) command_buffer_pool: OoS<CommandPool>,
    pub(crate) inflight_executions: Vec<Execution>,

    //Latest known value that is going to be signaled eventually.
    pub(crate) latest_signaled_value: u64,

    #[cfg(feature = "timestamps")]
    pub(crate) timestamp_table: TimestampTable,
}

impl Track {
    pub fn new(device: &Arc<Device>, queue_idx: u32, flags: vk::QueueFlags) -> Self {
        let sem = Semaphore::new(device, 0).expect("Could not create Track's semaphore");

        #[cfg(feature = "timestamps")]
        let timestamp_table = {
            let timestamps =
                Timestamps::new(device, TimestampTable::TIMESTAMP_COUNT as usize).unwrap();
            let mut table = AHashMap::default();
            table.reserve(TimestampTable::TIMESTAMP_COUNT as usize / 2);
            TimestampTable {
                timestamps,
                table,
                head: 0,
            }
        };

        Track {
            queue_idx,
            flags,
            sem,
            command_buffer_pool: OoS::new(
                CommandPool::new(
                    device,
                    queue_idx,
                    vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
                )
                .expect("Failed to create command pool!"),
            ),
            inflight_executions: Vec::with_capacity(10),
            latest_signaled_value: 0,

            #[cfg(feature = "timestamps")]
            timestamp_table,
        }
    }

    ///Ticks the track. Triggers internal cleanup operations
    pub fn tick_frame(&mut self) {
        let finished_till = self.sem.get_value();
        //Drop all executions (and therefore resources like buffers etc) that have finished till now
        self.inflight_executions
            .retain(|exec| exec.guard.target_value >= finished_till);
    }

    ///Allocates the next guard for this track.
    pub fn next_guard(&mut self) -> Guard {
        self.latest_signaled_value += 1;

        Guard {
            track: TrackId(self.flags),
            target_value: self.latest_signaled_value,
        }
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

    pub fn new_command_buffer(&mut self) -> Result<CommandBuffer, RecordError> {
        let cb = self
            .command_buffer_pool
            .share()
            .allocate_buffer(vk::CommandBufferLevel::PRIMARY)
            .map_err(|e| RecordError::MarpiiError(e.into()))?;

        Ok(cb)
    }

    /// Appends all known timings from the last execution.
    /// Note that, depending on how heavy the workload is, some timings might not (yet) be available.
    ///
    /// This call however does *not* block the CPU till all executions are ready. For that, use the [blocking](Self::get_recent_task_timings_blocking)
    /// alternative.
    #[cfg(feature = "timestamps")]
    pub fn get_recent_task_timings<const N: usize>(&mut self, dst: &mut TinyVec<[TaskTiming; N]>)
    where
        [TaskTiming; N]: Array<Item = TaskTiming>,
    {
        let increment = self.timestamp_table.timestamps.get_timestamp_increment();
        if let Ok(timings) = self.timestamp_table.timestamps.get_timestamps() {
            for idx in 0..timings.len() {
                //Do not consider if not even the start index is known.
                if let Some(start_timing) = &timings[idx] {
                    if let Some(scheduled) = self.timestamp_table.table.get(&(idx as u32)) {
                        //try to find end as well
                        if let Some(end_timing) = &timings[scheduled.till as usize] {
                            let nanoseconds =
                                ((end_timing - start_timing) as f32 * increment) / 1_000_000.0;
                            dst.push(TaskTiming {
                                name: scheduled.name.clone(),
                                timing: nanoseconds,
                            });
                        }
                    }
                }
            }
        }
    }

    #[cfg(feature = "timestamps")]
    pub fn get_recent_task_timings_blocking<const N: usize>(
        &mut self,
        dst: &mut TinyVec<[TaskTiming; N]>,
    ) where
        [TaskTiming; N]: Array<Item = TaskTiming>,
    {
        let increment = self.timestamp_table.timestamps.get_timestamp_increment();
        let timings = self
            .timestamp_table
            .timestamps
            .get_timestamps_blocking()
            .unwrap();
        for (idx, start_timing) in timings.iter().enumerate() {
            if let Some(scheduled) = self.timestamp_table.table.get(&(idx as u32)) {
                //get end timing
                let end_timing = timings[scheduled.till as usize];
                let nanoseconds = ((end_timing - start_timing) as f32 * increment) / 1_000_000.0;
                dst.push(TaskTiming {
                    name: scheduled.name.clone(),
                    timing: nanoseconds,
                });
            }
        }
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct TrackId(pub vk::QueueFlags);

impl TrackId {
    ///Creates a trackId with no capabilities.
    pub fn empty() -> Self {
        TrackId(vk::QueueFlags::empty())
    }
}

impl From<vk::QueueFlags> for TrackId {
    fn from(f: vk::QueueFlags) -> Self {
        TrackId(f)
    }
}

impl Display for TrackId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TrackId({:?})", self.0)
    }
}

pub(crate) struct Tracks(pub AHashMap<TrackId, Track>);

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

    /// Appends all known timings from the last execution.
    /// Note that, depending on how heavy the workload is, some timings might not (yet) be available.
    ///
    /// This call however does *not* block the CPU till all executions are ready. For that, use the [blocking](Self::get_recent_task_timings_blocking)
    /// alternative.
    #[cfg(feature = "timestamps")]
    pub fn get_recent_task_timings(&mut self) -> TinyVec<[TaskTiming; 16]> {
        let mut vec = tinyvec::tiny_vec!([TaskTiming; 16]);
        for track in self.0.values_mut() {
            track.get_recent_task_timings(&mut vec);
        }

        vec
    }

    #[cfg(feature = "timestamps")]
    pub fn get_recent_task_timings_blocking(&mut self) -> TinyVec<[TaskTiming; 16]> {
        let mut vec = tinyvec::tiny_vec!([TaskTiming; 16]);
        for track in self.0.values_mut() {
            track.get_recent_task_timings_blocking(&mut vec);
        }

        vec
    }
}
