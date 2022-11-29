use crate::{
    recorder::frame::CmdFrame,
    resources::res_states::AnyResKey,
    track::{Guard, TrackId},
    RecordError, Rmg,
};
use fxhash::FxHashMap;
use marpii::ash::vk::{self, SemaphoreSubmitInfo};

use super::{
    scheduler::{Schedule, SubmitFrame, TrackRecord},
    Execution,
};

struct Exec<'rmg> {
    record: TrackRecord<'rmg>,
}
pub struct Executor<'rmg> {
    tracks: FxHashMap<TrackId, Exec<'rmg>>,

    ///buffer that can collect image memory buffers. Used to prevent allocating each
    /// time we collect barriers from frames.
    image_barrier_buffer: Vec<vk::ImageMemoryBarrier2>,
    buffer_barrier_buffer: Vec<vk::BufferMemoryBarrier2>,
    ///buffer that holds guards while collecting acquire barriers
    guard_buffer: Vec<Guard>,
}

impl<'rmg> Executor<'rmg> {
    pub fn exec(rmg: &mut Rmg, schedule: Schedule<'rmg>) -> Result<Vec<Execution>, RecordError> {
        //recording command buffers works by finding the latest semaphore value
        // at which we can start a track. This is generally the greatest value of any imported
        // resource's guard.
        //
        // each frame is then associated with the next value.
        //
        // Afterwards we record one command buffer per frame, with barriers in between tasks.
        // Since all images remain in general layout and and the *all* access mask we don't need to transform
        // those.
        //
        // The start of each command buffer is pre-recorded with a barrier that takes care of all acquire operations,
        // and a post-record taking care of all release operations.
        //
        // Queue submission is then done based on the former mentioned Semaphore values.

        let Schedule {
            submission_order,
            tracks,
            ..
        } = schedule;

        //first build our helper structure on which we work.
        let mut exec = Executor {
            tracks: tracks
                .into_iter()
                .map(|(k, v)| (k, Exec { record: v }))
                .collect(),
            image_barrier_buffer: Vec::with_capacity(10),
            buffer_barrier_buffer: Vec::with_capacity(10),
            guard_buffer: Vec::with_capacity(10),
        };

        let mut executions = Vec::with_capacity(submission_order.len());

        //Schedule all release headers first
        let trackids = exec.tracks.keys().map(|k| *k).collect::<Vec<_>>();
        for track in trackids.into_iter() {
            if let Some(exec) = exec.release_header_for_track(rmg, track)? {
                executions.push(exec);
            }
        }

        //now we start the actual recording / submission process. Since we give the tasks access to the actual resources (not
        // just the keys) we have to do that in order. Luckily we've written a submission/recording list while scheduling. So we can just
        // iterator over this.
        for sub in &submission_order {
            if let Some(exec) = exec.record_frame(rmg, &sub)? {
                executions.push(exec);
            }
        }

        //Finally, trigger post execution
        for sub in submission_order {
            //execute post execution step for each task of the current frame
            for task in exec.tracks.get_mut(&sub.track).unwrap().record.frames
                [sub.frame.unwrap_index()]
            .tasks
            .iter_mut()
            {
                task.task.post_execution(&mut rmg.res, &rmg.ctx)?;
            }
        }

        Ok(executions)
    }

    fn wait_info_from_guard_buffer(&self, rmg: &mut Rmg) -> Vec<SemaphoreSubmitInfo> {
        self.guard_buffer
            .iter()
            .fold(
                FxHashMap::default(),
                |mut map: FxHashMap<TrackId, u64>, exec_guard| {
                    if let Some(val) = map.get_mut(exec_guard.as_ref()) {
                        *val = (*val).max(exec_guard.wait_value());
                    } else {
                        map.insert((*exec_guard).into(), exec_guard.wait_value());
                    }

                    map
                },
            )
            .into_iter()
            .map(
                |(track_id, value)| {
                    rmg.tracks
                        .0
                        .get(&track_id)
                        .unwrap()
                        .sem
                        .wait(value, u64::MAX)
                        .unwrap();

                    vk::SemaphoreSubmitInfo::builder()
                        .semaphore(rmg.tracks.0.get(&track_id).unwrap().sem.inner)
                        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //TODO: make more percise
                        .value(value)
                        .build()
                }, //TODO :(
            )
            .collect::<Vec<_>>()
    }

