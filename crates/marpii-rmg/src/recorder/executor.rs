use fxhash::FxHashMap;
use marpii::ash::vk;

use crate::{track::TrackId, RecordError, Rmg};

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
}

impl<'rmg> Executor<'rmg> {
    pub fn exec(rmg: &mut Rmg, schedule: Schedule<'rmg>) -> Result<(), RecordError> {
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
        };

        //now we start the actual recording / submission process. Since we give the tasks access to the actual resources (not
        // just the keys) we have to do that in order. Luckily we've written a submission/recording list while scheduling. So we can just
        // iterator over this.
        for sub in submission_order {
            exec.record_frame(rmg, sub)?;
        }

        Ok(())
    }

    fn record_frame(&mut self, rmg: &mut Rmg, frame: SubmitFrame) -> Result<(), RecordError> {
        //get us a new command buffer for the track
        let mut cb = rmg
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
        unsafe{

            self.image_barrier_buffer.clear();
            self.tracks.get(&frame.track).unwrap().record.frames[frame.frame].image_acquire_barriers(rmg, &mut self.image_barrier_buffer);

            self.buffer_barrier_buffer.clear();
            self.tracks.get(&frame.track).unwrap().record.frames[frame.frame].buffer_acquire_barriers(rmg, &mut self.buffer_barrier_buffer);

            rmg.ctx.device.inner.cmd_pipeline_barrier2(
                cb.inner,
                &vk::DependencyInfo::builder()
                    .image_memory_barriers(&self.image_barrier_buffer)
                    .buffer_memory_barriers(&self.buffer_barrier_buffer)
            );
        }

        //now all buffers/images are owned by this track. We therefore only have to
        for task in self.tracks.get(&frame.track).unwrap().record.frames[frame.frame].tasks.iter(){
            //TODO:
            //add task
            //add post task barrier
        }


        //TODO: update guard on each resource, debug assert owner
        todo!("submit to track's queue");

        Ok(())
    }
}
