//pub(crate) mod executor;
//pub(crate) mod frame;
//pub(crate) mod scheduler;
pub mod task;
pub(crate) mod task_executor;
pub(crate) mod task_scheduler;

use std::fmt::Debug;

use crate::{resources::handle::AnyHandle, track::Guard, ResourceError, Rmg, Task};
use marpii::{ash::vk, resources::CommandBuffer, MarpiiError};
use std::any::Any;
use thiserror::Error;

use self::{
    task::{MetaTask, ResourceRegistry},
    task_executor::Executor,
    task_scheduler::TaskSchedule,
};

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("No fitting track for flags {0:?} found")]
    NoFittingTrack(vk::QueueFlags),

    #[error("No such resource found")]
    NoSuchResource(AnyHandle),

    #[error("No track for queue_index {0}")]
    NoSuchTrack(u32),

    #[error("Resource {0} was owned still owned by {1} while trying to acquire.")]
    AcquireRecord(AnyHandle, u32),
    #[error("Resource {0} was already released from {1} to {2} while trying to release.")]
    ReleaseRecord(AnyHandle, u32, u32),

    #[error("Resource {0} was not owned while trying to release.")]
    ReleaseUninitialised(AnyHandle),

    #[error("Resource {0} was already release.")]
    AlreadyReleased(AnyHandle),

    #[error("Found unscheduled dependee scheduled for release")]
    UnscheduledDependee,

    #[error("Vulkan recording error")]
    VkError(#[from] vk::Result),

    #[error("MarpII internal error: {0}")]
    MarpiiError(#[from] MarpiiError),

    #[error("Record time resource error")]
    ResError(#[from] ResourceError),

    #[error("Found scheduling deadlock, there might be a dependency cycle.")]
    DeadLock,
}

pub struct Execution {
    ///All resources that need to be kept alive until the execution finishes
    #[allow(dead_code)]
    pub(crate) resources: Vec<Box<dyn Any + Send>>,
    ///The command buffer that is executed
    #[allow(dead_code)]
    pub(crate) command_buffer: CommandBuffer,
    ///Until when it is guarded.
    pub(crate) guard: Guard,
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
        task.pre_record(&mut self.rmg.resources, &self.rmg.ctx)?;
        //build registry
        let mut registry = ResourceRegistry::new();
        task.register(&mut registry);

        let record = TaskRecord { task, registry };

        self.records.push(record);

        Ok(self)
    }

    pub fn add_meta_task(self, meta_task: &'rmg mut dyn MetaTask) -> Result<Self, RecordError> {
        meta_task.record(self)
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
