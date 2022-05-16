use crate::{BufferState, StBuffer};
use marpii::{
    allocator::{Allocator, MemoryUsage},
    ash::vk,
    context::Device,
    resources::{BufDesc, Buffer, BufferMapError, SharingMode},
};
use std::sync::{Arc, Mutex};

use thiserror::Error;

use super::{AssumedState, Pass};

#[derive(Error, Debug)]
pub enum UploadPassError {
    #[error("Flushing memory range of staging buffer failed.")]
    FlushFailed,
    #[error("BufferMapping failed")]
    BufferMapFailed(#[from] BufferMapError),

    #[error("Some other error occured")]
    Other(#[from] anyhow::Error),
}

///Simple pass that creates a buffer from data that is then uploaded whenever the pass is submitted
/// to a graph.
///
/// Note that until the first submission the buffer is invalid. After the first submission the buffer will be valid.
/// Any additional submissions will not change the buffer.
///
/// The pass should be used for convenience if a single buffer is uploaded. For multiple buffers use [UploadBufferChunk](https://siebencorgie.rs/todo)
/// For a dynamically changing buffer use [DynamicBufferPass](crate::pass::DynamicBufferPass).
pub struct BufferUploadPass {
    ///Buffer rederence. Note that the buffer is undefined until first submission
    pub buffer: StBuffer,
    cpy_size: u64,
    assumed_states: [AssumedState; 2],
    //Staging buffer used for upload once. Is cleared afterwards
    staging: Option<StBuffer>,
}

impl BufferUploadPass {
    ///Creates the pass that uploads `data` to the resulting buffer. Note that the MemoryUsage will always be GpuOnly.
    /// The size is derived from `data`, `T`'s alignment and the member count. If possible use 16byte alignment for everything.
    ///
    /// on structs this means
    ///```rust
    ///#[repr(C, align(16))]
    ///struct MyStruct{
    ///    a: f32,
    ///    b: u32
    ///}
    ///```
    pub fn new<T: Copy + Sized + 'static, A: Allocator + Send + Sync + 'static>(
        device: &Arc<Device>,
        allocator: &Arc<Mutex<A>>,
        data: &[T],
        usage: vk::BufferUsageFlags,
        name: Option<&str>,
        create_flags: Option<vk::BufferCreateFlags>,
    ) -> Result<Self, UploadPassError> {
        //FIXME: check if we need to overallocate, depending on T's alignment...
        let size = core::mem::size_of::<T>() * data.len();

        let bufdesc = BufDesc {
            sharing: SharingMode::Exclusive,
            size: size as u64,
            usage: usage | vk::BufferUsageFlags::TRANSFER_DST,
        };

        let target_buffer = StBuffer::unitialized(Buffer::new(
            device,
            allocator,
            bufdesc,
            MemoryUsage::GpuOnly,
            name,
            create_flags,
        )?);

        let mut staging_buffer = Buffer::new(
            device,
            allocator,
            BufDesc {
                sharing: SharingMode::Exclusive,
                size: size as u64,
                usage: vk::BufferUsageFlags::TRANSFER_SRC,
            },
            MemoryUsage::CpuToGpu,
            None,
            None,
        )?;

        //write data for upload and flush
        staging_buffer.write(0, data)?;
        staging_buffer.flush_range();

        let staging_buffer = StBuffer::unitialized(staging_buffer);

        Ok(BufferUploadPass {
            cpy_size: size as u64,
            buffer: target_buffer.clone(),
            assumed_states: [
                AssumedState::Buffer {
                    buffer: target_buffer,
                    state: BufferState {
                        access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    },
                },
                AssumedState::Buffer {
                    buffer: staging_buffer.clone(),
                    state: BufferState {
                        access_mask: vk::AccessFlags::TRANSFER_READ,
                    },
                },
            ],
            staging: Some(staging_buffer),
        })
    }

    ///Returns true if the upload has been scheduled. Note that this does not necessarly mean that the buffer is valid.
    /// This is only the case after the scheduled upload has finished executing.
    pub fn is_uploaded(&self) -> bool {
        self.staging.is_none()
    }
}

impl Pass for BufferUploadPass {
    fn assumed_states(&self) -> &[super::AssumedState] {
        if self.staging.is_some() {
            &self.assumed_states
        } else {
            &[] //in the case of an uploaded buffer, nothing happens
        }
    }

    fn record(
        &mut self,
        command_buffer: &mut marpii_commands::Recorder,
    ) -> Result<(), anyhow::Error> {
        if let Some(staging) = self.staging.take() {
            #[cfg(feature = "logging")]
            log::info!("Scheduling buffer upload!");

            let dst_buffer = self.buffer.clone();
            let size = self.cpy_size;
            command_buffer.record(move |device, cmd| unsafe {
                device.cmd_copy_buffer(
                    *cmd,
                    staging.buffer().inner,
                    dst_buffer.buffer().inner,
                    &[vk::BufferCopy {
                        dst_offset: 0,
                        src_offset: 0,
                        size,
                    }],
                );
            });
        }

        Ok(())
    }

    fn requirements(&self) -> &'static [super::SubPassRequirement] {
        &[super::SubPassRequirement::TransferBit]
    }
}
