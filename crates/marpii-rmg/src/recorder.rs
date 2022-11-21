pub(crate) mod executor;
pub(crate) mod frame;
pub(crate) mod scheduler;
pub mod task;
pub mod task_executor;
pub mod task_scheduler;

use std::fmt::Debug;

use marpii::ash::vk;
use thiserror::Error;
use tinyvec::TinyVec;

use crate::{resources::handle::AnyHandle, track::TrackId, ResourceError, Rmg, Task};

use self::{executor::Executor, scheduler::Schedule, task::ResourceRegistry};

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("No fitting track for flags {0:?} found")]
    NoFittingTrack(vk::QueueFlags),

    #[error("No such resource found")]
    NoSuchResource(AnyHandle),

    #[error("Resource {0} was owned still owned by {1} while trying to acquire.")]
    AcquireRecord(AnyHandle, u32),
    #[error("Resource {0} was already released from {1} to {2} while trying to release.")]
    ReleaseRecord(AnyHandle, u32, u32),

    #[error("Resource {0} was not owned while trying to release.")]
    ReleaseUninitialised(AnyHandle),

    #[error("Vulkan recording error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Record time resource error")]
    ResError(#[from] ResourceError),

    #[error("Found scheduling deadlock, there might be a dependency cycle.")]
    DeadLock,
}

pub(crate) struct WaitEvent {
    ///The block ID we are waiting for.
    track: TrackId,
    ///The semaphore value that needs to be reached on the track before continuing.
    block_sem: u64,
}

impl Default for WaitEvent {
    fn default() -> Self {
        WaitEvent {
            track: TrackId::empty(),
            block_sem: 0,
        }
    }
}

///Abstract events that can occur on a track.
///
/// Block: A sequential block of task(s) that can be executed without having to wait for another track to finish a specific block.
///
/// Wait: Wait operation that waits for one or more blocks on possibly multiple other tracks.
///
/// Barrier: On Track barrier for resources. Theoretically there can be one in between each block. However, the scheduler should try and minimize those.
pub(crate) enum TrackEvent<'t> {
    Block(Vec<TaskRecord<'t>>),
    Wait(TinyVec<[WaitEvent; 3]>),
    Barrier,
}

pub struct TaskRecord<'t> {
    task: &'t mut dyn Task,
    registry: ResourceRegistry,
}

impl<'t> Debug for TaskRecord<'t> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Task [{}]", self.task.name())
    }
}

///records a new execution graph blocks any access to `rmg` until the graph is executed.
pub struct Recorder<'rmg> {
    pub rmg: &'rmg mut Rmg,
    pub records: Vec<TaskRecord<'rmg>>,
    #[allow(dead_code)]
    framebuffer_extent: vk::Extent2D,
}

impl<'rmg> Recorder<'rmg> {
    pub fn new(rmg: &'rmg mut Rmg, window_extent: vk::Extent2D) -> Self {
        let framebuffer_extent = rmg
            .res
            .swapchain
            .surface
            .get_current_extent(&rmg.ctx.device.physical_device)
            .unwrap_or({
                #[cfg(feature = "logging")]
                log::info!(
                    "Failed to get surface extent, falling back to window extent={:?}",
                    window_extent
                );
                window_extent
            });
        rmg.res.last_known_surface_extent = framebuffer_extent;

        Recorder {
            rmg,
            records: Vec::new(),
            framebuffer_extent,
        }
    }

    ///Adds `task` to the execution plan. Optionally naming the task's attachments (in order of definition) with the given names.
    pub fn add_task(mut self, task: &'rmg mut dyn Task) -> Result<Self, RecordError> {
        task.pre_record(&mut self.rmg.res, &self.rmg.ctx)?;
        //build registry
        let mut registry = ResourceRegistry::new();
        task.register(&mut registry);

        let record = TaskRecord { task, registry };

        self.records.push(record);

        Ok(self)
    }

    ///Schedules everything for execution
    pub fn execute(self) -> Result<(), RecordError> {
        let schedule = Schedule::from_tasks(self.rmg, self.records)?;
        //schedule.print_schedule();

        let executions = Executor::exec(self.rmg, schedule)?;

        for ex in executions {
            let track = self.rmg.tracks.0.get_mut(&ex.guard.into()).unwrap();
            track.inflight_executions.push(ex);
        }

        Ok(())
    }
}
