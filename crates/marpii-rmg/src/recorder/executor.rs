use crate::{
    recorder::task, resources::res_states::Guard, track::TrackId, RecordError, Rmg,
};
use fxhash::FxHashMap;
use marpii::{
    ash::vk,
    resources::{CommandBuffer, CommandPool},
};
use slotmap::SlotMap;
use std::sync::Arc;

use super::scheduler::{CmdFrame, Schedule, SubmitFrame, TrackRecord};

struct Exec<'rmg> {
    record: TrackRecord<'rmg>,
    current_frame: usize,
}

impl<'rmg> Exec<'rmg> {
    fn start_val(&self) -> u64 {
        //Is the latest known value we know on this track, plus one, since we want at to start earliest right afterwards
        self.record.latest_outside_sync + 1
    }

    fn sem_val(&self, frame_index: usize) -> u64 {
        self.start_val() + frame_index as u64
    }
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

pub struct Execution {
    ///The command buffer that is executed
    command_buffer: CommandBuffer<Arc<CommandPool>>,
    ///Until when it is guarded.
    pub(crate) guard: Guard,
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
                .map(|(k, v)| {
                    (
                        k,
                        Exec {
                            record: v,
                            current_frame: 0,
                        },
                    )
                })
                .collect(),
            image_barrier_buffer: Vec::with_capacity(10),
            buffer_barrier_buffer: Vec::with_capacity(10),
            guard_buffer: Vec::with_capacity(10),
        };

        let mut executions = Vec::with_capacity(submission_order.len());

        //now we start the actual recording / submission process. Since we give the tasks access to the actual resources (not
        // just the keys) we have to do that in order. Luckily we've written a submission/recording list while scheduling. So we can just
        // iterator over this.
        for sub in submission_order {
            executions.push(exec.record_frame(rmg, sub)?);
        }

        Ok(executions)
    }

    fn record_frame(
        &mut self,
        rmg: &mut Rmg,
        frame: SubmitFrame,
    ) -> Result<Execution, RecordError> {
        //create this frame's guard
        let guard = Guard {
            track: frame.track,
            target_value: self.tracks.get(&frame.track).unwrap().sem_val(frame.frame),
        };

        #[cfg(feature = "logging")]
        log::trace!("Recording frame on guard {:?}", guard);

        //get us a new command buffer for the track
        let cb = rmg
            .tracks
            .0
            .get_mut(&frame.track)
            .unwrap()
            .new_command_buffer()?;

        //start recording
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
            self.guard_buffer.clear();
            self.image_barrier_buffer.clear();
            self.buffer_barrier_buffer.clear();
            self.tracks.get(&frame.track).unwrap().record.frames[frame.frame].acquire_barriers(
                rmg,
                guard,
                &mut self.image_barrier_buffer,
                &mut self.buffer_barrier_buffer,
                &mut self.guard_buffer,
            );

            rmg.ctx.device.inner.cmd_pipeline_barrier2(
                cb.inner,
                &vk::DependencyInfo::builder()
                    .image_memory_barriers(&self.image_barrier_buffer)
                    .buffer_memory_barriers(&self.buffer_barrier_buffer),
            );
        }

        //FIXME: make fast :)
        let wait_semaphores = self
            .guard_buffer
            .iter()
            .fold(
                FxHashMap::default(),
                |mut map: FxHashMap<TrackId, u64>, exec_guard| {
                    if let Some(val) = map.get_mut(&exec_guard.track) {
                        *val = (*val).max(exec_guard.target_value);
                    } else {
                        map.insert(exec_guard.track, exec_guard.target_value);
                    }

                    map
                },
            )
            .into_iter()
            .map(
                |(track_id, value)| {
                    vk::SemaphoreSubmitInfo::builder()
                        .semaphore(rmg.tracks.0.get(&track_id).unwrap().sem.inner)
                        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //TODO: make more percise
                        .value(value)
                        .build()
                }, //TODO :(
            )
            .collect::<Vec<_>>();

        #[cfg(feature = "logging")]
        log::trace!("Wait info: {:?}", wait_semaphores);

        //now all buffers/images are owned by this track. We therefore only have to
        for task in self.tracks.get_mut(&frame.track).unwrap().record.frames[frame.frame]
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
            self.tracks.get(&frame.track).unwrap().record.frames[frame.frame].release_barriers(
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

        //finally, when finished recording, execute by
        unsafe {
            rmg.ctx.device.inner.end_command_buffer(cb.inner)?;

            let queue_family = rmg.trackid_to_queue_idx(frame.track);
            let queue = rmg
                .ctx
                .device
                .get_first_queue_for_family(queue_family)
                .unwrap();
            rmg.ctx.device.inner.queue_submit2(
                *queue.inner(),
                &[*vk::SubmitInfo2::builder()
                    .command_buffer_infos(&[
                        *vk::CommandBufferSubmitInfo::builder().command_buffer(cb.inner)
                    ])
                    .wait_semaphore_infos(wait_semaphores.as_slice())
                    //Signal this tracks value uppon finish
                    .signal_semaphore_infos(&[*vk::SemaphoreSubmitInfo::builder()
                        .semaphore(rmg.tracks.0.get(&frame.track).unwrap().sem.inner)
                        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .value(guard.target_value)])],
                vk::Fence::null(),
            )?;
        }
        Ok(Execution {
            command_buffer: cb,
            guard,
        })
    }
}
