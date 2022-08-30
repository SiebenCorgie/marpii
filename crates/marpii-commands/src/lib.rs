//! # MarpII-Commands
//!
//! Implements a highlevel command buffer representation. The main part is the extension of [CommandBuffer][marpii::resources::CommandBuffer] with a [Recorder](Recorder)
//!
//! The recorder records commands on this command buffer and caputures all needed resources. After submitting the recorder to a queue all caputured resources are assosiated with a
//! fence that gets signaled when the command buffer has finished its execution. This way the resources have to stay valid for the duration of the command buffer's execution.
//!

mod managed_buffer;
pub use managed_buffer::{Captured, ManagedCommands, Recorder, Signal, SignalState};

mod buffer_init;
pub use buffer_init::buffer_from_data;

mod image_init;
pub use image_init::image_from_data;
#[cfg(feature = "image_loading")]
pub use image_init::{image_from_file, image_from_image};
#[cfg(feature = "image_loading")]
///image create re-export. Feel free to use it, since its already in your dependency tree.
pub use image;
