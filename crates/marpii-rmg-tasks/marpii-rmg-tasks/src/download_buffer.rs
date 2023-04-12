use marpii::{ash::vk, resources::Buffer, DeviceError, MarpiiError};
use marpii_rmg::{BufferHandle, Guard, Rmg, Task};
use std::sync::Arc;

use crate::TaskError;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("Buffer can't be read by the cpu. Therfore it can't be mapped to a pointer.")]
    BufferCPUNotReadable,
    #[error("Task was not yet scheduled, we therfore can't wait for the download to finish.")]
    TaskNotScheduled,
}

///Downloads some GPU resident buffer into CPU accessible memory.
/// Note that you can resubmit the task if you want to update the cpu accessible buffer.
/// [Self::download] will always access the most recently downloaded state.
///
/// Also note that the data is downloaded into the slice when calling [Self::download]. This can block if the gpu is still working.
///
pub struct DownloadBuffer<T: bytemuck::Pod + 'static> {
    gpu_buffer: BufferHandle<T>,
    cpu_access: Arc<Buffer>,
    cpu_access_hdl: BufferHandle<T>,
    execution_guard: Option<Guard>,
}

impl<T: bytemuck::Pod + 'static> DownloadBuffer<T> {
    pub fn new(rmg: &mut Rmg, buffer: BufferHandle<T>) -> Result<Self, TaskError<DownloadError>> {
        //buffer we use for download
        let cpu_access = Arc::new(
            Buffer::new(
                &rmg.ctx.device,
                &rmg.ctx.allocator,
                buffer
                    .buf_desc()
                    .clone()
                    .add_usage(vk::BufferUsageFlags::TRANSFER_DST),
                marpii::allocator::MemoryUsage::GpuToCpu,
                None,
            )
            .map_err(|e| TaskError::Marpii(e.into()))?,
        );
        //NOTE: using the import function to have tightest controll over creation
        let cpuhdl = rmg
            .import_buffer(cpu_access.clone(), None, None)
            .map_err(|e| TaskError::RmgError(e.into()))?;

        Ok(DownloadBuffer {
            gpu_buffer: buffer,
            cpu_access,
            cpu_access_hdl: cpuhdl,
            execution_guard: None,
        })
    }

    ///Downloads buffer into `dst`. Does nothing if the task wasn't scheduled yet.
    ///
    /// If successful, returns the number of elements that where downloaded
    pub fn download(
        &self,
        rmg: &mut Rmg,
        dst: &mut [T],
    ) -> Result<usize, TaskError<DownloadError>> {
        if let Some(g) = &self.execution_guard {
            g.wait(rmg, u64::MAX)
                .map_err(|e| TaskError::Marpii(DeviceError::from(e).into()))?;

            //use bytemuck to copy over
            let dta = self
                .cpu_access
                .read()
                .map_err(|maperr| MarpiiError::from(maperr))?;
            if let Some(dta) = dta.as_slice_ref() {
                let dta_cast: &[T] = bytemuck::cast_slice(dta);
                let size = dta_cast.len().min(dst.len());
                dst[0..size].copy_from_slice(&dta_cast[0..size]);
                Ok(size)
            } else {
                #[cfg(feature = "logging")]
                log::error!("Can not map cpu access buffer.");
                Err(TaskError::Task(DownloadError::BufferCPUNotReadable))?
            }
        } else {
            Err(TaskError::Task(DownloadError::TaskNotScheduled))
        }
    }
}

impl<T: bytemuck::Pod + 'static> Task for DownloadBuffer<T> {
    fn name(&self) -> &'static str {
        "DownloadBuffer"
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_buffer(
                &self.gpu_buffer,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
            )
            .unwrap();
        registry
            .request_buffer(
                &self.cpu_access_hdl,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
            )
            .unwrap();
    }

    fn post_execution(
        &mut self,
        resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        self.execution_guard = resources.get_buffer_state(&self.cpu_access_hdl).guard();
        Ok(())
    }

    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        let src_access = resources.get_buffer_state(&self.gpu_buffer);
        let dst_access = resources.get_buffer_state(&self.cpu_access_hdl);

        let copy_size = src_access.buffer.desc.size.min(dst_access.buffer.desc.size);

        unsafe {
            device.inner.cmd_copy_buffer2(
                *command_buffer,
                &vk::CopyBufferInfo2::builder()
                    .src_buffer(src_access.buffer.inner)
                    .dst_buffer(dst_access.buffer.inner)
                    .regions(&[*vk::BufferCopy2::builder()
                        .src_offset(0)
                        .dst_offset(0)
                        .size(copy_size)]),
            );
        }
    }
}
