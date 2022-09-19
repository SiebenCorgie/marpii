use crate::{
    resources::{
        res_states::{
            AnyRes, AnyResKey, BufferKey, ImageKey, ResBuffer, ResImage, ResSampler, SamplerKey,
        },
        Resources,
    },
    RecordError, Rmg,
};
use marpii::{
    ash::vk::{self, Extent2D},
    context::Device,
};
use slotmap::SlotMap;
use std::{sync::Arc, ops::Deref};

pub struct AttachmentDescription {
    write: bool,
    format: vk::Format,
    extent: Extent2D,
}

pub(crate) enum AttachmentDescState {
    Unresolved(AttachmentDescription),
    Resolved(ImageKey),
}

pub struct ResourceRegistry<'res> {
    ///Current mapping used for attachments
    name_mapping: &'res [&'res str],

    images: Vec<ImageKey>,
    buffers: Vec<BufferKey>,
    sampler: Vec<SamplerKey>,

    //Attachment states, at some point they are resolved into an actual image that must be bound to
    // the attachment descriptor.
    attachments: Vec<AttachmentDescState>,

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
            foreign_sem: Vec::new()
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

    pub fn append_foreign_signal_semaphores(&self, infos: &mut Vec<vk::SemaphoreSubmitInfo>){
        for sem in self.foreign_sem.iter(){

            #[cfg(feature="logging")]
            log::trace!("Registering foreign semaphore {:?}", sem.deref().deref());

            infos.push(
                vk::SemaphoreSubmitInfo::builder()
                    .semaphore(**sem)
                    .build()
            );
        }

    }
}

pub trait Task {

    ///Gets called right before building the execution graph. Allows access to the Resources.
    fn pre_record(&mut self, resources: &mut Resources){}

    ///Gets called right after executing the resource graph
    fn post_execution(&mut self, resources: &mut Resources){}

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
