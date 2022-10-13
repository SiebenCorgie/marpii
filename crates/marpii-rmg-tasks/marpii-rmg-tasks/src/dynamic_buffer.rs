use marpii_rmg::{BufferHandle, Rmg, RmgError, Task, Resources, ResourceRegistry};
use marpii::{
    ash::vk,
    resources::{Buffer, BufferMapError},
};
use std::sync::Arc;

///Represents one of the implemented upload strategies.
#[allow(dead_code)]
enum UploadStrategy<T: Copy + 'static> {
    DMA {
        ///local buffer we use for write
        cpy: Arc<Buffer>,
        buffer: BufferHandle<T>,
    },
    Copy {
        cpu_local: Buffer,
        gpu_local: BufferHandle<T>,
    },
}

///Manages a buffer where the content can be changed from the CPU side
/// efficiently at runtime.
///
/// # Usage
///
/// Use the [write](Self::write) operation to schedule content change. After the task is executed
/// the content is guaranteed to have changed.
///
/// # Implementation
///
/// There are two different versions how updating works.
///
/// In case there is a DMA heap (a memory heap that is HOST_VISIBLE & DEVICE_LOCAL) updates are written
/// directly to the memory.
///
/// (⌚at the moment the DMA version is not yet implemented⌚)
///
/// In case (most cases atm.) Such a thing does not exist, a CPU local, map-abel buffer, and a GPU local clone are created.
/// Whenever data changes it is written to the CPU local buffer immediately, and later, at execution time of this buffer written
/// to the GPU local clone.
///
pub struct DynamicBuffer<T: Copy + 'static> {
    strategy: UploadStrategy<T>,
    has_changed: bool,
}

impl<T: Copy + 'static> DynamicBuffer<T> {
    ///creates the buffer with the given `initial_data`. Note that this data also determines the size of the buffer.
    pub fn new(rmg: &mut Rmg, initial_data: &[T]) -> Result<Self, RmgError> {
        //to decide for an upload strategy, allocate one "CpuToGpu" buffer, and check on which heap the allocation is
        // located.
        // TODO: since we can't currently get the heap type of an allocation this is not yet possible.
        let mappable_buffer =
            Buffer::new_staging_for_data(&rmg.ctx.device, &rmg.ctx.allocator, None, &initial_data)?;
        let strategy = {
            if mappable_buffer.memory_properties().unwrap().contains(vk::MemoryPropertyFlags::DEVICE_LOCAL){
                let buffer = Arc::new(mappable_buffer);
                UploadStrategy::DMA {
                    cpy: buffer.clone(),
                    buffer: rmg.import_buffer(buffer, None, None)?
                }
            }else{
                let gpu_local = rmg.new_buffer(initial_data.len(), None)?;
                UploadStrategy::Copy { cpu_local: mappable_buffer, gpu_local }
            }

        };

        Ok(DynamicBuffer {
            strategy,
            has_changed: true,
        })
    }

    ///Writes 'data' to the buffer, starting with `offset_element`. Returns Err(written_elements) if the
    /// buffer wasn't big enough.
    pub fn write(&mut self, data: &[T], offset_elements: usize) -> Result<(), usize> {


        let size_of_element = core::mem::size_of::<T>();
        let access_num_elements = self.buffer_handle().count();
        let num_write_elements = data.len().min(
            access_num_elements
                .checked_sub(offset_elements)
                .unwrap_or(0),
        );

        if num_write_elements == 0 {
            return Err(0);
        }

        let write_access = match &self.strategy {
            UploadStrategy::Copy {
                cpu_local,
                gpu_local: _,
            } =>{
                #[cfg(feature = "logging")]
                log::info!("Write to staging buffer {:?}@{}", cpu_local.inner, offset_elements);
                cpu_local
            },
            UploadStrategy::DMA { cpy, buffer: _ } => {
                #[cfg(feature = "logging")]
                log::info!("Write to DMA buffer @{}", offset_elements);
                cpy
            }
        };

        self.has_changed = true;
        if let Err(e) = write_access.write(size_of_element * offset_elements, data) {
            match e {
                BufferMapError::PartialyWritten { written, size: _ } => {
                    return Err(written / size_of_element)
                },
                _ => return Err(0),
            }
        }

        if let Err(e) = write_access.flush_range(){
            #[cfg(feature = "logging")]
            log::error!("failed to flush: {}", e);

            return Err(0);
        }

        Ok(())
    }

    ///Returns the buffer handle to the device local, dynamically updated buffer
    pub fn buffer_handle(&self) -> &BufferHandle<T> {
        match &self.strategy {
            UploadStrategy::Copy {
                cpu_local: _,
                gpu_local,
            } => gpu_local,
            UploadStrategy::DMA { cpy: _, buffer } => buffer,
        }
    }
}

impl<T: Copy + 'static> Task for DynamicBuffer<T> {
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
        match &self.strategy {
            UploadStrategy::Copy {
                cpu_local: _,
                gpu_local,
            } => registry.request_buffer(gpu_local),
            //Do not register since we already wrote.
            UploadStrategy::DMA { .. } => {},
        }
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

        if let UploadStrategy::Copy {
            cpu_local,
            gpu_local,
        } = &self.strategy
        {
            let dst_access = resources.get_buffer_state(&gpu_local);

            let copy_size = cpu_local.desc.size.min(dst_access.buffer.desc.size);

            unsafe {
                device.inner.cmd_copy_buffer2(
                    *command_buffer,
                    &vk::CopyBufferInfo2::builder()
                        .src_buffer(cpu_local.inner)
                        .dst_buffer(dst_access.buffer.inner)
                        .regions(&[*vk::BufferCopy2::builder()
                            .src_offset(0)
                            .dst_offset(0)
                            .size(copy_size)]),
                );
            }
        }

        self.has_changed = false;
    }
}
