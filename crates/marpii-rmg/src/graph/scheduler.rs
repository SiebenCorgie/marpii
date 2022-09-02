use std::{sync::Arc, collections::VecDeque};
use fxhash::FxHashMap;
use marpii::ash::vk::QueueFlags;
use marpii::sync::Semaphore;
use crate::{resources::{ImageKey, BufferKey, Resources}, Rmg, TrackId, task::Attachment};

use super::{TaskRecord, TaskAttachment};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Required resource could not be found. This is most likely a bug and should be reported")]
    ResorceNotInTrack,
    #[error("Could not find queue that contains this flags: {0:?}")]
    CouldNotFindQueue(QueueFlags),
    #[error("Could not find resource: {0:?} in working set, or the resource manager.")]
    CouldNotFindResource(Res)
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, PartialOrd)]
enum Res{
    Image(ImageKey),
    Buffer(BufferKey)
}

///Single CommandFrame. Is a set of tasks that are only synchronised via barriers.
struct CommandFrame<'a>{
    ///Acquire operations, containing the source track.
    acquires: Vec<(TrackId, Res)>,
    initialize: Vec<Res>,
    ///Release operations, containing the destination track.
    release: Vec<(TrackId, Res)>,
    tasks: Vec<TaskRecord<'a>>
}

impl<'a> CommandFrame<'a>  {

    fn new() -> Self{
        CommandFrame{
            acquires: Vec::new(),
            initialize: Vec::new(),
            release: Vec::new(),
            tasks: Vec::new()
        }
    }

    fn owns_res(&self, res: &Res) -> bool{
        //TODO: Cache
        self.acquires.iter().map(|ac| ac.1).find(|r| r == res).is_some() && !self.release.iter().map(|rel| rel.1).find(|r| r == res).is_none()
    }
}

///Schedule track for one queue
struct QueueTrack<'a>{
    ///Capability of this track
    track_id: TrackId,
    cmd_frames: Vec<CommandFrame<'a>>,

    ///This tracks timeline semaphore
    sem: Arc<Semaphore>,
    ///Semaphores start value. Note that a frames sync value is this start `value+frame_index`.
    sem_start: u64,
}

impl<'a> QueueTrack<'a>{
    ///Returns the currently worked on frame.
    fn current_frame(&self) -> usize{
        self.cmd_frames.len()
    }

    fn track_sem_val(&self, index: u64) -> u64{
        self.sem_start + index
    }

    fn active(&mut self) -> &mut CommandFrame{
        debug_assert!(self.cmd_frames.len() > 0); //Should always be the case, but for sanity
        self.cmd_frames.last_mut().as_mut().unwrap()
    }
}

///A schedule local attachment
struct RuntimeAttachment{
    key: ImageKey,
    info: Attachment,
}

///Schedule build from a Recorder. Takes care of finding the right queue for each task and defining inter-queue dependencies.
pub(crate) struct Schedule<'a>{
    tracks: Vec<QueueTrack<'a>>,
}

impl<'a> Schedule<'a> {
    pub(crate) fn from_tasks(rmg: &'a mut Rmg, tasks: Vec<TaskRecord<'a>>) -> Result<Self, SchedulerError>{
        let mut schedule = Schedule{
            tracks: rmg.tracks.iter().map(|track| QueueTrack{
                track_id: *track.0,
                //Add first, now active frame
                cmd_frames: vec![CommandFrame::new()],
                sem: track.1.sem.clone(),
                sem_start: track.1.sem_target //NOTE: Safe, since we borrow the rmg until we execute. therefore it won't change
            }).collect(),
        };

        //TODO: Before scheduling we could do a reverse search and check if there are unneeded passes. However, every write
        //      to a resource is a potentially needed sideeffect. So this is not that easy...

        //now schedule tracks
        //
        // Names:
        // - Active frame: The last member of cmd_frame in a QueueTrack
        //
        // This works by pushing each task into a fitting tracks active frame, as long as all its dependencies have been submitted.
        // If a dependency from an active Frame of another track is needed, we finish the track. And record the Realease of the resource.
        let mut tasks: VecDeque<_> = tasks.into();
        while let Some(task) = tasks.pop_front() {
            //first find the correct track for the workload.
            let track_id = if let Some((track_idx, _track)) = schedule.tracks.iter().enumerate().find(|(idx, track)| track.track_id.contains(task.capability)){
                track_idx
            }else{
                #[cfg(feature="logging")]
                log::error!("Could not find Queue with bit containing: {:?}", task.capability);

                return Err(SchedulerError::CouldNotFindQueue(task.capability));
            };

            //found track, gather dependencies and add to tracks active frame.
            // This includes requesting all static resources of the task, as well as querying for temporal
            // attachments, and if they are not created yet, adding them.

            //Check which resources we have to acquire...
            for res in task.buffers.iter().map(|bufid| Res::Buffer(*bufid)).chain(task.images.iter().map(|imgid| Res::Image(*imgid))){
                //If we don't own already, find resource in tracks/global resources
                schedule.request_res(rmg.res_mut(), &res, track_id)?;
            }

            //now create/bind all attachments
            //Note that the correct read/write rules are already enforced at the recording step,
            // we therefore only need to make sure that the images ownership is correct.
            for att in task.attachments.iter(){
                schedule.request_att(rmg.res_mut(), att, track_id)?;
            }

            //Since the track is now up to date, push the task
            schedule.tracks[track_id].active().tasks.push(task);
        }


        todo!();

        Ok(schedule)
    }

