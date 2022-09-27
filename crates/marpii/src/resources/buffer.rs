use std::{
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

use crate::{
    allocator::{Allocation, Allocator, AnonymAllocation, ManagedAllocation, MemoryUsage},
    context::Device,
};
use ash::vk::{self, DeviceSize};
use thiserror::Error;

use super::SharingMode;

#[derive(Error, Debug)]
pub enum BufferMapError {
    #[error("Supplied offset bigger then buffer")]
    OffsetTooLarge,
    #[error("Mapped buffer is partially written. {written} / {size}")]
    PartialyWritten { written: usize, size: usize },
    #[error("Buffer can not be mapped")]
    NotMapable,
}

pub struct BufDesc {
    pub size: ash::vk::DeviceSize,
    pub usage: ash::vk::BufferUsageFlags,
    pub sharing: super::SharingMode,
}

impl BufDesc {
    pub fn set_on_builder<'a>(
        &'a self,
        mut builder: ash::vk::BufferCreateInfoBuilder<'a>,
    ) -> ash::vk::BufferCreateInfoBuilder<'a> {
        builder = builder.size(self.size).usage(self.usage);

        match &self.sharing {
            super::SharingMode::Exclusive => {
                builder = builder.sharing_mode(ash::vk::SharingMode::EXCLUSIVE)
            }
            super::SharingMode::Concurrent {
                queue_family_indices,
            } => {
                builder = builder
                    .sharing_mode(ash::vk::SharingMode::CONCURRENT)
                    .queue_family_indices(queue_family_indices)
            }
        }

        builder
    }
}

///Self managing buffer that uses the allocator `A` to create the buffer, and free it when dropped.
//Note Freeing happens in `ManagedAllocation`'s implementation.
pub struct Buffer {
    pub desc: BufDesc,
    pub inner: ash::vk::Buffer,
    pub usage: MemoryUsage,
    pub device: Arc<Device>,
    //NOTE: The allocator was a generic once. However this clocks up the type system over time, as specially when
    //      Mixing different allocator types etc. Since the allocation field is only used once (on drop) to free the
    //      Memory I find it okay to use dynamic disaptch here. The benefit is a much cleaner API, and the ability to
    //      collect buffers from different allocators in one Vec<Buffer> for instance.
    pub allocation: Box<dyn AnonymAllocation + Send + Sync + 'static>,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_buffer(self.inner, None) }
    }
}

///The hash implementation is based on [Buffer](ash::vk::Buffer)'s hash.
impl Hash for Buffer {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.inner.hash(hasher)
    }
}

