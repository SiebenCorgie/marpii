use std::fmt::{Debug, Pointer};
use std::sync::Arc;

use crate::state::{StBuffer, StImage};
use crate::{BufferState, ImageState};
use marpii::sync::Semaphore;
use marpii_commands::Recorder;

mod image_blit;
pub use image_blit::ImageBlit;
mod swapchain_prepare;
pub use swapchain_prepare::SwapchainPrepare;

mod wait_external;
pub use wait_external::WaitExternal;

pub enum SubPassRequirement {
    ///Signales that the queue this is executed on must be graphics capable.
    GraphicsBit,
    ///Signals that the queue this is exectured on must be compute capable.
    ComputeBit,
    ///If transfer must be possible.
    TransferBit,
    ///Signales that raytracing must be possible.
    RayTracing,
}

#[derive(Clone)]
pub enum AssumedState {
    Image {
        image: StImage,
        state: ImageState,
    },
    Buffer {
        buffer: StBuffer,
        state: BufferState,
    },
}

impl Debug for AssumedState{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
	match self{
	    AssumedState::Buffer { buffer, state } => {
		f.write_str("AssumedState::Buffer: ")?;
		buffer.buffer.fmt(f)?;
		state.fmt(f)
	    },
	    AssumedState::Image { image, state } => {
		f.write_str("AssumedState::Image: ")?;
		image.image.fmt(f)?;
		state.fmt(f)
	    }
	}
	
    }
}

impl AssumedState {
    ///Makes the inner buffer/images state the assumed state
    pub(crate) fn apply_state(self) {
        match self {
            AssumedState::Buffer { buffer, state } => *buffer.state.write().unwrap() = state,
            AssumedState::Image { image, state } => *image.state.write().unwrap() = state,
        }
    }

    ///Unwraps assuming an image, panics if not.
    pub fn unwrap_image(&self) -> &StImage{
	if let AssumedState::Image { image, .. } = self{
	    &image
	}else{
	    panic!("Was not an image!")
	}
    }

    ///Unwraps assuming a buff, panics if not.
    pub fn unwrap_buffer(&self) -> &StBuffer{
	if let AssumedState::Buffer { buffer, .. } = self{
	    &buffer
	}else{
	    panic!("Was not a buffer!")
	}
    }
    
    ///Returns the queue family the resource of `Self` is currently in.
    pub(crate) fn current_queue(&self) -> u32{
	match self {
            AssumedState::Buffer { buffer, .. } => buffer.queue.read().unwrap().queue_family(),
            AssumedState::Image { image, .. } => image.queue.read().unwrap().queue_family(),
        }
    }
}

///Generic pass definition. If non local resources, like input attachments etc. are used, expose their assumed state on `record` via the `assumed_states` function.
pub trait Pass {
    ///Returns the resources of this sub pass as well as their assumed state when calling `record`.
    fn assumed_states(&self) -> &[AssumedState];

    ///The actual recording step. Gets provided with access to the actual resources through the
    /// `ResourceManager` as well as the `command_buffer` recorder that is currently in use.
    fn record(&mut self, command_buffer: &mut Recorder) -> Result<(), anyhow::Error>;

    ///Can return a list of requirements that need to be fullfilled by the hosting pass,graph and vulkan context.
    fn requirements(&self) -> &'static [SubPassRequirement] {
        &[]
    }

    ///Allows the pass to declare additional semaphores that are signaled. Can be used for instance to signal
    /// the swapchain's present semaphore after the swapchain image is finished.
    fn signals_external(&self) -> &[Arc<Semaphore>] {
        &[]
    }

    fn waits_for_external(&self) -> &[Arc<Semaphore>] {
        &[]
    }
}