    ///Repuests a resource from any of the tracks. Adds a release operation if released from a track.
    ///
    /// If found returns the src queue (identified by its flags) and the resource. Note that it is possible that the src_queue is
    /// the target queue. In that case this resource was acquired from the global resource store and happens to be on the right queue already.
    ///
    /// There are three side effects that can happen.
    /// 1. An active frame is finished.
    /// 2. An finished frame gains a release operation
    /// 3. An not yet used Res gets imported from `resources`, which might add a release operation on any of the tracks.
    fn request_res(&mut self, resources: &mut Resources, res: &Res, track: usize) -> Result<(), SchedulerError>{

        if let Some((src_track, src_track_frame)) = self.find_owner(res){
            //Found somewhere. Add a release for this resource to the current owner and acquire it for "us"
            let src_track_id = self.tracks[src_track].track_id;
            let dst_track_id = self.tracks[track].track_id;

            self.tracks[src_track].cmd_frames[src_track_frame].release.push((dst_track_id, *res));
            //and add it to our self
            self.tracks[track].active().acquires.push((src_track_id, *res));

            debug_assert!(self.find_owner(res) == Some((track, self.tracks.len()-1)), "New owner does not match!");
        }else{
            //Not loaded at all, gather from res manager.
            //
            // Note that the resource might be owned by some other resource still, we therefor add an release operation to that track if
            // needed. We then acquire on our queue anyways.
            //
            // If it isn't owned by us, or anyone else, this means we are initialising.
            match res{
                Res::Image(imgkey) => {
                    if let Some(img) = resources.get_image(*imgkey){
                        //Check who the owner is and if needed set appropriate acquire / release operations. In case of no owner,
                        // set as init op.
                        todo!();
                    }else{
                        #[cfg(feature="logging")]
                        log::error!("Failed to get image for key {:?} while requesting resource.", imgkey);
                        return Err(SchedulerError::CouldNotFindResource(*res));
                    }
                },
                Res::Buffer(bufkey) => {
                    todo!()
                }
            }
        }

        Ok(())
    }

    ///Requests the given attachment.
    fn request_att(&mut self, resources: &mut Resources, att: &TaskAttachment, track: usize) -> Result<(), SchedulerError>{
        //TODO: Search for it, release if found. Otherwise return for given track.


        Ok(())
    }

    ///Returns if found (owning_track_index, owning_frame_in_track)
    pub fn find_owner(&self, res: &Res) -> Option<(usize, usize)>{
        //TODO: Accelerate with an lookup table or something

        for track_id in 0..self.tracks.len(){
            for frame_id in 0..self.tracks[track_id].cmd_frames.len(){
                if self.tracks[track_id].cmd_frames[frame_id].owns_res(res){
                    return Some((track_id, frame_id));
                }
            }
        }

        None
    }

    pub fn execute(mut self){
        todo!("Execute unimplemented")
    }

    pub fn set_present_image(&mut self, attachment: &str){
        todo!("Present unimplenmented")
    }
}
