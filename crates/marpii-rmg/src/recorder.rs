

pub(crate) mod task;
mod scheduler;
mod executor;

use marpii::ash::vk;
use thiserror::Error;

use crate::{Rmg, Task, track::TrackId};

use self::{task::ResourceRegistry, scheduler::Schedule, executor::Executor};

#[derive(Debug, Error)]
pub enum RecordError{
    #[error("No fitting track for flags found")]
    NoFittingTrack(vk::QueueFlags),
}

pub(crate) struct TaskRecord<'t>{
    task: &'t dyn Task,
    registry: ResourceRegistry<'t>,
}

///records a new execution graph blocks any access to `rmg` until the graph is executed.
pub struct Recorder<'rmg>{
    pub rmg: &'rmg mut Rmg,
    records: Vec<TaskRecord<'rmg>>,
    framebuffer_extent: vk::Extent2D,
}


impl<'rmg> Recorder<'rmg> {
    pub fn new(rmg: &'rmg mut Rmg) -> Self{

        let framebuffer_extent = rmg.swapchain.surface.get_current_extent(&rmg.ctx.device.physical_device).unwrap_or({
            #[cfg(feature="logging")]
            log::error!("Failed to get surface extent, falling back to 1x1");

            vk::Extent2D{
                width: 1,
                height: 1
            }
        });

        Recorder {
            rmg,
            records: Vec::new(),
            framebuffer_extent
        }
    }


    ///Adds `task` to the execution plan. Optionally naming the task's attachments (in order of definition) with the given names.
    pub fn add_task(mut self, task: &'rmg dyn Task, attachment_names: &'rmg[&'rmg str]) -> Result<Self, RecordError>{
        //build registry
        let mut registry = ResourceRegistry::new(attachment_names);
        task.register(&mut registry);

        println!("resolve attachments to actual ids and register name->key mapping for pass");

        let record = TaskRecord{
            task,
            registry
        };

        self.records.push(record);


        Ok(self)
    }

    ///Schedules everything for execution
    pub fn execute(self) -> Result<(), RecordError>{
        let schedule = Schedule::from_tasks(self.rmg, self.records)?;

        Executor::exec(schedule)
    }
}
