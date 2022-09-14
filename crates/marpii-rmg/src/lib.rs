//! # ResourceManagingGraph (RMG)
//!
//! The RMG is a big abstraction layer over raw vulkan. It is therefore much more opinionated then the rest of MarpII.
//!
//! It handles the context creation as well as resource creation and binding. The user (you) primarily interacts in the form of [Task](recorder::Task)s. They can be scheduled
//! in an execution Graph using a [Recorder](recorder::Recorder). The tasks implementation is up to you and has full access to all resources and the Vulkan context.
//!
//! TODO: more docs on how to get started etc.

mod resources;

mod recorder;
pub use recorder::task::Task;

///Top level Error structure.
#[derive(Debug, Error)]
pub enum RmgError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Recording error")]
    RecordingError(#[from] RecordError),
}


///Main RMG interface.
pub struct Rmg{

}
