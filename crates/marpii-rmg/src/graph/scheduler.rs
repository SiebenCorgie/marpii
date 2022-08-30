use fxhash::FxHashSet;
use marpii::ash::vk::QueueFlags;

use crate::{resources::{ImageKey, BufferKey}, Rmg};

use super::TaskRecord;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Required resource could not be found. This is most likely a bug and should be reported")]
    ResorceNotInTrack,
    #[error("Could not find queue that contains this flags: {0:?}")]
    CouldNotFindQueue(QueueFlags)
}

#[derive(Hash, Eq, PartialEq, PartialOrd)]
enum Res{
    Image(ImageKey),
    Buffer(BufferKey)
}

///Single CommandFrame. Is a set of tasks that are only synchronised via barriers.
struct CommandFrame<'a>{

    acquires: Vec<Res>,
    release: Vec<Res>,
    tasks: Vec<TaskRecord<'a>>
}

impl<'a> CommandFrame<'a>  {

    fn new() -> Self{
        CommandFrame{
            acquires: Vec::new(),
            release: Vec::new(),
            tasks: Vec::new()
        }
    }
    ///Retuns all resources that are owned by this frame at the moment.
    fn owning(&self) -> impl Iterator<Item = &Res>{
        self.acquires.iter().filter(|itm| !self.release.contains(itm))
    }
}

///Schedule track for one queue
struct QueueTrack<'a>{
    ///Capability of this track
    capability: QueueFlags,
    //currently unscheduled tasks
    task_queue: Vec<TaskRecord<'a>>,
    cmd_frames: Vec<CommandFrame<'a>>
}

impl<'a> QueueTrack<'a>{
    ///Returns the currently worked on frame.
    fn current_frame(&self) -> usize{
        self.cmd_frames.len()
    }
}


///Schedule build from a Recorder. Takes care of finding the right queue for each task and defining inter-queue dependencies.
pub(crate) struct Schedule<'a>{
    tracks: Vec<QueueTrack<'a>>
}

impl<'a> Schedule<'a> {
    pub(crate) fn from_tasks(rmg: &'a mut Rmg, tasks: Vec<TaskRecord<'a>>) -> Result<Self, SchedulerError>{
        let mut schedule = Schedule{
            tracks: rmg.tracks.iter().map(|track| QueueTrack{
                capability: *track.0,
                task_queue: Vec::new(),
                cmd_frames: Vec::new()
            }).collect(),
        };

        //First we move the tasks to the correct queue track.
        // We then collect attachment and resource dependencies between all tasks.
        //
        // Attachments are temporary resources that are created on demand. After the initial write they behave
        // the same as normal resources, but can be addressed by their given temporary name.
        //
        // Other resources are addressed by their Key/Id. The whole graph uses the bindless model.


        //moving tasks into "best" track.
        'task_loop: for task in tasks{
            for track in schedule.tracks.iter_mut(){
                if track.capability.contains(task.capability){
                    track.task_queue.push(task);
                    continue 'task_loop;
                }
            }

            #[cfg(feature="log")]
            log::error!("could not find queue with capability {:?}", task.capability);
            return Err(SchedulerError::CouldNotFindQueue(task.capability))
        }

        //now schedule tracks
        //
        // For this to work we have to query the current state of met and unmet dependencies for each track.
        //
        todo!();

        Ok(schedule)
    }

    pub(crate) fn execute(mut self){
        todo!("Execute unimplemented")
    }

    pub fn set_present_image(&mut self, attachment: &str){
        todo!("Present unimplenmented")
    }
}
