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

///Abstract Resource type used within the scheduler. Allows us to not care about the actual type.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, PartialOrd)]
pub enum Res{
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
    outside_dependency: Option<u64>,
    tasks: Vec<TaskRecord<'a>>
}

impl<'a> CommandFrame<'a>  {

    fn new() -> Self{
        CommandFrame{
            acquires: Vec::new(),
            initialize: Vec::new(),
            release: Vec::new(),
            outside_dependency: None,
            tasks: Vec::new()
        }
    }

    fn owns_res(&self, res: &Res) -> bool{
        //TODO: Cache
        self.acquires.iter().map(|ac| ac.1).find(|r| r == res).is_some() && !self.release.iter().map(|rel| rel.1).find(|r| r == res).is_none()
    }

    fn add_outside_dep(&mut self, val: u64){
        if let Some(dep) = self.outside_dependency{
            self.outside_dependency = Some(dep.max(val)); //always wait for the maximum of this track to ocure
        }else {
            self.outside_dependency = Some(val);
        }
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
        self.cmd_frames.len() - 1
    }

    fn track_sem_val(&self, index: u64) -> u64{
        self.sem_start + index
    }

    fn finish_frame(&mut self){
        self.cmd_frames.push(CommandFrame::new());
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
        let mut tasks: VecDeque<TaskRecord<'a>> = tasks.into();
        while let Some(task) = tasks.pop_front() {
            //first find the correct track for the workload.
            let track_id = if let Some((track_idx, _track)) = schedule.tracks.iter().enumerate().find(|(idx, track)| track.track_id.contains(task.capability)){
                track_idx
            }else{
                #[cfg(feature="logging")]
                log::error!("Could not find Queue with bit containing: {:?}", task.capability);

                return Err(SchedulerError::CouldNotFindQueue(task.capability));
            };


            #[cfg(feature="logging")]
            log::info!("Scheduling task on track {}", track_id);


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
                #[cfg(feature="logging")]
                log::info!("Requesting attachment {} under id={:?}", att.name, att.key);
                schedule.request_res(rmg.res_mut(), &Res::Image(att.key), track_id)?;
            }

            //Since the track is now up to date, push the task
            // Safety: Note that there is always at least one frame.
            schedule.tracks[track_id].cmd_frames.last_mut().unwrap().tasks.push(task);
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

            #[cfg(feature="logging")]
            log::info!("Internal transfer of {:?}: {} -> {}", res, src_track, track);


            self.tracks[src_track].cmd_frames[src_track_frame].release.push((dst_track_id, *res));
            //and add it to our self
            self.tracks[track].cmd_frames.last_mut().unwrap().acquires.push((src_track_id, *res));

            //Note that we need to finalize a frame if we have to release resources. Otherwise a later scheduled task on that track could try
            // to use the resource without acquiring it first.
            if src_track_frame == self.tracks[src_track].current_frame(){
                #[cfg(feature="logging")]
                log::info!("Finish frame {} with num acquires: {}", src_track_frame, self.tracks[src_track].cmd_frames[src_track_frame].acquires.len());
                self.tracks[src_track].finish_frame();
            }

            debug_assert!(self.find_owner(res) == Some((track, self.tracks.len()-1)), "New owner does not match!");
        }else{
            //Not loaded at all, gather from res manager.
            //
            // Note that the resource might be owned by some other resource still, we therefor add an release operation to that track if
            // needed. We then acquire on our queue anyways.
            //
            // If it isn't owned by us, or anyone else, this means we are initialising.
            let guard = match res{
                Res::Image(imgkey) => {
                    if let Some(img) = resources.get_image_mut(*imgkey){
                        //Take the guard and schedule the wait
                        img.guard.take()
                    }else{
                        #[cfg(feature="logging")]
                        log::error!("Failed to get image for key {:?} while requesting resource.", imgkey);
                        return Err(SchedulerError::CouldNotFindResource(*res));
                    }
                },
                Res::Buffer(bufkey) => {
                    if let Some(buf) = resources.get_buffer_mut(*bufkey){
                        //Take the guard and schedule the wait
                        buf.guard.take()
                    }else{
                        #[cfg(feature="logging")]
                        log::error!("Failed to get buffer for key {:?} while requesting resource.", bufkey);
                        return Err(SchedulerError::CouldNotFindResource(*res));
                    }
                }
            };

            //Now add the resource to our schedule by adding the guard as execution dependency of this track,
            // by scheduling release from the current owner, and acquiring on the now owner
            //
            // If no guard was found we actually don't need to release, only acquire. This can happen if a resource
            // was created for instance.
            if let Some(guard) = guard{
                let src_track_index = self.track_id_to_index(guard.track);
                let dst_track_index = track;

                #[cfg(feature="logging")]
                log::info!("Acquiring res {:?} externally from index {} to {}", res, src_track_index, dst_track_index);


                let src_track_id = self.tracks[src_track_index].track_id;
                let dst_track_id = self.tracks[dst_track_index].track_id;

                assert!(guard.target_val <= self.tracks[src_track_index].sem_start, "Guard does not expire before this schedule starts. This is a bug!");

                //make sure the first block waits at least for the given semaphore value
                self.tracks[src_track_index].cmd_frames.first_mut().unwrap().add_outside_dep(guard.target_val);
                //add release to "us"
                self.tracks[src_track_index].cmd_frames.first_mut().unwrap().release.push((dst_track_id, *res));

                //acquire on "us"
                self.tracks[dst_track_index].cmd_frames.last_mut().unwrap().acquires.push((src_track_id, *res));


                //Note that the release operations are always added to the first frame of the track. We therefore, if needed seperate that frame.
                // This means that usually there will be some kind of *release only* frame at the start of each track.
                if self.tracks[src_track_index].current_frame() == 0{
                    #[cfg(feature="logging")]
                    log::info!("Finish frame {} with num acquires: {}", 0, self.tracks[src_track_index].cmd_frames[0].acquires.len());
                    self.tracks[src_track_index].finish_frame();
                }

            }else{
                #[cfg(feature="logging")]
                log::info!("First time seeing {:?} in a graph, only acquiring!", res);

                let dst_track_index = track;
                //initialise on "us", from *nothing*.
                self.tracks[dst_track_index].cmd_frames.last_mut().unwrap().initialize.push(*res);
            }
        }


        //TODO: remove redundant transfers. This happens for each frame where the acquire or release as identical src and dst queue ids

        Ok(())
    }


    ///Translates the general id to the scheduler local index into the `tracks` field.
    fn track_id_to_index(&self, id: TrackId) -> usize{
        self.tracks.iter().enumerate().filter_map(|(idx, track)| if track.track_id == id{Some(idx)}else{None}).next().unwrap()
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
        //TODO:
        //      - Calculate the correct semaphore values based on the initial semaphore start, frames position on track, and possible set outside
        //        values of inner frames of each frame.
        //      - Distribute Guards to actual resources for inter-schedule dependency handling
        //

        todo!("Execute unimplemented")
    }

    pub fn set_present_image(&mut self, attachment: &str){
        todo!("Present unimplenmented")
    }
}