    ///Schedules the release operations for the header track
    fn release_header_for_track(
        &mut self,
        rmg: &mut Rmg,
        track: TrackId,
    ) -> Result<Option<Execution>, RecordError> {
        if self
            .tracks
            .get(&track)
            .unwrap()
            .record
            .release_header
            .is_empty()
        {
            #[cfg(feature = "logging")]
            log::trace!("Skiping header release for {:?}, was empty", track);
            return Ok(None);
        }

        //create this frame's guard
        let release_end_guard = rmg.tracks.0.get_mut(&track).unwrap().next_guard();

        #[cfg(feature = "logging")]
        log::trace!("Recording release on guard {:?}", release_end_guard);

        //get us a new command buffer for the track
        let cb = rmg.tracks.0.get_mut(&track).unwrap().new_command_buffer()?;

        //issue all releases. We do this by constructing a frame and using its release function
        let mut release_frame = CmdFrame::new();
        release_frame.release = self
            .tracks
            .get(&track)
            .unwrap()
            .record
            .release_header
            .clone();

        unsafe {
            rmg.ctx.device.inner.begin_command_buffer(
                cb.inner,
                &vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }

        //As outlined we start out by building the acquire list (or not, if there is nothing to acquire/Inuit).
        // This barrier is immediately added to the cb we started above
        unsafe {
            self.image_barrier_buffer.clear();
            self.buffer_barrier_buffer.clear();
            release_frame.release_barriers(
                rmg,
                &mut self.image_barrier_buffer,
                &mut self.buffer_barrier_buffer,
            )?;

            rmg.ctx.device.inner.cmd_pipeline_barrier2(
                cb.inner,
                &vk::DependencyInfo::builder()
                    .image_memory_barriers(&self.image_barrier_buffer)
                    .buffer_memory_barriers(&self.buffer_barrier_buffer),
            );
        }

        //we want to wait for any outstanding cb that uses any of the released resources,
        // therfore fill the buffer with all guards we can find, and build the wait info
        self.guard_buffer.clear();

        //add general guard TODO: remove? all releases should be guarded by the resources if needed
        //self.guard_buffer.push(release_end_guard.guard_before());

        for rel in release_frame.release.iter() {
            let guard = match rel.res {
                AnyResKey::Image(imgkey) => rmg.res.images.get_mut(imgkey).unwrap().guard.take(),
                AnyResKey::Buffer(bufkey) => rmg.res.buffer.get_mut(bufkey).unwrap().guard.take(),
                AnyResKey::Sampler(_) => None,
            };

            if let Some(g) = guard {
                self.guard_buffer.push(g);
            }

            //and add our self as guard
            match rel.res {
                AnyResKey::Image(imgkey) => {
                    rmg.res.images.get_mut(imgkey).unwrap().guard = Some(release_end_guard)
                }
                AnyResKey::Buffer(bufkey) => {
                    rmg.res.buffer.get_mut(bufkey).unwrap().guard = Some(release_end_guard)
                }
                AnyResKey::Sampler(_) => {}
            };
        }

        let wait_info = self.wait_info_from_guard_buffer(rmg);

        let signal_semaphore = vec![vk::SemaphoreSubmitInfo::builder()
            .semaphore(
                rmg.tracks
                    .0
                    .get(release_end_guard.as_ref())
                    .unwrap()
                    .sem
                    .inner,
            )
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .value(release_end_guard.wait_value())
            .build()];

        //now end cb and submit
        unsafe {
            rmg.ctx.device.inner.end_command_buffer(cb.inner)?;
            let queue_family = rmg.trackid_to_queue_idx(track);
            let queue = rmg
                .ctx
                .device
                .get_first_queue_for_family(queue_family)
                .unwrap();
            assert!(queue.family_index == queue_family);

            rmg.ctx.device.inner.queue_submit2(
                *queue.inner(),
                &[*vk::SubmitInfo2::builder()
                    .command_buffer_infos(&[
                        *vk::CommandBufferSubmitInfo::builder().command_buffer(cb.inner)
                    ])
                    .wait_semaphore_infos(&wait_info.as_slice())
                    //Signal this tracks value uppon finish
                    .signal_semaphore_infos(&signal_semaphore)],
                vk::Fence::null(),
            )?;
        }

        Ok(Some(Execution {
            resources: Vec::new(), //FIXME: collect
            command_buffer: cb,
            guard: release_end_guard,
        }))
    }

    fn record_frame(
        &mut self,
        rmg: &mut Rmg,
        frame: &SubmitFrame,
    ) -> Result<Option<Execution>, RecordError> {
        if self.tracks.get(&frame.track).unwrap().record.frames[frame.frame.unwrap_index()]
            .is_empty()
        {
            #[cfg(feature = "logging")]
            log::trace!("Frame {:?} is empty, not recording!", frame);
            return Ok(None);
        }

        let num_res = self.tracks.get(&frame.track).unwrap().record.frames
            [frame.frame.unwrap_index()]
        .tasks
        .iter()
        .fold(0, |res, rec| res + rec.registry.resource_collection.len());
        let mut used_resources = Vec::with_capacity(num_res);

        //create this frame's guard
        let frame_end_guard = rmg.tracks.0.get_mut(&frame.track).unwrap().next_guard();

        #[cfg(feature = "logging")]
        log::trace!("Recording frame on guard {:?}", frame_end_guard);

        //get us a new command buffer for the track
        let cb = rmg
            .tracks
            .0
            .get_mut(&frame.track)
            .unwrap()
            .new_command_buffer()?;

        //clear tmp buffers
        self.guard_buffer.clear();
        self.image_barrier_buffer.clear();
        self.buffer_barrier_buffer.clear();

        //start recording
        unsafe {
            rmg.ctx.device.inner.begin_command_buffer(
                cb.inner,
                &vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            if frame.track.0.contains(vk::QueueFlags::COMPUTE) {
                #[cfg(feature = "logging")]
                log::trace!("Binding to Compute");

                rmg.ctx.device.inner.cmd_bind_descriptor_sets(
                    cb.inner,
                    vk::PipelineBindPoint::COMPUTE,
                    rmg.res.bindless_layout.layout,
                    0,
                    &rmg.res.bindless.clone_raw_descriptor_sets(),
                    &[],
                );
            }
            if frame.track.0.contains(vk::QueueFlags::GRAPHICS) {
                #[cfg(feature = "logging")]
                log::trace!("Binding to Graphics");

                rmg.ctx.device.inner.cmd_bind_descriptor_sets(
                    cb.inner,
                    vk::PipelineBindPoint::GRAPHICS,
                    rmg.res.bindless_layout.layout,
                    0,
                    &rmg.res.bindless.clone_raw_descriptor_sets(),
                    &[],
                );
            }
        }

        //As outlined we start out by building the acquire list (or not, if there is nothing to acquire/Inuit).
        // This barrier is immediately added to the cb we started above
        unsafe {
            self.tracks.get(&frame.track).unwrap().record.frames[frame.frame.unwrap_index()]
                .acquire_barriers(
                    rmg,
                    frame_end_guard,
                    &mut self.image_barrier_buffer,
                    &mut self.buffer_barrier_buffer,
                    &mut self.guard_buffer,
                )?;

            rmg.ctx.device.inner.cmd_pipeline_barrier2(
                cb.inner,
                &vk::DependencyInfo::builder()
                    .image_memory_barriers(&self.image_barrier_buffer)
                    .buffer_memory_barriers(&self.buffer_barrier_buffer),
            );
        }

        //add general guard
        // TODO remove? could be independent...
        //self.guard_buffer.push(frame_end_guard.guard_before());

        //FIXME: make fast :)
        // Finds the maximum guard value per track id. Since we have to wait at least until the last known
        let wait_info = self.wait_info_from_guard_buffer(rmg);

        #[cfg(feature = "logging")]
        log::trace!("Wait info: {:?}", wait_info);

        //now all buffers/images are owned by this track. We therefore only have to
        for task in self.tracks.get_mut(&frame.track).unwrap().record.frames
            [frame.frame.unwrap_index()]
        .tasks
        .iter_mut()
        {
            task.task.record(&rmg.ctx.device, &cb.inner, &rmg.res);
            //add execution barrier afterwards
            // TODO: make barrier handle attachment transition, and use more precise mask
            unsafe {
                rmg.ctx
                    .device
                    .inner
                    .cmd_pipeline_barrier2(cb.inner, &vk::DependencyInfo::builder());
            }
        }

        //now add the release barrier to the frame
        unsafe {
            self.image_barrier_buffer.clear();
            self.buffer_barrier_buffer.clear();
            self.tracks.get(&frame.track).unwrap().record.frames[frame.frame.unwrap_index()]
                .release_barriers(
                    rmg,
                    &mut self.image_barrier_buffer,
                    &mut self.buffer_barrier_buffer,
                )?;

            rmg.ctx.device.inner.cmd_pipeline_barrier2(
                cb.inner,
                &vk::DependencyInfo::builder()
                    .image_memory_barriers(&self.image_barrier_buffer)
                    .buffer_memory_barriers(&self.buffer_barrier_buffer),
            );
        }

        let mut signal_semaphore = vec![vk::SemaphoreSubmitInfo::builder()
            .semaphore(
                rmg.tracks
                    .0
                    .get(frame_end_guard.as_ref())
                    .unwrap()
                    .sem
                    .inner,
            )
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .value(frame_end_guard.wait_value())
            .build()];

        //if found, add all foreign semaphores
        for task in self.tracks.get(&frame.track).unwrap().record.frames[frame.frame.unwrap_index()]
            .tasks
            .iter()
        {
            task.registry
                .append_foreign_signal_semaphores(&mut signal_semaphore);
        }

        //finally, when finished recording, execute by
        unsafe {
            rmg.ctx.device.inner.end_command_buffer(cb.inner)?;

            let queue_family = rmg.trackid_to_queue_idx(frame.track);
            let queue = rmg
                .ctx
                .device
                .get_first_queue_for_family(queue_family)
                .unwrap();

            #[cfg(feature = "logging")]
            log::trace!(
                "Signal info: {:?}\nFamily: {}, index: {}",
                signal_semaphore,
                queue.family_index,
                0
            );

            assert!(queue.family_index == queue_family);

            rmg.ctx.device.inner.queue_submit2(
                *queue.inner(),
                &[*vk::SubmitInfo2::builder()
                    .command_buffer_infos(&[
                        *vk::CommandBufferSubmitInfo::builder().command_buffer(cb.inner)
                    ])
                    .wait_semaphore_infos(&wait_info)
                    //Signal this tracks value uppon finish
                    .signal_semaphore_infos(&signal_semaphore)],
                vk::Fence::null(),
            )?;
        }

        for rec in self.tracks.get_mut(&frame.track).unwrap().record.frames
            [frame.frame.unwrap_index()]
        .tasks
        .iter_mut()
        {
            used_resources.append(&mut rec.registry.resource_collection);
        }

        Ok(Some(Execution {
            resources: used_resources, //FIXME: collect
            command_buffer: cb,
            guard: frame_end_guard,
        }))
    }
}
