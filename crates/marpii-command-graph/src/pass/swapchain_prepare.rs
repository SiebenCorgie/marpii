use std::sync::Arc;

use marpii::{ash::vk, sync::Semaphore};
use marpii_commands::Recorder;

use crate::{ImageState, StImage};

use super::{AssumedState, Pass};

///Handles handles transition of the `image` into a presentable state and signals the `semaphore`.
pub struct SwapchainPrepare {
    #[allow(dead_code)] //TODO might use later ...
    image: StImage,
    assume: [AssumedState; 1],
    signals: Arc<Semaphore>,
}

impl SwapchainPrepare {
    pub fn new(image: StImage, signals: Arc<Semaphore>) -> Self {
        SwapchainPrepare {
            image: image.clone(),
            assume: [AssumedState::Image {
                image,
                state: ImageState {
                    access_mask: vk::AccessFlags::empty(),
                    layout: vk::ImageLayout::PRESENT_SRC_KHR,
                },
            }],
            signals,
        }
    }
}

impl Pass for SwapchainPrepare {
    fn assumed_states(&self) -> &[AssumedState] {
        &self.assume
    }

    ///The actual recording step. Gets provided with access to the actual resources through the
    /// `ResourceManager` as well as the `command_buffer` recorder that is currently in use.
    fn record(&mut self, _command_buffer: &mut Recorder) -> Result<(), anyhow::Error> {
        Ok(()) //doesn't do anything.
    }

    fn signals_external(&self) -> &[Arc<Semaphore>] {
        core::slice::from_ref(&self.signals)
    }
}
