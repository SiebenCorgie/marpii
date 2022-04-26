use std::{
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

use crate::allocator::{Allocation, Allocator, AnonymAllocation, ManagedAllocation, MemoryUsage};

pub struct BufDesc {
    size: ash::vk::DeviceSize,
    usage: ash::vk::BufferUsageFlags,
    sharing: super::SharingMode,
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
    //NOTE: The allocator was a generic once. However this clocks up the type system over time, as specially when
    //      Mixing different allocator types etc. Since the allocation field is only used once (on drop) to free the
    //      Memory I find it okay to use dynamic disaptch here. The benefit is a much cleaner API, and the ability to
    //      collect buffers from different allocators in one Vec<Buffer> for instance.
    pub allocaton: Box<dyn AnonymAllocation + Send + Sync + 'static>,
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
        device: &Arc<crate::context::Device>,
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
            allocaton: Box::new(ManagedAllocation {
                allocator: allocator.clone(),
                allocation: Some(allocation),
            }),
            desc: description,
            inner: buffer,
        })
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