impl Buffer {
    ///Creates a buffer for `description` and the supplied creation-time information. Note that the actual resulting
    ///allocation can be bigger than specified. use `extend` to change the creation info before the buffer is created.
    pub fn new<A: Allocator + Send + Sync + 'static>(
        device: &Arc<Device>,
        allocator: &Arc<Mutex<A>>,
        description: BufDesc,
        usage: MemoryUsage,
        name: Option<&str>,
        create_flags: Option<ash::vk::BufferCreateFlags>,
    ) -> Result<Self, anyhow::Error> {
        let mut builder = ash::vk::BufferCreateInfo::builder();
        if let Some(flags) = create_flags {
            builder = builder.flags(flags);
        }

        builder = description.set_on_builder(builder);

        //create buffer handle
        let buffer = unsafe { device.inner.create_buffer(&builder, None)? };
        let allocation =
            allocator
                .lock()
                .unwrap()
                .allocate_buffer(&device.inner, name, &buffer, usage)?;

        //if allocation did no fail, bind memory to buffer, update the description with the actual data and return.
        unsafe {
            device
                .inner
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())?
        };

        Ok(Buffer {
            device: device.clone(),
            allocation: Box::new(ManagedAllocation {
                allocator: allocator.clone(),
                device: device.clone(),
                allocation: Some(allocation),
            }),
            usage,
            desc: description,
            inner: buffer,
        })
    }

    ///A staging buffer is a host visible, mapable buffer. Those are usually used to either copy data (from them) to the GPU, or from the GPU back to
    /// the staging buffer to read the data.
    ///
    /// Buffers created by this function are initalized to `data` and can be used as transfer source and destination. Have a look at the code for more information.
    pub fn new_staging_for_data<A: Allocator + Send + Sync + 'static, T: Copy + Sized + 'static>(
        device: &Arc<Device>,
        allocator: &Arc<Mutex<A>>,
        name: Option<&str>,
        data: &[T],
    ) -> Result<Self, anyhow::Error> {
        //TODO:  Do we need alignment padding? But usually we can start at 0 can't we?
        //FIXME: Check that out. Until now it worked... If it didn't also fix the upload helper passes.
        let buffer_size = core::mem::size_of::<T>() * data.len();
        //TODO: related to above: we go to the next atom for now
        let buffer_size =
            device.offset_to_next_higher_coherent_atom_size(buffer_size as DeviceSize);

        //build the buffer description, as well as the staging buffer. Map data to staging buffer, then upload
        let desc = BufDesc {
            sharing: SharingMode::Exclusive,
            size: buffer_size,
            usage: vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST, //make sure copy works
        };

        let mut buffer = Buffer::new(device, allocator, desc, MemoryUsage::CpuToGpu, name, None)?;
        //write data to transfer buffer
        let data: &[u8] = unsafe { core::mem::transmute::<_, _>(data) };
        buffer.write(0, data)?;
        //Make sure the data is written
        buffer.flush_range();

        Ok(buffer)
    }

    ///Writes `data` to the buffer.
    ///If `(data.len() * size_of::<T>()) > buffer.len()` only the first `buffer.len()` bytes of data are written and an error is returned.
    ///
    ///If the buffer is not mapable by the host (usually if the buffer us created with MemoryUsage::GpuOnly) nothing is
    /// written and an error is returned.
    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), BufferMapError> {
        //Check that we have a chance for mapping
        match &self.usage {
            MemoryUsage::GpuOnly | MemoryUsage::Unknown => {
                #[cfg(feature = "logging")]
                log::error!("Tried to map buffer that has usage: {:?}", self.usage);
                return Err(BufferMapError::NotMapable);
            }
            _ => {}
        }

        //Test region of write and shrink if necessary
        let write_size = if (offset + data.len()) > (self.desc.size as usize) {
            //edge case where the offset is too big, in that case the subtraction below would underflow
            if offset > (self.desc.size as usize) {
                #[cfg(feature = "logging")]
                log::error!(
                    "Supplied offset for buffer write to large. BufferSize={}, offset={}",
                    self.desc.size,
                    offset
                );
                return Err(BufferMapError::OffsetTooLarge);
            }

            (self.desc.size as usize) - offset
        } else {
            data.len()
        };

        //since we sanitized the write, try to map the pointer and write the actual slice
        if let Some(ptr) = self.allocation.as_slice_mut() {
            #[cfg(feature = "logging")]
            log::info!("writing to mapped buffer[{:?}] of size {} with offset={}, data_size={}, write_size={}", self.inner, self.desc.size, offset, data.len(), write_size);
            ptr[0..write_size].copy_from_slice(data);
        } else {
            return Err(BufferMapError::NotMapable);
        }

        if write_size < data.len() {
            Err(BufferMapError::PartialyWritten {
                written: write_size,
                size: data.len(),
            })
        } else {
            Ok(())
        }
    }

    ///Tries to flash the memory range. Does nothing if the memory is not host mappable
    pub fn flush_range(&self) {
        match &self.usage {
            MemoryUsage::GpuOnly | MemoryUsage::Unknown => {
                #[cfg(feature = "logging")]
                log::error!("Tried flush buffer that has usage: {:?}", self.usage);
                return;
            }
            _ => {}
        }

        let range = self.allocation.as_memory_range().unwrap();
        /*
            //update range's offer and size to be in Device limits
            range.offset = self
                .device
                .offset_to_next_lower_coherent_atom_size(range.offset);
            range.size = self
                .device
                .offset_to_next_higher_coherent_atom_size(range.size);
        */
        unsafe {
            if let Err(e) = self.device.inner.flush_mapped_memory_ranges(&[range]) {
                #[cfg(feature = "logging")]
                log::error!("Failed to flush memory range of mapped buffer: {}", e);
                return;
            }
        }
    }

    ///Returns (if possible) a reference to the buffers data. Note that the data might be aligned, or not even be of one type. Turning this data into actual types should probably be implemented
    /// by whoever knows the actual data layout.
    pub fn read(&self) -> Result<&[u8], BufferMapError> {
        match &self.usage {
            MemoryUsage::GpuOnly | MemoryUsage::Unknown => {
                #[cfg(feature = "logging")]
                log::error!("Tried to map buffe that has usage: {:?}", self.usage);
                return Err(BufferMapError::NotMapable);
            }
            _ => {}
        }

        if let Some(slice) = self.allocation.as_slice_ref() {
            Ok(slice)
        } else {
            Err(BufferMapError::NotMapable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_impl_all;

    #[test]
    fn impl_send_sync() {
        assert_impl_all!(Buffer: Send, Sync);
    }
}
