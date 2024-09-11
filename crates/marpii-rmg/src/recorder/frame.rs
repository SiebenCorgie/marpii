use marpii::ash::vk;

use crate::{
    resources::res_states::{AnyResKey, QueueOwnership},
    track::Guard,
    RecordError, Rmg,
};

use super::TaskRecord;

#[derive(Debug, Clone)]
pub(crate) struct Acquire {
    //The track, and frame index this acquires from
    pub(crate) from: ResLocation,
    pub(crate) to: ResLocation,
    pub(crate) res: AnyResKey,
}

#[derive(Debug, Clone)]
pub(crate) struct Init {
    pub(crate) res: AnyResKey,
    pub(crate) to: ResLocation,
}

#[derive(Debug, Clone)]
pub(crate) struct Release {
    pub(crate) from: ResLocation,
    pub(crate) to: ResLocation,
    pub(crate) res: AnyResKey,
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
    pub(crate) fn new() -> Self {
        CmdFrame {
            acquire: Vec::new(),
            init: Vec::new(),
            release: Vec::new(),
            tasks: Vec::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
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
        for (res, _from, to) in self
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
                            .ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Image(imgkey).into())
                            })?
                            .ownership
                    } {
                        QueueOwnership::Owned(owner) => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Image {} was still owned by {} while trying to acquire",
                                res,
                                rmg.queue_idx_to_trackid(owner).unwrap()
                            );
                            return Err(RecordError::AcquireRecord(res.into(), owner));
                        }
                        QueueOwnership::Released {
                            src_family,
                            dst_family,
                        } => {
                            #[cfg(feature = "logging")]
                            log::trace!("Acquire Image: {:?}", imgkey);

                            let mut img = rmg.res.images.get_mut(imgkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Image(imgkey).into())
                            })?;
                            image_barrier_buffer.push(
                                vk::ImageMemoryBarrier2::default()
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
                            #[cfg(feature = "logging")]
                            log::trace!("Init Image: {:?}", imgkey);

                            let dst_family = rmg.trackid_to_queue_idx(to.track);
                            let mut img = rmg.res.images.get_mut(imgkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Image(imgkey).into())
                            })?;
                            //is a init
                            image_barrier_buffer.push(
                                vk::ImageMemoryBarrier2::default()
                                    //.src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                                    //.dst_queue_family_index(dst_family)
                                    .subresource_range(img.image.subresource_all())
                                    .image(img.image.inner)
                                    .old_layout(img.layout)
                                    .new_layout(vk::ImageLayout::GENERAL)
                                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS) //FIXME optimise
                                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                                    .src_access_mask(
                                        vk::AccessFlags2::SHADER_READ
                                            | vk::AccessFlags2::SHADER_WRITE,
                                    )
                                    .dst_access_mask(
                                        vk::AccessFlags2::SHADER_READ
                                            | vk::AccessFlags2::SHADER_WRITE,
                                    )
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
                            .ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Buffer(bufkey).into())
                            })?
                            .ownership
                    } {
                        QueueOwnership::Owned(owner) => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Buffer {} was still owned by {} while trying to acquire",
                                res,
                                owner
                            );
                            return Err(RecordError::AcquireRecord(res.into(), owner));
                        }
                        QueueOwnership::Released {
                            src_family,
                            dst_family,
                        } => {
                            #[cfg(feature = "logging")]
                            log::trace!("Acquire Buffer: {:?}", bufkey);
                            let mut buf = rmg.res.buffer.get_mut(bufkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Buffer(bufkey).into())
                            })?;
                            buffer_barrier_buffer.push(
                                vk::BufferMemoryBarrier2::default()
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
                            #[cfg(feature = "logging")]
                            log::trace!("Init Buffer: {:?}", bufkey);

                            let dst_family = rmg.trackid_to_queue_idx(to.track);
                            let mut buf = rmg.res.buffer.get_mut(bufkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Buffer(bufkey).into())
                            })?;
                            buffer_barrier_buffer.push(
                                vk::BufferMemoryBarrier2::default()
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
                            .ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Image(*imgkey).into())
                            })?
                            .ownership
                    } {
                        QueueOwnership::Owned(owner) => {
                            let src_family = owner;
                            let dst_family = rmg.trackid_to_queue_idx(to.track);
                            debug_assert!(rmg.trackid_to_queue_idx(from.track) == owner);

                            if src_family == dst_family {
                                #[cfg(feature = "logging")]
                                log::trace!(
                                    "Ignoring release, src/dst are both {} for: {:?}",
                                    src_family,
                                    imgkey
                                );
                                continue;
                            }

                            #[cfg(feature = "logging")]
                            log::trace!("Release Image: {:?}", imgkey);

                            let mut img = rmg.res.images.get_mut(*imgkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Image(*imgkey).into())
                            })?;
                            image_barrier_buffer.push(
                                vk::ImageMemoryBarrier2::default()
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
                            return Err(RecordError::ReleaseRecord(
                                (*res).into(),
                                src_family,
                                dst_family,
                            ));
                        }
                        QueueOwnership::Uninitialized => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Image {} was not initialised while released from {} to {}",
                                res,
                                from,
                                to
                            );
                            return Err(RecordError::ReleaseUninitialised((*res).into()));
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
                            .ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Buffer(*bufkey).into())
                            })?
                            .ownership
                    } {
                        QueueOwnership::Owned(owner) => {
                            let src_family = owner;
                            let dst_family = rmg.trackid_to_queue_idx(to.track);
                            debug_assert!(rmg.trackid_to_queue_idx(from.track) == owner);

                            if src_family == dst_family {
                                #[cfg(feature = "logging")]
                                log::trace!(
                                    "Ignoring release, src/dst are both {} for: {:?}",
                                    src_family,
                                    bufkey
                                );
                                continue;
                            }

                            #[cfg(feature = "logging")]
                            log::trace!("Release Buffer: {:?}", bufkey);

                            let mut buf = rmg.res.buffer.get_mut(*bufkey).ok_or_else(|| {
                                RecordError::NoSuchResource(AnyResKey::Buffer(*bufkey).into())
                            })?;
                            buffer_barrier_buffer.push(
                                vk::BufferMemoryBarrier2::default()
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
                                "Buffer {} was already released from {} to {}",
                                res,
                                src_family,
                                dst_family
                            );
                            return Err(RecordError::ReleaseRecord(
                                (*res).into(),
                                src_family,
                                dst_family,
                            ));
                        }
                        QueueOwnership::Uninitialized => {
                            #[cfg(feature = "logging")]
                            log::error!(
                                "Image {} was not initialised while released from {} to {}",
                                res,
                                from,
                                to
                            );
                            return Err(RecordError::ReleaseUninitialised((*res).into()));
                        }
                    };
                }
                AnyResKey::Sampler(_) => {}
            }
        }

        Ok(())
    }
}
