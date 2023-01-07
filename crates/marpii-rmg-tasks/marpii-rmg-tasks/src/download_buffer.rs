use marpii::{resources::{Buffer, BufferMapError}, ash::vk};
use marpii_rmg::{BufferHandle, Guard, Rmg, ResourceError, Task};
use std::sync::Arc;



///Downloads some GPU resident buffer into CPU accessible memory.
/// Note that you can resubmit the task if you want to update the cpu accessible buffer.
/// [Self::download] will always access the most recently downloaded state.
///
/// Also note that the data is downloaded into the slice when calling [Self::download]. This can block if the gpu is still working.
///
pub struct DownloadBuffer<T: bytemuck::Pod + 'static>{
    gpu_buffer: BufferHandle<T>,
    cpu_access: Arc<Buffer>,
    cpu_access_hdl: BufferHandle<T>,
    execution_guard: Option<Guard>,
}

impl<T: bytemuck::Pod + 'static> DownloadBuffer<T>{
    pub fn new(rmg: &mut Rmg, buffer: BufferHandle<T>) -> Result<Self, ResourceError>{

        //buffer we use for download
        let cpu_access = Arc::new(Buffer::new(
            &rmg.ctx.device,
            &rmg.ctx.allocator,
            buffer.buf_desc().clone().add_usage(vk::BufferUsageFlags::TRANSFER_DST),
            marpii::allocator::MemoryUsage::GpuToCpu,
            None,
            None
        )?);
        //NOTE: using the import function to have tightest controll over creation
        let cpuhdl = rmg.import_buffer(cpu_access.clone(), None, None)?;

        Ok(DownloadBuffer { gpu_buffer: buffer, cpu_access, cpu_access_hdl: cpuhdl, execution_guard: None })
    }

    ///Downloads buffer into `dst`. Does nothing if the task wasn't scheduled yet.
    ///
    /// If successful, returns the number of elments that where downloaded
    pub fn download(&self, rmg: &mut Rmg, dst: &mut [T]) -> Result<usize, ResourceError>{
        if let Some(g) = &self.execution_guard{
            g.wait(rmg, u64::MAX)?;

            //use bytemuck to copy over
            let dta = self.cpu_access.read().map_err(|maperr| ResourceError::BufferMapError(maperr))?;
            if let Some(dta) = dta.as_slice_ref(){
                let dta_cast: &[T] = bytemuck::cast_slice(dta);
                let size = dta_cast.len().min(dst.len());
                Ok(size)
            }else{
                #[cfg(feature="logging")]
                log::error!("Can not map cpu access buffer.");
                Err(ResourceError::BufferMapError(BufferMapError::NotReadable))
            }
        }else{
            Err(ResourceError::Any(anyhow::anyhow!("Download Task not yet scheduled!")))
        }
    }
}


impl<T: bytemuck::Pod + 'static> Task for DownloadBuffer<T>{
    fn name(&self) -> &'static str {
        "DownloadBuffer"
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry.request_buffer(&self.gpu_buffer, vk::PipelineStageFlags2::TRANSFER, vk::AccessFlags2::TRANSFER_READ).unwrap();
        registry.request_buffer(&self.cpu_access_hdl, vk::PipelineStageFlags2::TRANSFER, vk::AccessFlags2::TRANSFER_WRITE).unwrap();
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
