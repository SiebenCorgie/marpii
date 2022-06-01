/// Bindless descriptor helper. Manages a single big descriptor sset that binds all resources.
/// in the shader there is one binding per descriptor type (Sampled Image, Storage Image, Buffer), each binding is a array of multiple
/// images/buffers that can be indexed.
///
/// # Queue ownership
/// Since the bindless descriptorset does not take care of queue ownership you have to make sure that:
///
/// 1. The descriptor set is used only on the same queue family
/// 2. bound resoureces are owned by this queue family
/// 3. bound resources are in the correct access-mask / image-layout for the intended access
///
/// Note that the `marpii-command-graph` crate can also handle bindless handle transitions (in the same way it handles normal image/buffer resources). So if you are using this bindless helper and the command graph, just submit images/buffers as `StImage` and `StBuffer`.
///
/// # In shader usage
/// 👷TODO👷
///
/// # Safety
/// Note that the helper assumes that the resources are used on the correct queue, and with he correct layouts. It does not track queue ownership or layout tranisitons for you.
pub struct BindlessDescriptor {}

impl BindlessDescriptor {
    ///Creates a new instance of a bindless descriptor set.
    pub fn new() -> Result<Self, anyhow::Error> {
        //TODO - check that all flags are set
        //     - setup layout
        //     return
        todo!()
    }

    ///Bindless descriptorset layout
    pub fn layout(&self) -> &marpii::ash::vk::DescriptorSetLayout {
        todo!()
    }
}
