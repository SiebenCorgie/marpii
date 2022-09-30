use ash::vk;
use gpu_allocator::{vulkan::AllocationCreateDesc, MemoryLocation};

use super::{Allocation, MemoryUsage};

pub fn memory_usage_to_location(usage: MemoryUsage) -> MemoryLocation {
    match usage {
        MemoryUsage::CpuToGpu => MemoryLocation::CpuToGpu,
        MemoryUsage::GpuOnly => MemoryLocation::GpuOnly,
        MemoryUsage::GpuToCpu => MemoryLocation::GpuToCpu,
        MemoryUsage::Unknown => MemoryLocation::Unknown,
    }
}

impl Allocation for gpu_allocator::vulkan::Allocation {
    fn mapped_ptr(&self) -> Option<std::ptr::NonNull<std::ffi::c_void>> {
        self.mapped_ptr()
    }
    fn memory(&self) -> ash::vk::DeviceMemory {
        unsafe { self.memory() }
    }
    fn size(&self) -> u64 {
        self.size()
    }
    fn offset(&self) -> u64 {
        self.offset()
    }

    fn as_slice_ref(&self) -> Option<&[u8]> {
        self.mapped_slice()
    }

    fn as_slice_mut(&mut self) -> Option<&mut [u8]> {
        self.mapped_slice_mut()
    }
    fn memory_properties(&self) -> vk::MemoryPropertyFlags{
        self.memory_properties()
    }
}

///Default memory allocator implementation.
impl super::Allocator for gpu_allocator::vulkan::Allocator {
    type Allocation = gpu_allocator::vulkan::Allocation;
    type AllocationError = gpu_allocator::AllocationError;

    fn allocate(
        &mut self,
        name: Option<&str>,
        requirements: ash::vk::MemoryRequirements,
        usage: MemoryUsage,
        is_linear: bool,
    ) -> Result<Self::Allocation, Self::AllocationError> {
        let alloc_desc = AllocationCreateDesc {
            linear: is_linear,
            location: memory_usage_to_location(usage),
            name: name.unwrap_or("marpii allocation"),
            requirements,
        };

        self.allocate(&alloc_desc)
    }

    ///Frees a allocation
    fn free(&mut self, allocation: Self::Allocation) -> Result<(), Self::AllocationError> {
        self.free(allocation)
    }
}
