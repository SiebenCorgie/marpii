mod image;
pub use image::{Image, ImageType, ImageView, ImgDesc, ImgViewDesc, SafeImageView};

mod buffer;
pub use buffer::{BufDesc, Buffer};

mod push_constant;
pub use push_constant::PushConstant;

mod descriptor;
#[cfg(feature = "shader_reflection")]
pub use descriptor::shader_interface::Reflection;
pub use descriptor::DescriptorSetLayout;

mod pipeline;
pub use pipeline::PipelineLayout;

mod shader_module;
pub use shader_module::ShaderModule;

use smallvec::SmallVec;

///Memory usage types
#[derive(Clone, Debug)]
pub enum SharingMode {
    Exclusive,
    Concurrent {
        ///The queue family indices of families that can access the image concurrently.
        queue_family_indices: SmallVec<[u32; 4]>,
    },
}
