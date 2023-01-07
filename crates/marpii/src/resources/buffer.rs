use std::{
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, MutexGuard},
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
    #[error("Can not lock allocation for mapping")]
    NotLockable,
    #[error("Failed while flushing")]
    FailedToFlush,
    //NOTE: Not necessarly an error, but probably a problem with the user.
    #[error("Tried to write empty buffer")]
    EmptyWrite,
    #[error("Tried to read an none-readable buffer")]
    NotReadable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufDesc {
    pub size: vk::DeviceSize,
    pub usage: vk::BufferUsageFlags,
    pub sharing: SharingMode,
}

impl BufDesc {
    pub fn set_on_builder<'a>(
        &'a self,
        mut builder: vk::BufferCreateInfoBuilder<'a>,
    ) -> vk::BufferCreateInfoBuilder<'a> {
        builder = builder.size(self.size).usage(self.usage);

        match &self.sharing {
            super::SharingMode::Exclusive => {
                builder = builder.sharing_mode(vk::SharingMode::EXCLUSIVE)
            }
            super::SharingMode::Concurrent {
                queue_family_indices,
            } => {
                builder = builder
                    .sharing_mode(vk::SharingMode::CONCURRENT)
                    .queue_family_indices(queue_family_indices)
            }
        }

        builder
    }

    pub fn with(mut self, op: impl FnOnce(&mut Self)) -> Self {
        op(&mut self);
        self
    }

    pub fn for_slice<T: 'static>(slice: &[T]) -> Self {
        Self::for_data::<T>(slice.len())
    }

    pub fn add_usage(mut self, usage: vk::BufferUsageFlags) -> Self {
        self.usage |= usage;
        self
    }

    ///Creates a buffer description that could hold `size` elements of type `T`. Note that no usage is set.
    pub fn for_data<T: 'static>(size: usize) -> Self {
        let size = (core::mem::size_of::<T>() * size) as u64;
        BufDesc {
            size,
            usage: vk::BufferUsageFlags::empty(),
            sharing: SharingMode::Exclusive,
        }
    }

    ///Creates a storage buffer description that could hold `size` elements of type `T`
    pub fn storage_buffer<T: 'static>(size: usize) -> Self {
        Self::for_data::<T>(size).with(|b| b.usage = vk::BufferUsageFlags::STORAGE_BUFFER)
    }

    ///Creates a new vertex buffer description for `count` times a vertex `V`.
    pub fn vertex_buffer<V: 'static>(count: usize) -> Self {
        Self::for_data::<V>(count).with(|b| b.usage = vk::BufferUsageFlags::VERTEX_BUFFER)
    }

    ///Creates a new index buffer for `count` times an index of type `u32`.
    pub fn index_buffer_u32(count: usize) -> Self {
        Self::for_data::<u32>(count).with(|b| b.usage = vk::BufferUsageFlags::INDEX_BUFFER)
    }

    ///Creates a new index buffer for `count` times an index of type `u16`.
    pub fn index_buffer_u16(count: usize) -> Self {
        Self::for_data::<u16>(count).with(|b| b.usage = vk::BufferUsageFlags::INDEX_BUFFER)
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
    //      Memory I find it okay to use dynamic dispatch here. The benefit is a much cleaner API, and the ability to
    //      collect buffers from different allocators in one Vec<Buffer> for instance.
    pub allocation: Mutex<Box<dyn AnonymAllocation + Send + Sync + 'static>>,
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
    ///allocation can be bigger than specified.
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
            allocation: Mutex::new(Box::new(ManagedAllocation {
                allocator: allocator.clone(),
                device: device.clone(),
                allocation: Some(allocation),
            })),
            usage,
            desc: description,
            inner: buffer,
        })
    }

    ///A staging buffer is a host visible, mappable buffer. Those are usually used to either copy data (from them) to the GPU, or from the GPU back to
    /// the staging buffer to read the data.
    ///
    /// Buffers created by this function are initalized to `data` and can be used as transfer source and destination. Have a look at the code for more information.
    #[cfg(feature = "bytemuck")]
    pub fn new_staging_for_data<A: Allocator + Send + Sync + 'static, T: bytemuck::Pod>(
        device: &Arc<Device>,
        allocator: &Arc<Mutex<A>>,
        name: Option<&str>,
        data: &[T],
    ) -> Result<Self, anyhow::Error> {
        //TODO:  Do we need alignment padding? But usually we can start at 0 can't we?
        //FIXME: Check that out. Until now it worked... If it didn't also fix the upload helper passes.
        let buffer_size = core::mem::size_of::<T>() * data.len();

        //build the buffer description, as well as the staging buffer. Map data to staging buffer, then upload
        let desc = BufDesc {
            sharing: SharingMode::Exclusive,
            size: buffer_size as DeviceSize,
            usage: vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST, //make sure copy works
        };

        let buffer = Buffer::new(device, allocator, desc, MemoryUsage::CpuToGpu, name, None)?;

        let data = bytemuck::cast_slice(data);
        //write data to transfer buffer
        buffer.write(0, data)?;
        //Make sure the data is written
        buffer.flush_range()?;

        Ok(buffer)
    }

    ///Writes `data` to the buffer.
    ///If `data.len() > buffer.len()` only the first `buffer.len()` bytes of data are written and an error is returned.
    ///
    ///If the buffer is not mapable by the host (usually if the buffer us created with MemoryUsage::GpuOnly) nothing is
    /// written and an error is returned.
    ///
    /// If the buffer's allocation is currently locked (by another write or read function) an error might be returned.
    ///
    /// Hint: Use the `bytemuck` crate to create slices of bytes for data "T". Also make sure you fullfill alignment for GPUs.
    pub fn write(&self, offset: usize, data: &[u8]) -> Result<(), BufferMapError> {
        //Check that we have a chance for mapping
        match &self.usage {
            MemoryUsage::GpuOnly | MemoryUsage::Unknown => {
                #[cfg(feature = "logging")]
                log::error!("Tried to map buffer that has usage: {:?}", self.usage);
                return Err(BufferMapError::NotMapable);
            }
            _ => {}
        }

        let byte_offset = offset;
        //Test region of write and shrink if necessary
        let write_size = if (byte_offset + data.len()) > (self.desc.size as usize) {
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

            (self.desc.size as usize) - byte_offset
        } else {
            data.len()
        };

        #[cfg(feature = "logging")]
        log::info!("Using write_size={}", write_size);

        //since we sanitised the write, try to map the pointer and write the actual slice
        if let Some(slice) = self
            .allocation
            .lock()
            .map_err(|_| BufferMapError::NotLockable)?
            .as_slice_mut()
        {
            #[cfg(feature = "logging")]
            log::info!("writing to mapped buffer[{:?}] of size {} with offset={}, data_size={}, write_size={}", self.inner, self.desc.size, offset, data.len(), write_size);

            slice[offset..(offset + write_size)].copy_from_slice(&data[0..write_size]);
        } else {
            return Err(BufferMapError::NotMapable);
        }

        //check if we need to flush
        if !self
            .memory_properties()?
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
        {
            self.flush_range()?;
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
    pub fn flush_range(&self) -> Result<(), BufferMapError> {
        match &self.usage {
            MemoryUsage::GpuOnly | MemoryUsage::Unknown => {
                #[cfg(feature = "logging")]
                log::error!("Tried flush buffer that has usage: {:?}", self.usage);
                return Err(BufferMapError::NotMapable);
            }
            _ => {}
        }

        let mut range = self
            .allocation
            .lock()
            .map_err(|_| BufferMapError::NotLockable)?
            .as_memory_range()
            .unwrap();

        //update range's offset and size to be in Device limits
        range.offset = self
            .device
            .offset_to_next_lower_coherent_atom_size(range.offset);
        range.size = self
            .device
            .offset_to_next_higher_coherent_atom_size(range.size);

        #[cfg(feature = "logging")]
        log::info!(
            "Flushing {:?} in range {}..{}={}",
            self.inner,
            range.offset,
            (range.offset + range.size),
            range.size
        );
        unsafe {
            if let Err(e) = self.device.inner.flush_mapped_memory_ranges(&[range]) {
                #[cfg(feature = "logging")]
                log::error!("Failed to flush memory range of mapped buffer: {}", e);
                return Err(BufferMapError::FailedToFlush);
            }
        }
        Ok(())
    }

    ///Returns (if possible) a reference to the buffers data. Note that this lock the internal allocation until the value is dropped. Therefore while reading
    /// no write to the buffer can occure.
    pub fn read<'a>(
        &'a self,
    ) -> Result<MutexGuard<'a, Box<dyn AnonymAllocation + Send + Sync>>, BufferMapError> {
        match &self.usage {
            MemoryUsage::GpuOnly | MemoryUsage::Unknown => {
                #[cfg(feature = "logging")]
                log::error!("Tried to map buffer that has usage: {:?}", self.usage);
                return Err(BufferMapError::NotMapable);
            }
            _ => {}
        }

        let lock = self
            .allocation
            .lock()
            .map_err(|_| BufferMapError::NotLockable)?;
        Ok(lock)
    }

    pub fn memory_properties(&self) -> Result<vk::MemoryPropertyFlags, BufferMapError> {
        if let Ok(lck) = self.allocation.lock() {
            Ok(lck
                .memory_properties()
                .unwrap_or(vk::MemoryPropertyFlags::empty()))
        } else {
            Err(BufferMapError::NotLockable)
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
