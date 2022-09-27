use crate::{
    resources::{
        res_states::{AnyResKey, BufferKey, ImageKey, SamplerKey},
        Resources,
    },
    CtxRmg, RecordError,
};
use marpii::{
    ash::vk::{self, Extent2D},
    context::Device, resources::{ImgDesc, ImageType, SharingMode},
};
use std::{ops::Deref, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AttachmentType{
    Color,
    Depth
}

impl Into<vk::ImageUsageFlags> for AttachmentType{
    fn into(self) -> vk::ImageUsageFlags {
        match self {
            AttachmentType::Color => vk::ImageUsageFlags::COLOR_ATTACHMENT |vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::STORAGE,
            AttachmentType::Depth => vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT |vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::STORAGE
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AttachmentDescription {
    write: bool,
    format: vk::Format,
    extent: Extent2D,
    attachment_type: AttachmentType,
}

impl AttachmentDescription{
    pub fn to_image_desciption(&self) -> ImgDesc{
        ImgDesc {
            img_type: ImageType::Tex2d,
            format: self.format,
            extent: vk::Extent3D { width: self.extent.width, height: self.extent.height, depth: 0 },
            mip_levels: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: self.attachment_type.into(),
            sharing_mode: SharingMode::Exclusive
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum AttachmentDescState {
    Unresolved(AttachmentDescription),
    Resolved(ImageKey),
}

#[allow(dead_code)]
pub struct ResourceRegistry<'res> {
    ///Current mapping used for attachments
    name_mapping: &'res [&'res str],

    images: Vec<ImageKey>,
    buffers: Vec<BufferKey>,
    sampler: Vec<SamplerKey>,

    //Attachment states, at some point they are resolved into an actual image that must be bound to
    // the attachment descriptor.
    pub(crate) attachments: Vec<AttachmentDescState>,

    foreign_sem: Vec<Arc<vk::Semaphore>>,
}

impl<'res> ResourceRegistry<'res> {
    pub fn new(attachment_names: &'res [&'res str]) -> Self {
        ResourceRegistry {
            name_mapping: attachment_names,
            images: Vec::new(),
            buffers: Vec::new(),
            sampler: Vec::new(),
            attachments: Vec::new(),
            foreign_sem: Vec::new(),
        }
    }

    ///Registers `image` as needed storage image.
    pub fn request_image(&mut self, image: ImageKey) {
        self.images.push(image);
    }

    ///Registers `buffer` as needed storage buffer.
    pub fn request_buffer(&mut self, buffer: BufferKey) {
        self.buffers.push(buffer);
    }

    ///Registers `sampler` as needed sampler.
    pub fn request_sampler(&mut self, sampler: SamplerKey) {
        self.sampler.push(sampler);
    }

    pub fn request_attachment(&mut self, desc: AttachmentDescription) {
        self.attachments.push(AttachmentDescState::Unresolved(desc));
    }

    ///Registers that this foreign semaphore must be signaled after execution. Needed for swapchain stuff.
    pub(crate) fn register_foreign_semaphore(&mut self, semaphore: Arc<vk::Semaphore>) {
        self.foreign_sem.push(semaphore);
    }

    pub fn any_res_iter<'a>(&'a self) -> impl Iterator<Item = AnyResKey> + 'a {
        self.images
            .iter()
            .map(|img| AnyResKey::Image(*img))
            .chain(self.buffers.iter().map(|buf| AnyResKey::Buffer(*buf)))
            .chain(self.sampler.iter().map(|sam| AnyResKey::Sampler(*sam)))
    }

    pub(crate) fn append_foreign_signal_semaphores(&self, infos: &mut Vec<vk::SemaphoreSubmitInfo>) {
        for sem in self.foreign_sem.iter() {
            #[cfg(feature = "logging")]
            log::trace!("Registering foreign semaphore {:?}", sem.deref().deref());

            infos.push(
                vk::SemaphoreSubmitInfo::builder()
                    .semaphore(**sem)
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .build()
            );
        }
    }
}

pub trait Task {
    ///Gets called right before building the execution graph. Allows access to the Resources.
    fn pre_record(&mut self, _resources: &mut Resources, _ctx: &CtxRmg) -> Result<(), RecordError> {
        Ok(())
    }

    ///Gets called right after executing the resource graph
    fn post_execution(&mut self, _resources: &mut Resources, _ctx: &CtxRmg) -> Result<(), RecordError> {
        Ok(())
    }

    ///Gets called while building a execution graph. This function must register all resources that are
    /// needed for successfull execution.
    fn register(&self, registry: &mut ResourceRegistry);

    fn record(
        &mut self,
        device: &Arc<Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    );

    ///Signals the task type to the recorder. By default this is compute only.
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }

    ///Can be implemented to make debugging easier
    fn name(&self) -> &'static str {
        "Unnamed Task"
    }
}
