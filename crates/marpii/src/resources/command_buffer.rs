use std::sync::Arc;

use crate::context::Device;

pub struct CommandPool {
    ///Device this pool was created on.
    pub device: Arc<Device>,
    ///The queue family this pool's buffers can be used on.
    pub queue_family: u32,
    ///the raw vulkan handle.
    pub inner: ash::vk::CommandPool,
    pub can_reset_buffer: bool,
}

impl CommandPool {
    pub fn new(
        device: &Arc<Device>,
        queue_family: u32,
        flags: ash::vk::CommandPoolCreateFlags,
    ) -> Result<Self, anyhow::Error> {
        let create_info = ash::vk::CommandPoolCreateInfo::builder()
            .flags(flags)
            .queue_family_index(queue_family);

        let pool = unsafe { device.inner.create_command_pool(&create_info, None)? };

        let can_reset_buffer =
            flags.contains(ash::vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

        Ok(CommandPool {
            device: device.clone(),
            inner: pool,
            queue_family,
            can_reset_buffer,
        })
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_command_pool(self.inner, None) }
    }
}

impl CommandBufferAllocator for CommandPool {
    fn reset(
        &self,
        command_buffer: &ash::vk::CommandBuffer,
        release_resources: bool,
    ) -> Result<(), anyhow::Error> {
        if self.can_reset_buffer {
            let flag = if release_resources {
                ash::vk::CommandBufferResetFlags::RELEASE_RESOURCES
            } else {
                ash::vk::CommandBufferResetFlags::empty()
            };
            unsafe {
                self.device
                    .inner
                    .reset_command_buffer(*command_buffer, flag)
            }
            .map_err(|e| e.into())
        } else {
            anyhow::bail!("CommandPool can't reset buffer")
        }
    }
    fn allocate_buffer(
        self,
        level: ash::vk::CommandBufferLevel,
    ) -> Result<CommandBuffer<Self>, anyhow::Error>
    where
        Self: Sized,
    {
        let mut buffer = unsafe {
            self.device.inner.allocate_command_buffers(
                &ash::vk::CommandBufferAllocateInfo::builder()
                    .command_pool(self.inner)
                    .command_buffer_count(1)
                    .level(level),
            )?
        };

        if buffer.len() == 0 {
            anyhow::bail!("Failed to allocate buffer!");
        }

        #[cfg(feature = "logging")]
        if buffer.len() > 1 {
            log::warn!(
                "Allocated too many command buffer, expected 1, got {}",
                buffer.len()
            )
        }

        let buffer = buffer.remove(0);

        Ok(CommandBuffer {
            pool: self,
            inner: buffer,
        })
    }

    fn device(&self) -> &ash::Device {
        &self.device.inner
    }
    fn raw(&self) -> &ash::vk::CommandPool {
        &self.inner
    }
}

///Command buffer allocation implementation.
pub trait CommandBufferAllocator {
    ///Tries to reset the command buffer. Might fail, for instance if a validation layer fails, or if the pool was not created
    /// with the `RESET` flag.
    ///
    /// `release_resources` is is synonym to [this](https://www.khronos.org/registry/vulkan/specs/1.3-extensions/man/html/VkCommandBufferResetFlagBits.html) flag.
    /// Usually this should be set to `true`. However depending on the usecase this might not lead to the best performance when re-recording on this `command_buffer`.
    fn reset(
        &self,
        command_buffer: &ash::vk::CommandBuffer,
        release_resources: bool,
    ) -> Result<(), anyhow::Error>;
    ///Allocates a single command buffer. Might fail if no free buffers are left
    fn allocate_buffer(
        self,
        level: ash::vk::CommandBufferLevel,
    ) -> Result<CommandBuffer<Self>, anyhow::Error>
    where
        Self: Sized;
    ///Allocates multiple command buffers at once. If it fails the error, and all successfuly allocated buffers are returned.
    ///
    /// By default this calls [allocate_buffer](CommandBufferAllocator::allocate_buffer) multiple times. An implementation however is free to provide a better implementation.
    fn allocate_buffers(
        self,
        level: ash::vk::CommandBufferLevel,
        count: u32,
    ) -> Result<Vec<CommandBuffer<Self>>, (anyhow::Error, Vec<CommandBuffer<Self>>)>
    where
        Self: Sized + Clone,
    {
        let mut buffers = Vec::with_capacity(count as usize);
        let mut err = None;
        for _i in 0..count {
            match Self::allocate_buffer(self.clone(), level) {
                Ok(b) => buffers.push(b),
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        if let Some(err) = err {
            return Err((err.into(), buffers));
        } else {
            Ok(buffers)
        }
    }

    fn device(&self) -> &ash::Device;
    fn raw(&self) -> &ash::vk::CommandPool;
}

pub struct CommandBuffer<P: CommandBufferAllocator> {
    ///Pool this command buffer was created from. Used for reset operations, and freeing on drop.
    pub pool: P,
    ///the raw vulkan handle
    pub inner: ash::vk::CommandBuffer,
}

impl<P: CommandBufferAllocator> CommandBuffer<P> {
    pub fn reset(&mut self, release_resources: bool) -> Result<(), anyhow::Error> {
        self.pool.reset(&self.inner, release_resources)
    }
}

impl<P: CommandBufferAllocator> Drop for CommandBuffer<P> {
    fn drop(&mut self) {
        unsafe {
            self.pool
                .device()
                .free_command_buffers(*self.pool.raw(), core::slice::from_ref(&self.inner))
        }
    }
}
