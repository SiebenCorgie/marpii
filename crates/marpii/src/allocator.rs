//! ## Allocator
//!
//! In Vulkan the application itself is responsible for allocating memory.
//! Most of the time however this will be done trough some external allocator.
//!
//! Since there are several MarpII provides a simple abstraction via the `Allocator` trait.
//!
//! A default implementation based on [Traverse Researche's](https://github.com/Traverse-Research/gpu-allocator) `gpu-allocator` trait is included trough the `default-allocator` feature that is enabled by default.

#[cfg(feature = "default_allocator")]
mod gpu_allocator;

mod unallocated;
use std::{ffi::c_void, ptr::NonNull};

use ash::vk::{self, MappedMemoryRange};
pub use unallocated::{UnamanagedAllocationError, UnmanagedAllocation, UnmanagedAllocator};

use crate::context::Device;

///Types of memory usage. Make sure to use GpuOnly wherever it applies to get optimal performance.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MemoryUsage {
    Unknown,
    GpuOnly,
    CpuToGpu,
    GpuToCpu,
}

///Implemented for all managed allocations. Allows the [Image](crate::resources::Image) and [Buffer](crate::resources::Buffer) implementations to hide their allocator type.
pub trait AnonymAllocation {
    fn mapped_ptr(&self) -> Option<NonNull<c_void>>;
    fn as_memory_range(&self) -> Option<MappedMemoryRange>;
    ///Returns the memory as host readable slice, or none if this is not possible.
    fn as_slice_ref(&self) -> Option<&[u8]>;
    ///Returns the memory as host read/write-able slice, or none if this is not possible.
    fn as_slice_mut(&mut self) -> Option<&mut [u8]>;
    ///Returns the memory properties of the allocation
    fn memory_properties(&self) -> Option<vk::MemoryPropertyFlags>;
}

impl<A: Allocator + Send + Sync + 'static> AnonymAllocation for ManagedAllocation<A> {
    fn mapped_ptr(&self) -> Option<NonNull<c_void>> {
        if let Some(a) = &self.allocation {
            a.mapped_ptr()
        } else {
            None
        }
    }
    fn as_memory_range(&self) -> Option<MappedMemoryRange> {
        self.allocation.as_ref().map(|alo| alo.as_memory_range())
    }
    fn as_slice_ref(&self) -> Option<&[u8]> {
        if let Some(alloc) = &self.allocation {
            alloc.as_slice_ref()
        } else {
            None
        }
    }

    fn as_slice_mut(&mut self) -> Option<&mut [u8]> {
        if let Some(alloc) = &mut self.allocation {
            alloc.as_slice_mut()
        } else {
            None
        }
    }

    fn memory_properties(&self) -> Option<vk::MemoryPropertyFlags> {
        self.allocation
            .as_ref()
            .map(|alloc| alloc.memory_properties())
    }
}

///An allocation that frees itself when dropped.
pub struct ManagedAllocation<A: Allocator + Send + Sync + 'static> {
    pub allocator: std::sync::Arc<std::sync::Mutex<A>>,
    pub device: std::sync::Arc<Device>, //needed to outlive the allocator
    pub allocation: Option<<A as Allocator>::Allocation>,
}

impl<A: Allocator + Send + Sync + 'static> ManagedAllocation<A> {
    ///Returns false if the allocation is for some reason invalid, aka. shouldn't be used.
    pub fn is_valid(&self) -> bool {
        self.allocation.is_some()
    }
}

impl<A: Allocator + Send + Sync + 'static> Drop for ManagedAllocation<A> {
    fn drop(&mut self) {
        //free self
        if let (Ok(lck), Some(allocation)) = (&mut self.allocator.lock(), self.allocation.take()) {
            #[allow(unused_variables)]
            if let Err(e) = lck.free(allocation) {
                //NOTE: failed free happens "silently" as in, we don't panic. Should be fine
                //      since the allocator "knows" something is wrong and wont use the allocation anymore.
                //      The ManagedAllocation in turn becomes invalid anyways since we took the allocation.
                //TODO: Maybe we should panic on debug builds with a verbose error message?
                #[cfg(feature = "logging")]
                log::error!("Freeing allocation failed with: {}", e);
            }
        } else {
            #[cfg(feature = "logging")]
            log::warn!("Could not free managed allocation");
        }
    }
}

///Abstract allocation trait that allows finding the memory handle of an allocation, as well as its offset on that memory.
pub trait Allocation {
    fn mapped_ptr(&self) -> Option<NonNull<c_void>>;
    fn as_memory_range(&self) -> MappedMemoryRange {
        MappedMemoryRange {
            memory: self.memory(),
            offset: self.offset(),
            size: self.size(),
            ..Default::default()
        }
    }
    fn memory(&self) -> ash::vk::DeviceMemory;
    ///Returns the allocation.
    fn size(&self) -> u64;
    fn offset(&self) -> u64;
    ///Returns the memory as host readable slice, or none if this is not possible.
    fn as_slice_ref(&self) -> Option<&[u8]>;
    ///Returns the memory as host read/write-able slice, or none if this is not possible.
    fn as_slice_mut(&mut self) -> Option<&mut [u8]>;
    ///Returns the memory properties of the allocation
    fn memory_properties(&self) -> vk::MemoryPropertyFlags;
}

///Trait that can be implemented by anything that can handle allocation for a initialized [ash::Device].
pub trait Allocator {
    type Allocation: Allocation + Send + Sync + 'static;
    type AllocationError: std::error::Error + Send + Sync + 'static;
    ///creates a single allocation (possibly tagged via `name` for debugging).
    fn allocate(
        &mut self,
        name: Option<&str>,
        requirements: ash::vk::MemoryRequirements,
        usage: MemoryUsage,
        is_linear: bool,
    ) -> Result<Self::Allocation, Self::AllocationError>;

    ///Frees a allocation
    fn free(&mut self, allocation: Self::Allocation) -> Result<(), Self::AllocationError>;

    ///Allocates for a provided buffer
    fn allocate_buffer(
        &mut self,
        device: &ash::Device,
        name: Option<&str>,
        buffer: &ash::vk::Buffer,
        usage: MemoryUsage,
    ) -> Result<Self::Allocation, Self::AllocationError> {
        //By providing the buffer ans usage we have enough information to create the allocation for the buffer procedurally.

        //Get buffer's requirements
        let requirements = unsafe { device.get_buffer_memory_requirements(*buffer) };
        //create allocation
        self.allocate(name, requirements, usage, false) //NOTE: Buffers are always "linear" in memory
    }

    fn allocate_image(
        &mut self,
        device: &ash::Device,
        name: Option<&str>,
        image: &ash::vk::Image,
        usage: MemoryUsage,
        is_linear: bool,
    ) -> Result<Self::Allocation, Self::AllocationError> {
        let requirements = unsafe { device.get_image_memory_requirements(*image) };

        self.allocate(name, requirements, usage, is_linear)
    }
}
