use ahash::{AHashMap, AHashSet};
use marpii::ash::vk;
use marpii_commands::BarrierBuilder;

use crate::{
    recorder::task_scheduler::DepPart,
    resources::res_states::{AnyResKey, QueueOwnership},
    track::{Guard, TrackId},
    RecordError, Rmg,
};

use super::task_scheduler::{TaskSchedule, Dependency};

///Schedule executor. Takes Frames, dependencies and dependees to build an
/// command buffer that is immediately pushed to the GPU.
pub struct Executor<'t> {
    schedule: TaskSchedule<'t>,

    ///tracks which frame for which track should be scheduled next
    next_frame: AHashMap<TrackId, usize>,

    //For sync guards are often collected. This vector is used to prevent re-allocation each time.
    guard_cache: Vec<Guard>,
    submit_info_cache: Vec<vk::SemaphoreSubmitInfo>,
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

        let n_nodes = schedule
            .tracks
            .values()
            .fold(0, |sum, track| sum + track.nodes.len());
        let mut execution_order = Vec::with_capacity(n_nodes);
        let mut exec = Executor {
            schedule,
            next_frame,
            guard_cache: Vec::with_capacity(10),
            submit_info_cache: Vec::with_capacity(rmg.tracks.0.len()),
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
        for (trackid, frame_id) in execution_order {
            exec.schedule_frame(rmg, trackid, frame_id)?;
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

    ///clears and rebuilds the submitinfo cache from the current set of guards.
    fn build_submitinfo_cache(&mut self, rmg: &mut Rmg) {
        self.submit_info_cache.clear();
        //for all guards, find the biggest/latest semaphore value and build a submit info for that
        // semaphore.
        //
        // If a track isn't listed in the current guards, it won't produce a submitinfo.
        // Therefore, if there are no guards, a submit using the submit cache wont wait... which is a good thing.
        //
        let track_biggest_pairs = self.guard_cache.iter().fold(
            AHashMap::default(),
            |mut map: AHashMap<TrackId, u64>, exec_guard| {
                if let Some(val) = map.get_mut(exec_guard.as_ref()) {
                    *val = (*val).max(exec_guard.wait_value());
                } else {
                    map.insert((*exec_guard).into(), exec_guard.wait_value());
                }

                map
            },
        );
        for (track_id, sem_value) in track_biggest_pairs {
            //turn each track-value pair into a submit info
            rmg.tracks
                .0
                .get(&track_id)
                .unwrap()
                .sem
                .wait(sem_value, u64::MAX)
                .unwrap();

            self.submit_info_cache.push(
                vk::SemaphoreSubmitInfo::builder()
                    .semaphore(rmg.tracks.0.get(&track_id).unwrap().sem.inner)
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //TODO: make more percise
                    .value(sem_value)
                    .build(),
            );
        }
    }

    ///checks all import statements and adds release operations to the currently owning tracks, to make
    /// the frames acquire operation valid.
    fn schedule_import_release_frame(&mut self, rmg: &mut Rmg) -> Result<(), RecordError> {
        //clear for this pass.
        self.guard_cache.clear();

        struct ReleaseOp {
            current_owner: TrackId,
            destination_owner: TrackId,
            res: AnyResKey,
        }

        //Collect all resources and where they have to be released to.
        let mut release_ops = Vec::new();

        for (trackid, track) in self.schedule.tracks.iter() {
            let track_family = rmg.trackid_to_queue_idx(*trackid);
            for dep in track
                .nodes
                .iter()
                .map(|node| node.dependencies.iter())
                .flatten()
            {
                if let DepPart::Import = dep.participant {
                    //if there is a current owner, build release.
                    //
                    // There are two events where there is no owner:
                    // 1. Res is a sampler
                    // 2. Res is uninitialised. In that case the access/layout transition implicitly takes care of initialising
                    //    queue ownership.
                    if let Some(current_owner) = rmg.resources().get_current_owner(dep.dep) {
                        //Do not have to acquire if it is already on the same track/queue_family
                        if current_owner == track_family {
                            #[cfg(feature = "logging")]
                            log::trace!(
                                "Resource {} already owned on {} at import",
                                dep.dep,
                                current_owner
                            );
                            continue;
                        }

                        #[cfg(feature = "logging")]
                        log::trace!(
                            "Releasing {} from {}",
                            dep.dep,
                            rmg.queue_idx_to_trackid(current_owner).unwrap()
                        );

                        release_ops.push(ReleaseOp {
                            current_owner: rmg.queue_idx_to_trackid(current_owner).ok_or(
                                RecordError::Any(anyhow::anyhow!(
                                    "no track for queue {}",
                                    current_owner
                                )),
                            )?,
                            destination_owner: *trackid,
                            res: dep.dep.clone(),
                        });
                    } else {
                        #[cfg(feature = "logging")]
                        log::trace!("{} not yet owned, not releasing", dep.dep);
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
        let mut barriers: AHashMap<TrackId, BarrierBuilder> = self
            .schedule
            .tracks
            .keys()
            .map(|k| (*k, BarrierBuilder::default()))
            .collect();

        for op in &release_ops {
            let barrier_builder = barriers.get_mut(&op.current_owner).unwrap();
            let src_family = rmg.trackid_to_queue_idx(op.current_owner);
            let dst_family = rmg.trackid_to_queue_idx(op.destination_owner);
            match &op.res {
                AnyResKey::Buffer(buf) => {
                    let state = rmg
                        .resources_mut()
                        .buffer
                        .get_mut(*buf)
                        .ok_or(RecordError::NoSuchResource(buf.into()))?;

                    barrier_builder.buffer_queue_transition(
                        state.buffer.inner,
                        0,
                        vk::WHOLE_SIZE,
                        src_family,
                        dst_family,
                    );
                    //flag internally and setup guard for this execution. If there is already a guard, push that into the
                    // cache for later submit building.
                    state.ownership = QueueOwnership::Released {
                        src_family,
                        dst_family,
                    };
                    if let Some(guard) = state.guard.take() {
                        self.guard_cache.push(guard);
                    }
                }
                AnyResKey::Image(img) => {
                    let state = rmg
                        .resources_mut()
                        .images
                        .get_mut(*img)
                        .ok_or(RecordError::NoSuchResource(img.into()))?;
                    barrier_builder.image_queue_transition(
                        state.image.inner,
                        state.image.subresource_all(),
                        src_family,
                        dst_family,
                    );
                    //flag internally and setup guard for this execution. If there is already a guard, push that into the
                    // cache for later submit building.
                    state.ownership = QueueOwnership::Released {
                        src_family,
                        dst_family,
                    };
                    if let Some(guard) = state.guard.take() {
                        self.guard_cache.push(guard);
                    }
                }
                AnyResKey::Sampler(_) => {} //has no ownership
            }
        }

        #[cfg(feature = "logging")]
        log::error!("UNTESTED: is release correct?");

        println!("Release barriers");
        for (track, barrier) in &barriers {
            println!("    [{}]", track);
            println!("    {:?}", barrier);
        }

        //build submit infos
        self.build_submitinfo_cache(rmg);

        //now setup semaphore values for each frame. Depending on if there is a release on that track
        // or not it might change by 1.
        for (trackid, track) in self.schedule.tracks.iter() {
            //schedule on sem val and execute release barrier immediately, move semval up once.
            if barriers.get(trackid).unwrap().has_barrier() {
                //allocate submission guard.
                let release_guard = rmg.tracks.0.get_mut(trackid).unwrap().next_guard();
                //set execution guard for each resource in a release op that is used.
                for op in release_ops.iter().filter(|op| &op.current_owner == trackid) {
                    match op.res {
                        AnyResKey::Buffer(buf) => {
                            let guard_field =
                                &mut rmg.resources_mut().buffer.get_mut(buf).unwrap().guard;
                            assert!(
                                guard_field.is_none(),
                                "Resource had guard, therefore wait was scheduled wrong"
                            );
                            *guard_field = Some(release_guard.clone());
                        }
                        AnyResKey::Image(img) => {
                            let guard_field =
                                &mut rmg.resources_mut().images.get_mut(img).unwrap().guard;
                            assert!(
                                guard_field.is_none(),
                                "Resource had guard, therefore wait was scheduled wrong"
                            );
                            *guard_field = Some(release_guard.clone());
                        }
                        AnyResKey::Sampler(_) => {}
                    }
                }

                let cb = rmg
                    .tracks
                    .0
                    .get_mut(trackid)
                    .unwrap()
                    .new_command_buffer()?;
                let dependency_info = barriers.get(trackid).unwrap().as_dependency_info();
                unsafe {
                    rmg.ctx.device.inner.begin_command_buffer(
                        cb.inner,
                        &vk::CommandBufferBeginInfo::builder()
                            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                    )?;
                    rmg.ctx
                       .device
                       .inner
                       .cmd_pipeline_barrier2(cb.inner, &dependency_info);
                    rmg.ctx.device.inner.end_command_buffer(cb.inner)?;
                }

                //execute cb, waiting for the wait info of this track
                // NOTE: not waiting for the others, since this is essentially put at the end of the previous
                //       record.
                let queue_fam = rmg.trackid_to_queue_idx(*trackid);
                let queue = rmg
                    .ctx
                    .device
                    .get_first_queue_for_family(queue_fam)
                    .unwrap();
                assert!(queue.family_index == queue_fam);

                //signal only the created guard
                let signal_info = vk::SemaphoreSubmitInfo::builder()
                    .semaphore(rmg.tracks.0.get(release_guard.as_ref()).unwrap().sem.inner)
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .value(release_guard.wait_value());

                #[cfg(feature = "logging")]
                log::trace!("Executing release for {} on {:?}", trackid, release_guard);

                unsafe {
                    rmg.ctx.device.inner.queue_submit2(
                        *queue.inner(),
                        &[*vk::SubmitInfo2::builder()
                          .command_buffer_infos(&[
                              *vk::CommandBufferSubmitInfo::builder().command_buffer(cb.inner)
                          ])
                          .wait_semaphore_infos(&self.submit_info_cache)
                          .signal_semaphore_infos(&[*signal_info])],
                        vk::Fence::null(),
                    )?;
                }
            }
        }

        Ok(())
    }

    ///builds the import/acquire barrier for all dependencies that are not yet owned by
    /// this track.
    ///
    /// Updates the guard cache accordingly.
    fn build_import_acquire_barrier(&mut self, rmg: &mut Rmg, trackid: TrackId, frame_index: usize, exec_guard: Guard) -> Result<BarrierBuilder, RecordError>{
        let track_queue_family = rmg.trackid_to_queue_idx(trackid);

        //create acquire barrier for all imports.
        let mut barrier = BarrierBuilder::default();

        let acquire_deps = self.schedule.tracks.get(&trackid).unwrap().frames[frame_index]
            .iter_indices()
            .map(|node_idx| {
                self.schedule.tracks.get(&trackid).unwrap().nodes[node_idx]
                    .dependencies
                    .iter()
                //filter out all deps that are already on the queue. This also acts as a filter for multiple occurance
                // of any res, since the participant can only differ once per frame
                    .filter(|dep| {
                        if let DepPart::Scheduled { track, task_idx } = dep.participant{
                            track != trackid
                        }else{
                            //imports are always true
                            true
                        }
                    })
            })
            .flatten();

        for dep in acquire_deps {
            //get current state and init acquire operation.
            // this will
            // - flag resource as acquired
            // - push execution guard for resource
            // - set new execution guard
            match dep.dep {
                AnyResKey::Buffer(buf) => {
                    let bufstate = rmg.res.buffer.get_mut(buf).unwrap();
                    //update ownership,  and if needed push acquire
                    match bufstate.ownership{
                        QueueOwnership::Released { src_family, dst_family } => {
                            #[cfg(feature="logging")]
                            log::trace!("Acquire {:?} to track {:?}",  buf, trackid);
                            assert!(dst_family == track_queue_family, "Release queue family does not match {} != {}", dst_family, track_queue_family);
                            bufstate.ownership = QueueOwnership::Owned(track_queue_family);
                            barrier.buffer_queue_transition(bufstate.buffer.inner, 0, vk::WHOLE_SIZE, src_family, dst_family);
                        },
                        QueueOwnership::Uninitialized => {
                            //intit to queue
                            #[cfg(feature="logging")]
                            log::trace!("Init {:?} to track {:?}",  buf, trackid);
                            bufstate.ownership = QueueOwnership::Owned(track_queue_family)
                        },
                        QueueOwnership::Owned(_) => {
                            #[cfg(feature="logging")]
                            log::error!("Buffer[{:?}] ownership was not released to {} before acquire!", buf, track_queue_family);
                            return Err(RecordError::AcquireRecord(buf.into(), track_queue_family));
                        }
                    }

                    //update guards
                    if let Some(guard) = bufstate.guard.take(){
                        self.guard_cache.push(guard);
                    }
                    bufstate.guard = Some(exec_guard.clone());
                }
                AnyResKey::Image(img) => {
                    //same as buffer acquire
                    let imgstate = rmg.res.images.get_mut(img).unwrap();
                    //update ownership,  and if needed push acquire
                    match imgstate.ownership{
                        QueueOwnership::Released { src_family, dst_family } => {
                            #[cfg(feature="logging")]
                            log::trace!("Acquire {:?} to track {:?}",  img, trackid);
                            assert!(dst_family == track_queue_family, "Release queue family does not match {} != {}", dst_family, track_queue_family);
                            imgstate.ownership = QueueOwnership::Owned(track_queue_family);
                            barrier.image_queue_transition(imgstate.image.inner, imgstate.image.subresource_all(), src_family, dst_family);
                        },
                        QueueOwnership::Uninitialized => {
                            //intit to queue
                            #[cfg(feature="logging")]
                            log::trace!("Init {:?} to track {:?}",  img, trackid);
                            imgstate.ownership = QueueOwnership::Owned(track_queue_family)
                        },
                        QueueOwnership::Owned(_) => {
                            #[cfg(feature="logging")]
                            log::error!("Image[{:?}] ownership was not released to {} before acquire!", img, track_queue_family);
                            return Err(RecordError::AcquireRecord(img.into(), track_queue_family));
                        }
                    }

                    //update guards
                    if let Some(guard) = imgstate.guard.take(){
                        self.guard_cache.push(guard);
                    }
                    imgstate.guard = Some(exec_guard.clone());
                }
                AnyResKey::Sampler(_) => {}
            }
        }

        Ok(barrier)
    }

    fn schedule_frame(
        &mut self,
        rmg: &mut Rmg,
        trackid: TrackId,
        frame_index: usize,
    ) -> Result<(), RecordError> {
        //- build the acquire semaphores by collecting all "import" dependencies and checking their current state.
        //  Wait for the guard/owner semaphore each.
        //- then build transition barriers pre/post task, maybe merge semaphore.
        //- schedule each task
        //- then build post execution release barriers for each dependee.
        //
        // TODO: assert that each acquired res was released before.

        let track_queue_family = rmg.trackid_to_queue_idx(trackid);
        //command buffer that is recorded.
        let cb = rmg.tracks.0.get_mut(&trackid).unwrap().new_command_buffer()?;
        let exec_guard = rmg.tracks.0.get_mut(&trackid).unwrap().next_guard();
        //clear to collect this context
        self.guard_cache.clear();

        #[cfg(feature = "logging")]
        {
            let track = self.schedule.tracks.get(&trackid).unwrap();
            log::trace!("Frame[{}] @ {}", frame_index, trackid);
            for i in track.frames[frame_index].iter_indices() {
                log::trace!("    [{}] {}: ", i, track.nodes[i].task.task.name());
                for dep in &track.nodes[i].dependencies {
                    log::trace!("            {:?} -> this | {:?}", dep.participant, dep.dep);
                }
                log::trace!("        with dependees:");
                for dependee in &track.nodes[i].dependees {
                    log::trace!(
                        "            this -> {:?} | {:?}",
                        dependee.participant,
                        dependee.dep
                    );
                }
            }
        }

        //get acquire barrier and start command buffer
        let acquire_barrier = self.build_import_acquire_barrier(rmg, trackid, frame_index, exec_guard)?;
        unsafe{
            rmg.ctx.device.inner.begin_command_buffer(cb.inner, &vk::CommandBufferBeginInfo::builder().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;
            rmg.ctx.device.inner.cmd_pipeline_barrier2(cb.inner, &acquire_barrier.as_dependency_info());
        }

        //at this point all resources should be acquired. We can no schedule all nodes in this
        // frame by iteratively building diff of nodes needed layout and the current layout, building the
        // transition barriers, scheduling those, then scheduling the actual node.
        for node_idx in self.schedule.tracks.get(&trackid).unwrap().frames[frame_index].iter_indices(){

        }

        //finished scheduling all nodes. We can now release to all dependees that are not on this track.


        //finally build submission info from all guards that we collected over all submission operations.
        // and submit the cb to the track's queue.



        Ok(())
    }
}
