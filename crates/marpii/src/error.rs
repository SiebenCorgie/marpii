use std::error::Error;

use ash::{vk, LoadingError};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DeviceError {
    #[error("Extension {0} is not supported by device")]
    UnsupportedExtension(String),
    #[error("Feature {0} not supported")]
    UnsupportedFeature(String),
    #[error("Could not get format properties for {format:#?}")]
    GetFormatProperties {
        format: vk::Format,
        #[source]
        error: vk::Result,
    },
    #[error("No physical device found. Is a Vulkan capable GPU and driver installed?")]
    NoPhysicalDevice,
    #[error("Swapchain can't have a extent of 0 on either axis, was: {0:#?}")]
    InvalidSwapchainSize(vk::Extent2D),
    //FIXME: Not happy about that Box :/
    #[error("GpuAllocator error: {0}")]
    GpuAllocatorError(#[from] Box<dyn Error + Send + Sync + 'static>),
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),
}

#[derive(Error, Debug)]
pub enum ShaderError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),
    #[error("Filesystem error: {0}")]
    FileError(#[from] std::io::Error),
    //FIXME: The actual error is not sendable atm.
    #[cfg_attr(feature = "shader_reflection", error("Reflection error: {0}"))]
    ReflectionError(String),
}

#[derive(Error, Debug)]
pub enum CommandBufferError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),
    #[error("Command pool is not resettable")]
    PoolNotResetable,
    #[error("Submitting to queue failed with {0}")]
    SubmitFailed(vk::Result),
    #[error("Failed to allocate command buffer. Requested {count}, got {allocated}")]
    FailedToAllocate { allocated: usize, count: usize },
}

#[derive(Error, Debug)]
pub enum InstanceError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),
    #[error("Failed to load Vulkan entry point: {0}")]
    EntryLoading(#[from] LoadingError),
}

#[derive(Error, Debug)]
pub enum DescriptorError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),
    #[error("Failed to allocate Descriptors from pool. Requested {requested} got {count}")]
    Allocation { requested: usize, count: usize },
    #[error("Descriptorset can't be freed")]
    UnFreeable,
}

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),
    #[error("Failed to allocate pipeline")]
    Allocation,
}

#[derive(Error, Debug)]
pub enum MarpiiError {
    #[error("CommandBuffer error: {0}")]
    CommandBufferError(#[from] CommandBufferError),
    #[error("Device error: {0}")]
    DeviceError(#[from] DeviceError),
    #[error("Desriptor error: {0}")]
    DescriptorError(#[from] DescriptorError),
    #[error("Instance error: {0}")]
    InstanceError(#[from] InstanceError),
    #[error("Pipeline error: {0}")]
    PipelineError(#[from] PipelineError),
    #[error("Shader/ShaderModule error: {0}")]
    ShaderError(#[from] ShaderError),
    #[error("Other error: {0}")]
    Other(String),
}

#[cfg(test)]
mod test {
    use static_assertions::assert_impl_all;

    use crate::{
        error::{
            CommandBufferError, DescriptorError, DeviceError, InstanceError, PipelineError,
            ShaderError,
        },
        MarpiiError,
    };

    #[test]
    fn assure_send_sync() {
        assert_impl_all!(DeviceError: Send, Sync);
        assert_impl_all!(ShaderError: Send, Sync);
        assert_impl_all!(CommandBufferError: Send, Sync);
        assert_impl_all!(InstanceError: Send, Sync);
        assert_impl_all!(DescriptorError: Send, Sync);
        assert_impl_all!(PipelineError: Send, Sync);
        assert_impl_all!(MarpiiError: Send, Sync);
    }
}
