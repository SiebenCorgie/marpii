//pub(crate) mod executor;
//pub(crate) mod frame;
//pub(crate) mod scheduler;
pub mod task;
pub(crate) mod task_executor;
pub(crate) mod task_scheduler;

use std::{fmt::Debug, sync::Arc};

use crate::{
    resources::handle::AnyHandle,
    track::{Guard, TrackId},
    ResourceError, Rmg, Task,
};
use marpii::{
    ash::vk,
    resources::{CommandBuffer, CommandPool},
};
use std::any::Any;
use thiserror::Error;

use self::{task::ResourceRegistry, task_scheduler::TaskSchedule, task_executor::Executor};

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

pub struct Execution {
    ///All resources that need to be kept alive until the execution finishes
    #[allow(dead_code)]
    pub(crate) resources: Vec<Box<dyn Any + Send>>,
    ///The command buffer that is executed
    #[allow(dead_code)]
    pub(crate) command_buffer: CommandBuffer<Arc<CommandPool>>,
    ///Until when it is guarded.
    pub(crate) guard: Guard,
}

impl Default for WaitEvent {
    fn default() -> Self {
        WaitEvent {
            track: TrackId::empty(),
            block_sem: 0,
        }
    }
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
}

impl<'rmg> Recorder<'rmg> {
    pub fn new(rmg: &'rmg mut Rmg) -> Self {
        Recorder {
            rmg,
            records: Vec::new(),
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
        let schedule = TaskSchedule::new_from_tasks(self.rmg, self.records)?;
        let executions = Executor::execute(self.rmg, schedule)?;
        for ex in executions {
            let track = self.rmg.tracks.0.get_mut(&ex.guard.into()).unwrap();
            track.inflight_executions.push(ex);
        }

        Ok(())
    }
}
