use marpii::ash::vk::{self, QueueFlags};
use thiserror::Error;

use crate::{
    resources::{BufferHdl, BufferKey, ImageHdl, ImageKey, ResourceError},
    task::{AccessType, Attachment, Task},
    Rmg,
};

use self::scheduler::{Schedule, SchedulerError};

mod scheduler;

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("anyhow")]
    Any(#[from] anyhow::Error),
    #[error("Task does not exist (anymore?)")]
    TaskDoesNotExist,
    #[error("Pass reads from undefined attachment {0}")]
    ReadUndefined(String),
    #[error("Pass overwriting an already defined attachment ({0}) is not allowed (yet).")]
    OverwriteAttachment(String),
    #[error("Scheduling failed")]
    SchedulingFailed(#[from] SchedulerError),
    #[error("Swapchain error")]
    SwapchainError,
    #[error("Resource Error")]
    ResourceError(#[from] ResourceError),
}

///task graph (possibly render graph) recorder. Schedules subtasks in a way that the sequential order of the
/// recording steps is respected.
pub struct Recorder<'a> {
    rmg: &'a mut Rmg,
    framebuffer_ext: vk::Extent2D,
    tasks: Vec<TaskRecord<'a>>,
}

impl<'a> Recorder<'a> {
    ///Reserved name for undefined attachments.
    pub const UNDEFINED_ATTACHMENT: &'static str = "_UNDEFINED_ATTACHMENT";

    pub fn new(rmg: &'a mut Rmg, current_ext: vk::Extent2D) -> Self {
        Recorder {
            rmg,
            tasks: Vec::new(),
            framebuffer_ext: current_ext,
        }
    }

    ///Finishes the tasklist by presenting `present_image`. Might add an additional blit operation if the image is in the wrong format.
    pub fn present(mut self, present_image: &str) -> Result<(), RecordError> {
        //TODO: Needs to acquire a new image, setup semaphores etc. to schedule execution.

        let img = if let Ok(img) = self.rmg.swapchain.acquire_next_image() {
            img
        } else {
            #[cfg(feature = "logging")]
            log::warn!("Getting swapchain image failed, trying recreation");
            let current_ext = self
                .rmg
                .swapchain
                .surface
                .get_current_extent(&self.rmg.ctx.device.physical_device)
                .ok_or(RecordError::SwapchainError)?;
            self.rmg.swapchain.recreate(current_ext);
            self.rmg.swapchain.acquire_next_image()?
        };

        todo!("Add present pass correctly");

        let Recorder {
            rmg,
            tasks,
            framebuffer_ext,
        } = self;
        let mut schedule = Schedule::from_tasks(rmg, tasks)?;
        schedule.set_present_image(present_image);
        schedule.execute();
        Ok(())
    }
    ///Finishes the pass by executing the accumulated tasks on the GPU.
    pub fn execute(mut self) -> Result<(), RecordError> {
        let Recorder {
            rmg,
            tasks,
            framebuffer_ext,
        } = self;
        let schedule = Schedule::from_tasks(rmg, tasks)?;
        schedule.execute();
        Ok(())
    }

    fn get_attachment(&self, name: &str) -> Option<ImageKey> {
        //not allowed, there might be multiple
        if name == Self::UNDEFINED_ATTACHMENT {
            return None;
        }
        //search for any with this name.
        // NOTE: We could filter for attachments that are writing. But there wouldn't be an read attachment
        //       with this name if there wasn't a write before anyways. So we can take any with this name.
        for t in &self.tasks {
            for a in &t.attachments {
                if a.name == name {
                    return Some(a.key);
                }
            }
        }

        None
    }

    ///Adds the pass to the graph, given the temporary resource names. Note that undefined write targets won't be accessible for
    /// following tasks. If you need information about the attachments use the task's [Task::attachments](Task::attachments) function. The names
    /// are only temporarily mapped to the attachments.
    pub fn pass(
        mut self,
        task: &'a dyn Task,
        attachment_names: &[&'a str],
    ) -> Result<Self, RecordError> {
        #[cfg(feature = "logging")]
        if attachment_names.len() < task.attachments().len() {
            log::warn!("Task had unnamed attachments!");
        }

        //register write attachments and collect read attachments
        let mut attachments = Vec::with_capacity(attachment_names.len());

        //match attachments and names. On read attachment try to find the dependency. On write register new one.
        for (idx, att) in task.attachments().iter().enumerate() {
            let name = if idx < attachments.len() {
                attachment_names[idx]
            } else {
                #[cfg(feature = "log_reasoning")]
                log::trace!("Undefined name for task attachment at index={}", idx);
                Self::UNDEFINED_ATTACHMENT
            };

            match att.access {
                AccessType::Read => {
                    let att_key = if let Some(k) = self.get_attachment(name) {
                        k
                    } else {
                        return Err(RecordError::ReadUndefined(name.to_string()));
                    };

                    attachments.push(TaskAttachment {
                        info: att.clone(),
                        key: att_key,
                        name,
                    });
                }
                AccessType::Write => {
                    //make sure we don't overwrite. Is theory this can be okay in Vulkan, but its hard to track and probably unwanted.
                    // Therefore we don't allow it.
                    if self.get_attachment(name).is_some() {
                        return Err(RecordError::OverwriteAttachment(name.to_string()));
                    }

                    let key = self.rmg.res.tmp_image(
                        att.as_desc(self.framebuffer_ext),
                        &self.rmg.ctx,
                        &self.rmg.tracks,
                    )?;

                    attachments.push(TaskAttachment {
                        info: att.clone(),
                        key,
                        name,
                    });
                }
            }
        }
        //just to be sure all attachments are resolved.
        assert!(task.attachments().len() == attachments.len());

        //now we can resolve the normal buffers and images.
        let buffers = task.buffers().to_vec(); //FIXME: Performance: On no allocation on hot path :(
        let images = task.images().to_vec();

        self.tasks.push(TaskRecord {
            task_id: self.tasks.len(),
            task,
            capability: task.queue_flags(),
            attachments,
            buffers,
            images,
        });

        Ok(self)
    }
}

pub(crate) struct TaskAttachment<'a> {
    info: Attachment,
    key: ImageKey,
    name: &'a str,
}

///Record of a single task. Carries all context information needed to execute the task
/// in the correct environment.
pub struct TaskRecord<'a> {
    // Its index on the global task list of this graph
    pub(crate) task_id: usize,
    task: &'a dyn Task,
    ///Declares capabilities needed to run this task.
    pub capability: QueueFlags,
    ///Attachments in order defined by the tasks `attachments` return.
    pub(crate) attachments: Vec<TaskAttachment<'a>>,
    pub(crate) buffers: Vec<BufferKey>,
    pub(crate) images: Vec<ImageKey>,
}

impl<'a> TaskRecord<'a> {
    pub fn need_buffer<T>(&mut self, buffer: BufferHdl<T>) {
        //TODO only allow one "need" per key
        todo!("Impl state assignment and type erasure");
    }

    pub fn need_image(&mut self, image: ImageHdl) {
        todo!("Impl state assignment and type erasure");
    }
}

#[cfg(test)]
mod tests {
    use crate::task::{DummyTask, READATT};

    #[test]
    fn add_task() {
        let a = DummyTask {
            attachments: [READATT],
        };
        //TODO impl the tasks.
        //todo!("check that adding etc. works as expected");
    }
}
