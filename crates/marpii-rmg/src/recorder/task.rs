use marpii::ash::vk::{self, Extent2D};
use marpii_commands::ManagedCommands;
use slotmap::SlotMap;

use crate::resources::{res_states::{ImageKey, AnyRes, BufferKey, SamplerKey, ResImage, ResBuffer, ResSampler}, Resources};


pub struct AttachmentDescription{
    write: bool,
    format: vk::Format,
    extent: Extent2D
}

pub(crate) enum AttachmentDescState{
    Unresolved(AttachmentDescription),
    Resolved(ImageKey)
}

pub struct ResourceRegistry<'res>{
    ///Current mapping used for attachments
    name_mapping: &'res [&'res str],

    images: Vec<ImageKey>,
    buffers: Vec<BufferKey>,
    sampler: Vec<SamplerKey>,

    //Attachment states, at some point they are resolved into an actual image that must be bound to
    // the attachment descriptor.
    attachments: Vec<AttachmentDescState>
}

impl<'res> ResourceRegistry<'res>{

    pub fn new(attachment_names: &'res [&'res str]) -> Self{
        ResourceRegistry {
            name_mapping: attachment_names,
            images: Vec::new(),
            buffers: Vec::new(),
            sampler: Vec::new(),
            attachments: Vec::new()
        }
    }

    ///Registers `image` as needed storage image.
    pub fn request_image(&mut self, image: ImageKey){
        self.images.push(image);
    }

    ///Registers `buffer` as needed storage buffer.
    pub fn request_buffer(&mut self, buffer: BufferKey){
        self.buffers.push(buffer);
    }

    ///Registers `sampler` as needed sampler.
    pub fn request_sampler(&mut self, sampler: SamplerKey){
        self.sampler.push(sampler);
    }

    pub fn request_attachment(&mut self, desc: AttachmentDescription){
        self.attachments.push(AttachmentDescState::Unresolved(desc));
    }
}


///Scoped access to resources, exposing their handle and current state at the time of recording.
pub struct ResourceAccess<'res>{
    pub images: SlotMap<ImageKey, &'res ResImage>,
    pub buffers: SlotMap<BufferKey, &'res ResBuffer>,
    pub samplers: SlotMap<SamplerKey, &'res ResSampler>,
}

pub trait Task{
    ///Gets called while building a execution graph. This function must register all resources that are
    /// needed for successfull execution.
    fn register(&self, registry: &mut ResourceRegistry);

    fn record(&mut self, command_buffer: &mut ManagedCommands, resources: &ResourceAccess);
}
