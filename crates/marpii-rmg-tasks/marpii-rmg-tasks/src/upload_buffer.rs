use marpii::{
    ash::vk,
    resources::{BufDesc, Buffer, BufferMapError},
    MarpiiError,
};
use marpii_rmg::{BufferHandle, ResourceRegistry, Resources, Rmg, RmgError, Task};
use std::sync::Arc;

///Uploads a number of elements of type `T`.
///
/// A fitting buffer (`self.buffer`) is created. Note that the buffer is uninitialised
/// until the task is scheduled.
pub struct UploadBuffer<T: marpii::bytemuck::Pod> {
    pub buffer: BufferHandle<T>,
    src_buffer: BufferHandle<T>,
}

impl<T: marpii::bytemuck::Pod> UploadBuffer<T> {
    ///Creates a new storage buffer for the given data. If the buffer needs to be configured, for instance
    /// as vertex buffer, use [new_with_buffer](Self::new_with_buffer).
    pub fn new<'src>(rmg: &mut Rmg, data: &'src [T]) -> Result<Self, RmgError> {
        Self::new_with_buffer(rmg, data, BufDesc::storage_buffer::<T>(data.len()))
    }

    pub fn new_with_buffer<'src>(
        rmg: &mut Rmg,
        data: &'src [T],
        mut desc: BufDesc,
    ) -> Result<Self, RmgError> {
        let staging = Buffer::new_staging_for_data(
            &rmg.ctx.device,
            &rmg.ctx.allocator,
            Some("Staging buffer upload"),
            data,
        )
        .map_err(|e| MarpiiError::from(e))?;

        staging
            .flush_range()
            .map_err(|#[allow(unused_variables)] e| {
                #[cfg(feature = "logging")]
                log::error!("Flushing upload buffer failed: {}", e);
                MarpiiError::from(BufferMapError::FailedToFlush)
            })?;

        if !desc.usage.contains(vk::BufferUsageFlags::TRANSFER_DST) {
            #[cfg(feature = "logging")]
            log::warn!("Upload buffer had TRANSEFER_DST not set, adding to usage...");
            desc.usage |= vk::BufferUsageFlags::TRANSFER_DST;
        }

        let staging = rmg.import_buffer(Arc::new(staging), None, None)?;
        let dst_buffer = rmg.new_buffer_uninitialized(desc, None)?;

        Ok(UploadBuffer {
            buffer: dst_buffer,
            src_buffer: staging,
        })
    }
}

impl<T: marpii::bytemuck::Pod> Task for UploadBuffer<T> {
    fn name(&self) -> &'static str {
        "BufferUpload"
    }

    fn register(&self, registry: &mut ResourceRegistry) {
        registry
            .request_buffer(
                &self.buffer,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
            )
            .unwrap();
        registry
            .request_buffer(
                &self.src_buffer,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
            )
            .unwrap();
    }

    fn queue_flags(&self) -> marpii::ash::vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }

    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        //NOTE: buffer barrier is done by scheduler

        let src_access = resources.get_buffer_state(&self.src_buffer);
        let dst_access = resources.get_buffer_state(&self.buffer);

        let copy_size = src_access.buffer.desc.size.min(dst_access.buffer.desc.size);

        unsafe {
            device.inner.cmd_copy_buffer2(
                *command_buffer,
                &vk::CopyBufferInfo2::default()
                    .src_buffer(src_access.buffer.inner)
                    .dst_buffer(dst_access.buffer.inner)
                    .regions(&[vk::BufferCopy2::default()
                        .src_offset(0)
                        .dst_offset(0)
                        .size(copy_size)]),
            );
        }
    }
}
