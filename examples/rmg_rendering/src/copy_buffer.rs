use marpii::ash::vk;
use marpii_rmg::{BufferHandle, Rmg, RmgError, Task};
use shared::SimObj;

use crate::OBJECT_COUNT;

///Copies a src buffer to the next "free" buffer.
pub struct CopyToGraphicsBuffer {
    src_buffer: BufferHandle<SimObj>,
    //We are currently double buffering
    buffers: [BufferHandle<SimObj>; 2],
    next: usize,
}

impl CopyToGraphicsBuffer {
    pub fn new(rmg: &mut Rmg, src_buffer: BufferHandle<SimObj>) -> Result<Self, RmgError> {
        //Allocate the two buffers
        let tranfer_buffer = [
            rmg.new_buffer::<SimObj>(OBJECT_COUNT, Some("TransferBuffer 1"))?,
            rmg.new_buffer::<SimObj>(OBJECT_COUNT, Some("TransferBuffer 2"))?,
        ];

        Ok(CopyToGraphicsBuffer {
            src_buffer,
            buffers: tranfer_buffer,
            next: 0,
        })
    }

    pub fn next_buffer(&self) -> BufferHandle<SimObj> {
        self.buffers[self.next].clone()
    }

    pub fn last_buffer(&self) -> BufferHandle<SimObj> {
        //the *oldest* buffer
        self.buffers[(self.next + 1) % self.buffers.len()].clone()
    }
}

impl Task for CopyToGraphicsBuffer {
    fn name(&self) -> &'static str {
        "CopyToGraphics"
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_buffer(
                &self.src_buffer,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
            )
            .unwrap();
        registry
            .request_buffer(
                &self.next_buffer(),
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
            )
            .unwrap();
    }

    fn queue_flags(&self) -> marpii::ash::vk::QueueFlags {
        //we want to stay on the compute queue if possible
        vk::QueueFlags::TRANSFER | vk::QueueFlags::COMPUTE
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        //copy the minimum of both buffers, then transfer back. Note that the
        // scheduler takes care of the barriers in this case
        let src_access = resources.get_buffer_state(&self.src_buffer);
        let dst_access = resources.get_buffer_state(&self.next_buffer());

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

    fn post_execution(
        &mut self,
        _resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        //flip to next buffer
        self.next = (self.next + 1) % self.buffers.len();
        Ok(())
    }
}
