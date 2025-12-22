//! Helper that facilitate _common_ usage patterns in RMG. Many of those depend on the `marpii-rmg-macros` crate, to derive certain
//! functionality.

use marpii::ash::vk;
use smallvec::SmallVec;

use crate::{resources::handle::TypeErased, BufferHandle, ImageHandle, SamplerHandle};

pub mod computepass;
pub mod rasterpass;

///Declares at a high level how the image is used in the pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageUsage {
    StorageRead,
    StorageWrite,
    SampledRead,
    StorageAndSampleRead,
    ///Is bound as a render-target or read-attachment to the pass
    Attachment,
}

impl ImageUsage {
    pub fn into_layout(&self) -> vk::ImageLayout {
        match self {
            Self::StorageRead | Self::StorageWrite | Self::StorageAndSampleRead => {
                vk::ImageLayout::GENERAL
            }
            Self::SampledRead => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            Self::Attachment => vk::ImageLayout::ATTACHMENT_OPTIMAL,
        }
    }

    pub fn into_access_flags(&self) -> vk::AccessFlags2 {
        match self {
            Self::StorageRead => vk::AccessFlags2::SHADER_STORAGE_READ,
            Self::StorageWrite => vk::AccessFlags2::SHADER_STORAGE_WRITE,
            Self::SampledRead => vk::AccessFlags2::SHADER_SAMPLED_READ,
            Self::StorageAndSampleRead => {
                vk::AccessFlags2::SHADER_SAMPLED_READ | vk::AccessFlags2::SHADER_STORAGE_READ
            }
            Self::Attachment => {
                vk::AccessFlags2::INPUT_ATTACHMENT_READ
                    | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags2::COLOR_ATTACHMENT_READ
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    Read,
    Write,
    ReadWrite,
}

impl BufferUsage {
    pub fn into_access_flags(&self) -> vk::AccessFlags2 {
        match self {
            Self::Read => vk::AccessFlags2::SHADER_STORAGE_READ,
            Self::Write => vk::AccessFlags2::SHADER_STORAGE_WRITE,
            Self::ReadWrite => {
                vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE
            }
        }
    }
}

///Small helper that makes it easier for us to write
/// generic passes.
pub(crate) struct ResourceStorage {
    pub images: SmallVec<[(ImageHandle, ImageUsage); 8]>,
    pub buffers: SmallVec<[(BufferHandle<TypeErased>, BufferUsage); 8]>,
    pub samplers: SmallVec<[SamplerHandle; 4]>,
}
impl ResourceStorage {
    pub(crate) fn new() -> Self {
        ResourceStorage {
            images: SmallVec::default(),
            buffers: SmallVec::default(),
            samplers: SmallVec::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.images.clear();
        self.buffers.clear();
        self.samplers.clear();
    }
}

///Trait that generates the VertexFormat of some data.
pub trait VertexFormat {
    fn vertex_input_attribute_descriptions(&self) -> &[vk::VertexInputAttributeDescription];
    fn vertex_input_state<'a>(&'a self) -> vk::PipelineVertexInputStateCreateInfo<'a>;
}

///Trait that lets a pass define how many / which `DynamicRendering` based input attachments will be supplied
///to a graphics pipeline.
///
/// # Safety:
///
/// `assert_eq!(self.color_image_formats().len(), self.color_blend_attachments().len())` should hold true.
pub trait DynamicRenderingInfo {
    fn color_image_formats(&self) -> &[vk::Format];
    fn depth_format(&self) -> Option<&vk::Format>;
    fn color_blend_attachments(&self) -> &[vk::PipelineColorBlendAttachmentState];
}
