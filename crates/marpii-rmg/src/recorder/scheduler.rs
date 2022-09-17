use fxhash::FxHashMap;
use marpii::{ash::vk, sync::Semaphore};
use std::{fmt::Display, sync::Arc};

use crate::{
    resources::res_states::{AnyResKey, Guard, QueueOwnership},
    track::TrackId,
    RecordError, Rmg,
};

use super::TaskRecord;

#[derive(Debug)]
pub(crate) struct Acquire {
    //The track, and frame index this aquires from
    from: ResLocation,
    to: ResLocation,
    res: AnyResKey,
}

#[derive(Debug)]
pub(crate) struct Init {
    res: AnyResKey,
    to: ResLocation,
}

#[derive(Debug)]
pub(crate) struct Release {
    from: ResLocation,
    to: ResLocation,
    res: AnyResKey,
}

///A frame is a set of tasks on a certain Track that can be executed after each other without having to synchronise via
/// Semaphores in between.
#[derive(Debug)]
pub(crate) struct CmdFrame<'rmg> {
    pub acquire: Vec<Acquire>,
    pub init: Vec<Init>,
    pub release: Vec<Release>,

    pub tasks: Vec<TaskRecord<'rmg>>,
}

impl<'rmg> CmdFrame<'rmg> {
    fn new() -> Self {
        CmdFrame {
            acquire: Vec::new(),
            init: Vec::new(),
            release: Vec::new(),
            tasks: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.acquire.is_empty()
            && self.init.is_empty()
            && self.release.is_empty()
            && self.tasks.is_empty()
    }

    //TODO: - write/remove new owner to to each res
    //      -

    ///append all image acquire barriers for the frame. If there returns the execution guard that was guarding the buffer until now.
    /// This guard must be obeyed before using the image on the released to queue.
    ///
    /// # Safety
    /// The barriers (mostly the vk resource handle is not lifetime checked)
    pub unsafe fn acquire_barriers(
        &self,
        rmg: &mut Rmg,
        new_guard: Guard,
        image_barrier_buffer: &mut Vec<vk::ImageMemoryBarrier2>,
        buffer_barrier_buffer: &mut Vec<vk::BufferMemoryBarrier2>,
        guard_buffer: &mut Vec<Guard>,
    ) -> Result<(), RecordError> {
        //Add all acquire operations
        for (res, from, to) in self
            .acquire
            .iter()
            .map(|acc| (acc.res, Some(acc.from), acc.to))
            .chain(self.init.iter().map(|init| (init.res, None, init.to)))
        {
            match res {
                AnyResKey::Image(imgkey) => {
                    //Sort out one of three cases:
                    // Is owned -> error, should be released or uninit
                    // Is Released -> acquire and update ownership
                    // Is uninit -> init resource
                    match {
                        rmg.res
                            .images
                            .get_mut(imgkey)
                            .ok_or_else(|| RecordError::NoSuchResource(AnyResKey::Image(imgkey)))?
                            .ownership
                    } {
                        QueueOwnership::Owned(owner) => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Image {} was still owned by {} while trying to acquire",
                                res,
                                owner
                            );
                            return Err(RecordError::AcquireRecord(res, owner));
                        }
                        QueueOwnership::Released {
                            src_family,
                            dst_family,
                        } => {
                            let mut img = rmg.res.images.get_mut(imgkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Image(imgkey))
                            })?;
                            image_barrier_buffer.push(
                                vk::ImageMemoryBarrier2::builder()
                                    .src_queue_family_index(src_family)
                                    .dst_queue_family_index(dst_family)
                                    .subresource_range(img.image.subresource_all())
                                    .image(img.image.inner)
                                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //FIXME optimise
                                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                                    .build(),
                            );

                            //If this was guarded, push as well
                            if let Some(old_guard) = img.guard.take() {
                                guard_buffer.push(old_guard);
                            }
                            //and set new execution guard
                            img.guard = Some(new_guard);
                            img.ownership = QueueOwnership::Owned(dst_family);
                        }
                        QueueOwnership::Uninitialized => {
                            let dst_family = rmg.trackid_to_queue_idx(to.track);
                            let mut img = rmg.res.images.get_mut(imgkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Image(imgkey))
                            })?;
                            //is a init
                            image_barrier_buffer.push(
                                vk::ImageMemoryBarrier2::builder()
                                    //.src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                                    //.dst_queue_family_index(dst_family)
                                    .subresource_range(img.image.subresource_all())
                                    .image(img.image.inner)
                                    .old_layout(img.layout)
                                    .new_layout(vk::ImageLayout::GENERAL)
                                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //FIXME optimise
                                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                                    .build(),
                            );

                            //update image
                            //If this was guarded, push as well
                            if let Some(old_guard) = img.guard.take() {
                                guard_buffer.push(old_guard);
                            }
                            //and set new execution guard
                            img.guard = Some(new_guard);
                            img.layout = vk::ImageLayout::GENERAL;
                            img.ownership = QueueOwnership::Owned(dst_family);
                        }
                    };
                }
                AnyResKey::Buffer(bufkey) => {
                    //Sort out one of three cases:
                    // Is owned -> error, should be released or uninit
                    // Is Released -> acquire and update ownership
                    // Is uninit -> init resource
                    match {
                        rmg.res
                            .buffer
                            .get_mut(bufkey)
                            .ok_or_else(|| RecordError::NoSuchResource(AnyResKey::Buffer(bufkey)))?
                            .ownership
                    } {
                        QueueOwnership::Owned(owner) => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Buffer {} was still owned by {} while trying to acquire",
                                res,
                                owner
                            );
                            return Err(RecordError::AcquireRecord(res, owner));
                        }
                        QueueOwnership::Released {
                            src_family,
                            dst_family,
                        } => {
                            let mut buf = rmg.res.buffer.get_mut(bufkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Buffer(bufkey))
                            })?;
                            buffer_barrier_buffer.push(
                                vk::BufferMemoryBarrier2::builder()
                                    .src_queue_family_index(src_family)
                                    .dst_queue_family_index(dst_family)
                                    .buffer(buf.buffer.inner)
                                    .offset(0)
                                    .size(vk::WHOLE_SIZE)
                                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //FIXME optimise
                                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                                    .build(),
                            );

                            //If this was guarded, push as well
                            if let Some(old_guard) = buf.guard.take() {
                                guard_buffer.push(old_guard);
                            }
                            //and set new execution guard
                            buf.guard = Some(new_guard);
                            buf.ownership = QueueOwnership::Owned(dst_family);
                        }
                        QueueOwnership::Uninitialized => {
                            let dst_family = rmg.trackid_to_queue_idx(to.track);
                            let mut buf = rmg.res.buffer.get_mut(bufkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Buffer(bufkey))
                            })?;
                            buffer_barrier_buffer.push(
                                vk::BufferMemoryBarrier2::builder()
                                    //.src_queue_family_index(src_family)
                                    //.dst_queue_family_index(dst_family)
                                    .buffer(buf.buffer.inner)
                                    .offset(0)
                                    .size(vk::WHOLE_SIZE)
                                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //FIXME optimise
                                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                                    .build(),
                            );

                            //If this was guarded, push as well
                            if let Some(old_guard) = buf.guard.take() {
                                guard_buffer.push(old_guard);
                            }
                            //and set new execution guard
                            buf.guard = Some(new_guard);
                            buf.ownership = QueueOwnership::Owned(dst_family);
                        }
                    };
                }
                AnyResKey::Sampler(_) => {}
            }
        }

        Ok(())
    }

    ///append all release barriers for the frame.
    ///
    /// # Safety
    /// The barriers (mostly the vk resource handle is not lifetime checked)
    pub unsafe fn release_barriers(
        &self,
        rmg: &mut Rmg,
        image_barrier_buffer: &mut Vec<vk::ImageMemoryBarrier2>,
        buffer_barrier_buffer: &mut Vec<vk::BufferMemoryBarrier2>,
    ) -> Result<(), RecordError> {
        //Add all acquire operations
        for Release { from, to, res } in self.release.iter() {
            match res {
                AnyResKey::Image(imgkey) => {
                    //Sort out one of three cases:
                    // Is owned -> release
                    // Is Released -> error
                    // Is uninit -> release
                    match {
                        rmg.res
                            .images
                            .get_mut(*imgkey)
                            .ok_or_else(|| RecordError::NoSuchResource(AnyResKey::Image(*imgkey)))?
                            .ownership
                    } {
                        QueueOwnership::Owned(owner) => {
                            let src_family = owner;
                            let dst_family = rmg.trackid_to_queue_idx(to.track);
                            debug_assert!(rmg.trackid_to_queue_idx(from.track) == owner);

                            let mut img = rmg.res.images.get_mut(*imgkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Image(*imgkey))
                            })?;
                            image_barrier_buffer.push(
                                vk::ImageMemoryBarrier2::builder()
                                    .src_queue_family_index(src_family)
                                    .dst_queue_family_index(dst_family)
                                    .subresource_range(img.image.subresource_all())
                                    .image(img.image.inner)
                                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //FIXME optimise
                                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                                    .build(),
                            );

                            img.ownership = QueueOwnership::Released {
                                src_family,
                                dst_family,
                            };
                        }
                        QueueOwnership::Released {
                            src_family,
                            dst_family,
                        } => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Image {} was already released from {} to {}",
                                res,
                                from,
                                to
                            );
                            return Err(RecordError::ReleaseRecord(*res, src_family, dst_family));
                        }
                        QueueOwnership::Uninitialized => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Image {} was not initialised while released from {} to {}",
                                res,
                                from,
                                to
                            );
                            return Err(RecordError::ReleaseUninitialised(*res));
                        }
                    };
                }
                AnyResKey::Buffer(bufkey) => {
                    //Sort out one of three cases:
                    // Is owned -> release
                    // Is Released -> error
                    // Is uninit -> release
                    match {
                        rmg.res
                            .buffer
                            .get_mut(*bufkey)
                            .ok_or_else(|| RecordError::NoSuchResource(AnyResKey::Buffer(*bufkey)))?
                            .ownership
                    } {
                        QueueOwnership::Owned(owner) => {
                            let src_family = owner;
                            let dst_family = rmg.trackid_to_queue_idx(to.track);
                            debug_assert!(rmg.trackid_to_queue_idx(from.track) == owner);

                            let mut buf = rmg.res.buffer.get_mut(*bufkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Buffer(*bufkey))
                            })?;
                            buffer_barrier_buffer.push(
                                vk::BufferMemoryBarrier2::builder()
                                    .src_queue_family_index(src_family)
                                    .dst_queue_family_index(dst_family)
                                    .buffer(buf.buffer.inner)
                                    .offset(0)
                                    .size(vk::WHOLE_SIZE)
                                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //FIXME optimise
                                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                                    .build(),
                            );

                            buf.ownership = QueueOwnership::Released {
                                src_family,
                                dst_family,
                            };
                        }
                        QueueOwnership::Released {
                            src_family,
                            dst_family,
                        } => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Image {} was already released from {} to {}",
                                res,
                                from,
                                to
                            );
                            return Err(RecordError::ReleaseRecord(*res, src_family, dst_family));
                        }
                        QueueOwnership::Uninitialized => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Image {} was not initialised while released from {} to {}",
                                res,
                                from,
                                to
                            );
                            return Err(RecordError::ReleaseUninitialised(*res));
                        }
                    };
                }
                AnyResKey::Sampler(_) => {}
            }
        }

        Ok(())
    }
}

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
        self.frames.retain(|f| !f.is_empty());
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
            .map(|(id, _track)| {
                (
                    *id,
                    TrackRecord {
                        latest_outside_sync: 0, //NOTE: if nothing is imported, the track can start immediately
                        frames: vec![CmdFrame::new()],
                    },
                )
            })
            .collect();

        let mut schedule = Schedule {
            submission_order: Vec::new(),
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
                    //Note that we release from the current owner by pushing the release to the firs track
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

                    //update semaphore value on this track
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
