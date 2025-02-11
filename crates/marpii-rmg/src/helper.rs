//! Helper that facilitate _common_ usage patterns in RMG. Many of those depend on the `marpii-rmg-macros` crate, to derive certain
//! functionality.

use marpii::ash::vk;

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
