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
    fn memory(&self) -> ash::vk::DeviceMemory {
        unsafe { self.memory() }
    }

    fn offset(&self) -> u64 {
        self.offset()
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
