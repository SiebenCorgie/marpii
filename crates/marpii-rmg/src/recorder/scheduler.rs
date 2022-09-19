use fxhash::FxHashMap;
use std::fmt::Display;

use crate::{
    resources::res_states::AnyResKey,
    track::TrackId,
    RecordError, Rmg, recorder::frame::{Release, Acquire, Init},
};

use super::{TaskRecord, frame::CmdFrame};

///Represents all frames for this specific track.
#[derive(Debug)]
pub(crate) struct TrackRecord<'rmg> {
    ///Latest known semaphore value of any imported resource on this track
    pub latest_outside_sync: u64,
    pub frames: Vec<CmdFrame<'rmg>>,
}

impl<'rmg> TrackRecord<'rmg> {
    fn current_frame(&self) -> usize {
        self.frames.len() - 1
    }
    fn finish_frame(&mut self) {
        self.frames.push(CmdFrame::new())
    }

    fn remove_empty_frames(&mut self) {
        //TODO make sure the indices match up...
        //self.frames.retain(|f| !f.is_empty());
    }
    //removes all acquire and release pairs where track == this_id.
    fn remove_redundant_chains(&mut self, this_id: &TrackId) {
        for frame in &mut self.frames {
            frame.acquire.retain(|ac| &ac.from.track != this_id);
            frame.release.retain(|re| &re.to.track != this_id);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct ResLocation {
    pub track: TrackId,
    pub frame: usize,
}

impl Display for ResLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ResLocation{{\n\ttrack: {},\n\tframe: {}}}",
            self.track, self.frame
        )
    }
}

///Technically something different, but is the same data, thefore just a renamed type.
///
pub(crate) type SubmitFrame = ResLocation;

pub struct Schedule<'rmg> {
    pub(crate) submission_order: Vec<SubmitFrame>,
    known_res: FxHashMap<AnyResKey, ResLocation>,
    pub(crate) tracks: FxHashMap<TrackId, TrackRecord<'rmg>>,
}

impl<'rmg> Schedule<'rmg> {
    pub(crate) fn from_tasks(
        rmg: &mut Rmg,
        records: Vec<TaskRecord<'rmg>>,
    ) -> Result<Self, RecordError> {
        //setup at least one frame per track.
        let tracks = rmg
            .tracks
            .0
            .iter()
            .map(|(id, track)| {
                (
                    *id,
                    TrackRecord {
                        latest_outside_sync: track.sem.get_value(), //NOTE: if nothing is imported, the track can start immediately
                        frames: vec![CmdFrame::new(), CmdFrame::new()], //Note, first frame is for releases that have to happen first
                    },
                )
            })
            .collect();

        //ininitialy submit all frames once, since the release operations when starting a frame are in there.
        // TODO: put those in a track specific /release header/ instead
        let initial_submission = rmg.tracks.0.iter().map(|track| SubmitFrame{
            track: *track.0,
            frame: 0
        }).collect();

        let mut schedule = Schedule {
            submission_order: initial_submission,
            known_res: FxHashMap::default(),
            tracks,
        };

        for task in records {
            schedule.schedule_task(rmg, task)?;
        }
        schedule.finish_schedule();

        //after building the base schedule, optimise the transfer operations by removing:
        // - release/acquire chain from/to same track
        // - remove empty frames

        for (track_id, track) in &mut schedule.tracks {
            track.remove_empty_frames();
            track.remove_redundant_chains(track_id);
        }

        Ok(schedule)
    }

