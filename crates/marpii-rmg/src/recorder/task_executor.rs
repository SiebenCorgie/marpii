use std::sync::Barrier;

use ahash::AHashMap;

use crate::{recorder::task_scheduler::DepPart, track::TrackId, RecordError, Rmg, resources::res_states::AnyResKey};

use super::task_scheduler::TaskSchedule;

///Schedule executor. Takes Frames, dependencies and dependees to build an
/// command buffer that is immediately pushed to the GPU.
pub struct Executor<'t> {
    schedule: TaskSchedule<'t>,

    ///tracks which frame for which track should be scheduled next
    next_frame: AHashMap<TrackId, usize>,
}

impl<'t> Executor<'t> {
    pub fn execute(rmg: &mut Rmg, schedule: TaskSchedule<'t>) -> Result<(), RecordError> {
        let next_frame = schedule
            .tracks
            .iter()
            .filter_map(|(trackid, track)| {
                if track.frames.len() > 0 {
                    Some((*trackid, 0))
                } else {
                    #[cfg(feature = "logging")]
                    log::info!(
                        "Ignoring track {} since there are no frames on that track.",
                        trackid
                    );
                    None
                }
            })
            .collect();

        let n_nodes = schedule.tracks.values().fold(0, |sum, track| sum + track.nodes.len());
        let mut execution_order = Vec::with_capacity(n_nodes);
        let mut exec = Executor {
            schedule,
            next_frame,
        };

        while exec.has_executable() {
            let (next_track, next_tracks_frame_index) = exec.select_next_frame()?;
            //update *next* value
            *exec.next_frame.get_mut(&next_track).unwrap() += 1;
            execution_order.push((next_track, next_tracks_frame_index));
        }

        //Add release operations for all imports
        exec.schedule_import_release_frame(rmg)?;

        //execute frames
        for (trackid, frame_id) in execution_order{
            exec.schedule_frame(trackid, frame_id)?;
        }

        Ok(())
    }

    ///Returns true as long as there are unexecuted frames.
    fn has_executable(&self) -> bool {
        for (id, next) in &self.next_frame {
            if self.schedule.tracks.get(id).unwrap().frames.len() > *next {
                return true;
            }
        }

        false
    }

    ///Returns true if the node on the given task was already scheduled.
    fn is_executed(&self, track: &TrackId, node_idx: &usize) -> bool {
        self.next_frame.get(track).unwrap() > node_idx
    }

    ///Selects the next that can be scheduled.
    fn select_next_frame(&mut self) -> Result<(TrackId, usize), RecordError> {
        //go through our tracks and check if we can find a frame where all
        // dependencies are already in flight.
        //
        // NOTE: This actually "preferres" to schedule the first track id
        //  OR   which is not really uniform.
        // TODO: It might be beneficial to use some kind of heuristic here.
        //       Maybe order by *task pressure*, or preffer tracks that haven't scheduled
        //       in a while.
        for (trackid, next_idx) in self.next_frame.iter() {
            let is_executeable = if let Some(frame) = self
                .schedule
                .tracks
                .get(trackid)
                .unwrap()
                .frames
                .get(*next_idx)
            {
                //Check if all dependencies in the frame are scheduled or on same frame
                frame.iter_indices().fold(true, |is, node_idx| {
                    //Check if node in frame is scheduleabel. Skip if we already found that it isn't.
                    if is {
                        self.schedule.tracks.get(trackid).unwrap().nodes[node_idx]
                            .dependencies
                            .iter()
                            .fold(true, |is_sch, dep| {
                                //skip if we already found that it isn't again
                                if is_sch {
                                    match &dep.participant {
                                        DepPart::Import => true,
                                        DepPart::Scheduled { track, task_idx } => {
                                            //always true if on same index and track
                                            // allows us to *peek into the future*.
                                            if track == trackid && frame.contains_idx(*task_idx) {
                                                true
                                            } else {
                                                //actually check
                                                self.is_executed(track, task_idx)
                                            }
                                        }
                                    }
                                } else {
                                    false
                                }
                            })
                    } else {
                        false
                    }
                })
            } else {
                false
            };

            if is_executeable {
                return Ok((*trackid, *next_idx));
            }
        }

        #[cfg(feature = "logging")]
        log::error!("Found no frame that can be executed! This is probably a bug.");

        Err(RecordError::DeadLock)
    }

    ///checks all import statements and adds release operations to the currently owning tracks, to make
    /// the frames acquire operation valid.
    fn schedule_import_release_frame(&mut self, rmg: &mut Rmg) -> Result<(), RecordError>{

        struct ReleaseOp{
            current_owner: TrackId,
            destination_owner: TrackId,
            res: AnyResKey,
        }

        //Collect all resources and where they have to be released to.
        let mut release_ops = Vec::new();

        for (trackid, track) in self.schedule.tracks.iter(){
            for dep in track.nodes.iter().map(|node| node.dependencies.iter()).flatten(){
                if let DepPart::Import = dep.participant{
                    //if there is a current owner, build release.
                    //
                    // There are two events where there is no owner:
                    // 1. Res is a sampler
                    // 2. Res is uninitialized. In that case the access/layout transition implicitly takes care of initializing
                    //    queue ownership.
                    if let Some(current_owner) = rmg.resources().get_current_owner(dep.dep){
                        release_ops.push(ReleaseOp{
                            current_owner: rmg.queue_idx_to_trackid(current_owner).ok_or(RecordError::Any(anyhow::anyhow!("no track for queue {}", current_owner)))?,
                            destination_owner: *trackid,
                            res: dep.dep.clone()
                        });
                    }
                }
            }
        }

        //collect all release ops into one big barrier per track
        //TODO: collect into barrier per track and execute.
        //      To schedule find latest semaphore for each resource for each track. Use that as base offset as well to
        //      setup semaphore values for the tracks.
        //
        //
        let mut barriers: AHashMap<TrackId, BarrierBuilder> = self.schedule.tracks.keys().map(|k| (k, BarrierBuilder::default())).collect();

        Ok(())
    }


    fn schedule_frame(&mut self, trackid: TrackId, frame_index: usize) -> Result<(), RecordError> {
        //- build the acquire semaphores by collecting all "first" dependencies and checking their current state.
        //  Wait for the latest semaphore each.
        //- then build transition barriers pre/post task.
        //- scheduel each task
        //- then build post execution release barriers for each dependee.

        let track = self.schedule.tracks.get(&trackid).unwrap();
        println!("Frame[{}] @ {}", frame_index, trackid);
        for i in track.frames[frame_index].iter_indices() {
            println!("    [{}] {}: ", i, track.nodes[i].task.task.name());
            for dep in &track.nodes[i].dependencies {
                println!("            {:?} -> this | {:?}", dep.participant, dep.dep);
            }
            println!("        with dependees:");
            for dependee in &track.nodes[i].dependees {
                println!("            this -> {:?} | {:?}", dependee.participant, dependee.dep);
            }
        }

        Ok(())
    }
}
