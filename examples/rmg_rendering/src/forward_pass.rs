

struct ForwardPass {
    shadow: ImageKey,
    target: ImageKey,
    meshes: BufferKey,
}

impl Task for ForwardPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        registry.request_image(self.shadow);
        registry.request_image(self.target);
        registry.request_buffer(self.meshes);
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
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
