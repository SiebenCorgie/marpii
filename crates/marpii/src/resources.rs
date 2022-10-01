mod image;
pub use image::{Image, ImageType, ImageView, ImgDesc, ImgViewDesc, SafeImageView, Sampler};

mod buffer;
pub use buffer::{BufDesc, Buffer, BufferMapError};

mod push_constant;
pub use push_constant::PushConstant;

mod descriptor;
#[cfg(feature = "shader_reflection")]
pub use descriptor::shader_interface::Reflection;
pub use descriptor::{DescriptorAllocator, DescriptorPool, DescriptorSet, DescriptorSetLayout};

pub mod pipeline;
pub use pipeline::{compute::ComputePipeline, graphics::GraphicsPipeline, PipelineLayout};

mod command_buffer;
pub use command_buffer::{CommandBuffer, CommandBufferAllocator, CommandPool};

mod shader_module;
pub use shader_module::{ShaderModule, ShaderStage};

use smallvec::SmallVec;

///Memory usage types
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SharingMode {
    Exclusive,
    Concurrent {
        ///The queue family indices of families that can access the image concurrently.
        queue_family_indices: SmallVec<[u32; 4]>,
    },
}
