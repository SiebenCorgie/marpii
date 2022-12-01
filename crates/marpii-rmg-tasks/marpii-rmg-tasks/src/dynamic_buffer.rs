use marpii::{
    ash::vk,
    resources::{BufDesc, Buffer, BufferMapError},
};
use marpii_rmg::{BufferHandle, ResourceRegistry, Resources, Rmg, RmgError, Task};
use std::sync::Arc;

///Manages a buffer where the content can be changed from the CPU side
/// efficiently at runtime.
///
/// # Usage
///
/// Use the [write](Self::write) operation to schedule content change. After the task is executed
/// the content is guaranteed to have changed.
///
/// # Implementation
/// A CPU local, map-abel buffer, and a GPU local clone are created.
/// Whenever data changes it is written to the CPU local buffer immediately, and later, at execution time of this task the CPU buffer is written
/// to the GPU local clone.
///
pub struct DynamicBuffer<T: marpii::bytemuck::Pod> {
    cpu_local: Buffer,
    gpu_local: BufferHandle<T>,
    has_changed: bool,
}

impl<T: marpii::bytemuck::Pod> DynamicBuffer<T> {
    pub fn new_with_buffer(
        rmg: &mut Rmg,
        initial_data: &[T],
        description: BufDesc,
        name: Option<&str>,
    ) -> Result<Self, RmgError> {
        let description = description.with(|b| {
            b.usage |= vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::STORAGE_BUFFER
        }); //atleast transfer dst for this pass

        let mappable_buffer =
            Buffer::new_staging_for_data(&rmg.ctx.device, &rmg.ctx.allocator, None, &initial_data)?;

        let gpu_local = rmg.new_buffer_uninitialized(description, name)?;

        Ok(DynamicBuffer {
            cpu_local: mappable_buffer,
            gpu_local,
            has_changed: true,
        })
    }

    ///creates the buffer with the given `initial_data`. Note that this data also determines the size of the buffer.
    pub fn new(rmg: &mut Rmg, initial_data: &[T]) -> Result<Self, RmgError> {
        let desc = BufDesc::for_data::<T>(initial_data.len());
        Self::new_with_buffer(rmg, initial_data, desc, None)
    }

    ///Writes 'data' to the buffer, starting with `offset_element`. Returns Err(written_elements) if the
    /// buffer wasn't big enough.
    pub fn write(&mut self, data: &[T], offset_elements: usize) -> Result<(), BufferMapError> {
        let size_of_element = core::mem::size_of::<T>();
        let access_num_elements = self.buffer_handle().count();
        if access_num_elements
            .checked_sub(offset_elements)
            .unwrap_or(0)
            < data.len()
        {
            return Err(BufferMapError::OffsetTooLarge);
        }

        #[cfg(feature = "logging")]
        log::info!(
            "Write to staging buffer {:?}@{}",
            self.cpu_local.inner,
            offset_elements
        );

        self.has_changed = true;
        let data = bytemuck::cast_slice(data);
        self.cpu_local
            .write(size_of_element * offset_elements, data)?;

        Ok(())
    }

    ///Returns the buffer handle to the device local, dynamically updated buffer
    pub fn buffer_handle(&self) -> &BufferHandle<T> {
        &self.gpu_local
    }
}

impl<T: marpii::bytemuck::Pod> Task for DynamicBuffer<T> {
    fn name(&self) -> &'static str {
        "DynamicBuffer"
    }
    fn queue_flags(&self) -> marpii::ash::vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
    fn register(&self, registry: &mut ResourceRegistry) {
        if !self.has_changed {
            return;
        }
        registry
            .request_buffer(
                &self.gpu_local,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
            )
            .unwrap();
    }
    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        if !self.has_changed {
            return;
        }

        let dst_access = resources.get_buffer_state(&self.gpu_local);

        let copy_size = self.cpu_local.desc.size.min(dst_access.buffer.desc.size);

        unsafe {
            device.inner.cmd_copy_buffer2(
                *command_buffer,
                &vk::CopyBufferInfo2::builder()
                    .src_buffer(self.cpu_local.inner)
                    .dst_buffer(dst_access.buffer.inner)
                    .regions(&[*vk::BufferCopy2::builder()
                        .src_offset(0)
                        .dst_offset(0)
                        .size(copy_size)]),
            );
        }

        self.has_changed = false;
    }
}
