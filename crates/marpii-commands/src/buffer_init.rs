use marpii::{
    allocator::{Allocator, MemoryUsage},
    ash::vk::{
        BufferCopy, BufferCreateFlags, BufferUsageFlags, CommandBufferLevel,
        CommandPoolCreateFlags, DeviceSize,
    },
    context::{Device, Queue},
    resources::{BufDesc, Buffer, CommandBufferAllocator, CommandPool, SharingMode},
};
use std::sync::{Arc, Mutex};

use crate::ManagedCommands;

///Creates a Gpu exclusive buffer filled with `data`.
///Returns when the buffer has finished uploading.
/// Since this can potentually be a long operation you can either use a dedicated
/// uploading pass in a graph if the upload should be scheduled better, or use something like [poll-promise](https://crates.io/crates/poll-promise) to do the upload on another thread.
pub fn buffer_from_data<A: Allocator + Send + Sync + 'static, T: marpii::bytemuck::Pod>(
    device: &Arc<Device>,
    allocator: &Arc<Mutex<A>>,
    upload_queue: &Queue,
    buffer_usage: BufferUsageFlags,
    name: Option<&str>,
    create_flags: Option<BufferCreateFlags>,
    data: &[T],
) -> Result<Buffer, anyhow::Error> {
    //TODO:  Do we need alignment padding? But usually we can start at 0 can't we?
    //FIXME: Check that out. Until now it worked... If it didn't also fix the upload helper passes.
    let buffer_size = core::mem::size_of::<T>() * data.len();

    //build the buffer description, as well as the staging buffer. Map data to staging buffer, then upload
    let desc = BufDesc {
        sharing: SharingMode::Exclusive,
        size: buffer_size as DeviceSize,
        usage: buffer_usage | BufferUsageFlags::TRANSFER_DST, //make sure copy works
    };

    let buffer = Buffer::new(
        device,
        allocator,
        desc,
        MemoryUsage::GpuOnly,
        name,
        create_flags,
    )?;

    let transfer_buffer =
        Buffer::new_staging_for_data(device, allocator, Some("StagingBuffer"), data)?;

    let command_pool = Arc::new(CommandPool::new(
        device,
        upload_queue.family_index,
        CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
    )?);
    let command_buffer = command_pool.allocate_buffer(CommandBufferLevel::PRIMARY)?;
    //Now launch command buffer that uploads the data
    let mut cb = ManagedCommands::new(device, command_buffer)?;
    let mut recorder = cb.start_recording()?;

    let buffer_hdl = buffer.inner;
    recorder.record(move |device, cmd| unsafe {
        device.cmd_copy_buffer(
            *cmd,
            transfer_buffer.inner,
            buffer_hdl,
            &[BufferCopy {
                dst_offset: 0,
                src_offset: 0,
                size: buffer_size as DeviceSize,
            }],
        );
    });

    recorder.finish_recording()?;

    cb.submit(device, upload_queue, &[], &[])?;
    cb.wait()?;

    Ok(buffer)
}
