use crate::{Rmg, RmgError, BufferHandle, Task};
use marpii::{resources::Buffer, ash::vk};
use std::sync::Arc;

///Uploads a number of elements of type `T`.
///
/// A fitting buffer (`self.buffer`) is created. Note that the buffer is uninitialised
/// until the task is scheduled.
pub struct UploadBuffer<T: Copy + 'static>{
    pub buffer: BufferHandle<T>,
    src_buffer:  BufferHandle<T>,
}

impl<T: Copy + 'static> UploadBuffer<T> {
    pub fn new<'src>(rmg: &mut Rmg, data: &'src [T]) -> Result<Self, RmgError>{

        let staging = Buffer::new_staging_for_data(
            &rmg.ctx.device,
            &rmg.ctx.allocator,
            Some("Staging buffer upload"),
            data
        )?;

        let staging = rmg.res.add_buffer(Arc::new(staging))?;

        let dst_buffer = rmg.new_buffer(data.len(), None)?;

        Ok(UploadBuffer { buffer: dst_buffer, src_buffer: staging })
    }
}


impl<T: Copy + 'static> Task for UploadBuffer<T>{
    fn name(&self) -> &'static str {
        "BufferUpload"
    }

    fn register(&self, registry: &mut crate::ResourceRegistry) {
        registry.request_buffer(&self.buffer);
        registry.request_buffer(&self.src_buffer);
    }

    fn queue_flags(&self) -> marpii::ash::vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }

    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &crate::Resources,
    ) {
        //NOTE: buffer barrier is done by scheduler


        let src_access = resources.get_buffer_state(&self.src_buffer);
        let dst_access = resources.get_buffer_state(&self.buffer);

        let copy_size = src_access.buffer.desc.size.min(dst_access.buffer.desc.size);

        unsafe{
            device.inner.cmd_copy_buffer2(
                *command_buffer,
                &vk::CopyBufferInfo2::builder()
                    .src_buffer(src_access.buffer.inner)
                    .dst_buffer(dst_access.buffer.inner)
                    .regions(&[
                        *vk::BufferCopy2::builder()
                            .src_offset(0)
                            .dst_offset(0)
                            .size(copy_size)
                    ])
            );
        }
    }
}
