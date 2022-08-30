use marpii::ash::vk::{self, QueueFlags};
use crate::{graph::TaskRecord, resources::{ImageKey, BufferKey}};

mod blit;
pub use blit::Blit;

#[derive(Clone, Copy)]
pub enum AttachmentType{
    Framebuffer,
    Defined(vk::Extent2D)
}

#[derive(Clone, Copy)]
pub enum AccessType {
    Read,
    Write
}

///Defines an attachment image to this pass.
#[derive(Clone, Copy)]
pub struct Attachment{
    pub ty: AttachmentType,
    pub access: AccessType,
    pub access_mask: vk::AccessFlags2,
    pub layout: vk::ImageLayout
}


//TODO: Currently the used buffers and images have to be declared by the pass. Would be nicer if we could calculate the access.
//      Not sure how though...
pub trait Task{
    ///Should return the list of image attachments for this pass.
    fn attachments(&self) -> &[Attachment]{
        &[]
    }

    ///Should return images that are used in this task. This should basically be the list of all image keys that are available to the shader/kernel
    /// when executed on GPU.
    ///
    /// # Important
    ///
    /// This does only mean image resources, NOT shader attachments (like render targets). Usually this means textures or lookup tables etc.
    fn images(&self) -> &[ImageKey]{
        &[]
    }

    ///Should return all buffers that are available to the kernel/shader at execution time.
    fn buffers(&self) -> &[BufferKey]{
        &[]
    }

    ///on record function that gets called while recording the command buffer. Note that a DescriptorSet as described by [attachments](Self::attachments) is
    /// bound to set 0. Set 1 contains the bindless resources, which should be everything else apart from the PushConstants.
    fn record(&self, recorder: &mut TaskRecord);

    ///Signals the task type to the recorder. By default this is compute only.
    fn queue_flags(&self) -> QueueFlags{
        QueueFlags::COMPUTE
    }
}



///Dummy task that does nothing but needs the given targets.
pub(crate) struct DummyTask<const N: usize>{
    pub attachments: [Attachment; N]
}

pub(crate) const READATT: Attachment = Attachment{
    ty: AttachmentType::Framebuffer,
    access: AccessType::Read,
    access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_READ,
    layout: vk::ImageLayout::ATTACHMENT_OPTIMAL
};
pub(crate) const WRITEATT: Attachment = Attachment{
    ty: AttachmentType::Framebuffer,
    access: AccessType::Write,
    access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_READ,
    layout: vk::ImageLayout::ATTACHMENT_OPTIMAL
};

impl<const N: usize> Task for DummyTask<N> {
    fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }

    fn record(&self, recorder: &mut TaskRecord) {

    }
}
