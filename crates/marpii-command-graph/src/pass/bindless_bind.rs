use super::{AssumedState, Pass};
use marpii::{ash::vk, resources::PipelineLayout};
use marpii_commands::Recorder;
use marpii_descriptor::bindless::BindlessDescriptor;

/// Binds the given bindless setup to the command buffer as well . Must be bound before anything that depends on the
/// bindless setup is executed.
pub struct BindlessBind {
    pub bindless_descriptor: BindlessDescriptor,
    layout: PipelineLayout,
    pub bindpoint: vk::PipelineBindPoint,
}

impl BindlessBind {
    pub fn new(bindless: BindlessDescriptor, bindpoint: vk::PipelineBindPoint) -> Self {
        let layout = bindless.new_pipeline_layout(128);
        BindlessBind {
            bindless_descriptor: bindless,
            bindpoint,
            layout,
        }
    }
}

impl Pass for BindlessBind {
    fn assumed_states(&self) -> &[AssumedState] {
        &[]
    }

    ///The actual recording step. Gets provided with access to the actual resources through the
    /// `ResourceManager` as well as the `command_buffer` recorder that is currently in use.
    fn record(&mut self, command_buffer: &mut Recorder) -> Result<(), anyhow::Error> {
        let descriptors = self.bindless_descriptor.clone_descriptor_sets();
        let bindpoint = self.bindpoint;
        let layout = self.layout.layout;

        command_buffer.record(move |dev, cmd| {
            let bindings = descriptors.iter().map(|d| d.inner).collect::<Vec<_>>();
            unsafe {
                dev.cmd_bind_descriptor_sets(*cmd, bindpoint, layout, 0, &bindings, &[]);
            }
        });

        Ok(())
    }
}