    fn schedule_task<'a>(
        &mut self,
        rmg: &'a mut Rmg,
        task: TaskRecord<'rmg>,
    ) -> Result<(), RecordError> {
        let track_id = rmg
            .tracks
            .track_for_usage(task.task.queue_flags())
            .ok_or(RecordError::NoFittingTrack(task.task.queue_flags()))?;
        let frame_index = self.tracks.get(&track_id).unwrap().current_frame();

        let record_location = ResLocation {
            track: track_id,
            frame: frame_index,
        };

        //now move all resources to this track and add to the newest frame on this track
        for res in task.registry.any_res_iter() {
            let new_loc = self.request_res(rmg, track_id, res)?;
            assert!(new_loc == record_location); //sanity check
            debug_assert!(self.known_res.get(&res).unwrap() == &new_loc);
        }

        //Finally push frame to this index
        debug_assert!(
            self.tracks
                .get(&record_location.track)
                .unwrap()
                .current_frame()
                == record_location.frame
        );
        self.tracks.get_mut(&record_location.track).unwrap().frames[record_location.frame]
            .tasks
            .push(task);

        Ok(())
    }

    ///Requests the resource on the given track. Note that this will always be written to the *latest* frame.
    /// Returns the new location if successful.
    fn request_res<'a>(
        &mut self,
        rmg: &'a mut Rmg,
        track: TrackId,
        res: AnyResKey,
    ) -> Result<ResLocation, RecordError> {
        //Check if we know where the res is at. If so, arrange release/acquire. Otherwise we have to either import the res,
        // or if the res was just created, init it

        let dst_loc = ResLocation {
            track,
            frame: self.tracks.get(&track).unwrap().current_frame(),
        };

        if let Some(src_loc) = self.known_res.remove(&res) {
            //found, release it from current location to new one.

            //Note: if we are already on the dst_loc we don't need to acquire
            if src_loc != dst_loc {
                #[cfg(feature = "logging")]
                log::trace!("Transfer {:?}: {:?} -> {:?}", res, src_loc, dst_loc);
                // if the frame we release from is the *current*, we also end the frame.
                self.tracks.get_mut(&src_loc.track).unwrap().frames[src_loc.frame]
                    .release
                    .push(Release {
                        from: src_loc,
                        to: dst_loc,
                        res,
                    });

                self.tracks.get_mut(&dst_loc.track).unwrap().frames[dst_loc.frame]
                    .acquire
                    .push(Acquire {
                        from: src_loc,
                        to: dst_loc,
                        res,
                    });

                //if we where on the same frame, finish
                if src_loc.frame == self.tracks.get(&src_loc.track).unwrap().current_frame() {
                    #[cfg(feature = "logging")]
                    log::trace!("Finishing {:?}", src_loc);

                    self.tracks.get_mut(&src_loc.track).unwrap().finish_frame();
                    //add to submission list
                    self.submission_order.push(src_loc);
                    debug_assert!(
                        self.tracks.get(&src_loc.track).unwrap().current_frame()
                            == src_loc.frame + 1
                    );
                } else {
                    debug_assert!(
                        self.tracks.get(&src_loc.track).unwrap().current_frame() > src_loc.frame
                    )
                }

                //for sanity, if a transfer happened, the src_loc can't be the last frame on its track
                debug_assert!(
                    self.tracks.get(&src_loc.track).unwrap().current_frame() > src_loc.frame
                );
            } else {
                #[cfg(feature = "logging")]
                log::trace!("{:?} already owned by {:?}", res, src_loc);
            }
        } else {
            //check if the resource was initialised yet. If not we init on this track/frame. Otherwise we add a release op on the header
            // of the currently owning track and a acquire for us.
            if res.is_initialised(rmg) {
                #[cfg(feature = "logging")]
                log::trace!("Import res={:?}", res);

                //Note, we try to import from origin track. If there is none this a state less object like a sampler. In that case we ignore ownership
                // transfer all together
                if let Some(origin_track) = res.current_owner(rmg) {
                    //Note that we release from the current owner by pushing the release to the firs frame of
                    // the origin track.
                    #[cfg(feature = "logging")]
                    log::trace!(
                        "Importing outside from {:?} to {:?} for res={:?}",
                        origin_track,
                        track,
                        res
                    );

                    let src_loc = ResLocation {
                        track: origin_track,
                        frame: 0,
                    };

                    self.tracks.get_mut(&origin_track).unwrap().frames[0]
                        .release
                        .push(Release {
                            from: src_loc,
                            to: dst_loc,
                            res,
                        });
                    self.tracks.get_mut(&dst_loc.track).unwrap().frames[dst_loc.frame]
                        .acquire
                        .push(Acquire {
                            from: src_loc,
                            to: dst_loc,
                            res,
                        });

                    //update semaphore value on  track
                    self.tracks
                        .get_mut(&origin_track)
                        .unwrap()
                        .latest_outside_sync = self
                        .tracks
                        .get(&origin_track)
                        .unwrap()
                        .latest_outside_sync
                        .max(res.guarded_until(rmg));
                } else {
                    #[cfg(feature = "logging")]
                    log::trace!("Ignoring ownership transfer for res={:?}", res);
                }
            } else {
                //add as init
                #[cfg(feature = "logging")]
                log::trace!("Init res={:?}, seeing for first time", res);

                self.tracks.get_mut(&dst_loc.track).unwrap().frames[dst_loc.frame]
                    .init
                    .push(Init { res, to: dst_loc });
            }
        }

        //now update inner tracker. Note that the key was removed in the Some case, or never added at all in the none case above.
        self.known_res.insert(res, dst_loc);

        Ok(dst_loc)
    }

    //Adds all currently active, non-empty frames to the submission list
    fn finish_schedule(&mut self) {
        for (track_id, track) in self.tracks.iter_mut() {
            if !track.frames.last().unwrap().is_empty() {
                let frame = track.current_frame();
                track.finish_frame();
                self.submission_order.push(SubmitFrame {
                    track: *track_id,
                    frame,
                });

                #[cfg(feature = "logging")]
                log::trace!(
                    "Late Submit frame {:?}",
                    SubmitFrame {
                        track: *track_id,
                        frame
                    }
                );
            }
        }
    }

    pub(crate) fn print_schedule(&self) {
        println!("Schedule");
        println!("    Submission: {:?}\n", self.submission_order);

        for t in self.tracks.iter() {
            println!("    [{:?}]\n    {:#?}\n", t.0, t.1);
        }
    }
}
