use std::{fmt::Display, marker::PhantomData};

use super::Allocation;

///Allocator implementation that does nothing. Any attempt at `allocate` will fail,
/// any attempt to `free` will do nothing. Is used for instance for a swapchain image, since the swapchain handles allocation itself.
pub struct UnmanagedAllocator;

pub struct UnmanagedAllocation {
    pub(crate) hidden: PhantomData<()>, //exists so that this struct cannot be created by anyone else.
}

#[derive(Clone, Copy, Debug)]
pub struct UnamanagedAllocationError;
impl Display for UnamanagedAllocationError {
    fn fmt<'a>(&self, _f: &mut std::fmt::Formatter<'a>) -> std::fmt::Result {
        Ok(())
    }
}
impl std::error::Error for UnamanagedAllocationError {}

//Those function cannot be called, since the struct cannot be created.
impl Allocation for UnmanagedAllocation {
    fn mapped_ptr(&self) -> Option<std::ptr::NonNull<std::ffi::c_void>> {
        None
    }
    fn memory(&self) -> ash::vk::DeviceMemory {
        ash::vk::DeviceMemory::null()
    }

    fn size(&self) -> u64 {
        0
    }
    fn offset(&self) -> u64 {
        0
    }

    fn as_slice_ref(&self) -> Option<&[u8]> {
        None
    }

    fn as_slice_mut(&mut self) -> Option<&mut [u8]> {
        None
    }
    fn memory_properties(&self) -> ash::vk::MemoryPropertyFlags {
        ash::vk::MemoryPropertyFlags::empty()
    }
}

impl super::Allocator for UnmanagedAllocator {
    type Allocation = UnmanagedAllocation;
    type AllocationError = UnamanagedAllocationError;

    fn allocate(
        &mut self,
        _name: Option<&str>,
        _requirements: ash::vk::MemoryRequirements,
        _usage: super::MemoryUsage,
        _is_linear: bool,
    ) -> Result<Self::Allocation, Self::AllocationError> {
        Err(UnamanagedAllocationError)
    }

    ///Frees a allocation
    fn free(&mut self, _allocation: Self::Allocation) -> Result<(), Self::AllocationError> {
        Ok(()) //free always succeeds since nothing can be allocated
    }
}
