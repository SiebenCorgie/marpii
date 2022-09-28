pub(crate) mod executor;
pub(crate) mod frame;
pub(crate) mod scheduler;
pub(crate) mod task;

use std::fmt::Debug;

use marpii::ash::vk;
use thiserror::Error;

use crate::{AnyResKey, ResourceError, Rmg, Task};

use self::{
    executor::Executor,
    scheduler::Schedule,
    task::ResourceRegistry,
};

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("No fitting track for flags found")]
    NoFittingTrack(vk::QueueFlags),

    #[error("No such resource found")]
    NoSuchResource(AnyResKey),

    #[error("Resource {0} was owned still owned by {1} while trying to acquire.")]
    AcquireRecord(AnyResKey, u32),
    #[error("Resource {0} was already released from {1} to {2} while trying to release.")]
    ReleaseRecord(AnyResKey, u32, u32),

    #[error("Resource {0} was not owned while trying to release.")]
    ReleaseUninitialised(AnyResKey),

    #[error("Vulkan recording error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Record time resource error")]
    ResError(#[from] ResourceError),
}

pub(crate) struct TaskRecord<'t> {
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
    records: Vec<TaskRecord<'rmg>>,
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
                log::error!(
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
    pub fn add_task(
        mut self,
        task: &'rmg mut dyn Task,
    ) -> Result<Self, RecordError> {
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
        schedule.print_schedule();

        let executions = Executor::exec(self.rmg, schedule)?;

        for ex in executions {
            let track = self.rmg.tracks.0.get_mut(&ex.guard.into()).unwrap();
            track.inflight_executions.push(ex);
        }

        Ok(())
    }
}
