use marpii::{ash::vk, context::Device};
use marpii_rmg::{ImageKey, ResourceRegistry, AttachmentDescription, Resources, Task, BufferKey};
use std::sync::Arc;


struct ForwardPass {
    attdesc: AttachmentDescription,
    sim_src: BufferKey
}

impl Task for ForwardPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        //register the description we need
        registry.request_attachment(self.attdesc.clone());
    }

    fn record(
        &mut self,
        device: &Arc<Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        println!("Forward pass")
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }
    fn name(&self) -> &'static str {
        "ForwardPass"
    }
}
