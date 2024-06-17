//! # Tasks
//!
//! Collects a number of tasks that are usefull in multiple rendering/GPGPU contexts.
//!
//! ## Building
//!
//! Note that you need `glslangVaildator` in your `$PATH` to be able to build the crate.
#![deny(warnings)]

use marpii::MarpiiError;
use marpii_rmg::RmgError;
use std::fmt::{Debug, Display};
use thiserror::Error;

mod dynamic_buffer;
pub use dynamic_buffer::DynamicBuffer;
mod swapchain_present;
pub use swapchain_present::SwapchainPresent;
mod upload_buffer;
pub use upload_buffer::UploadBuffer;
mod upload_image;
pub use upload_image::{MipOffset, UploadImage};

mod dynamic_image;
pub use dynamic_image::DynamicImage;
mod image_blit;
pub use image_blit::ImageBlit;
mod alpha_blend;
pub use alpha_blend::AlphaBlend;
mod download_buffer;
pub use download_buffer::{DownloadBuffer, DownloadError};

mod downsample;
pub use downsample::Downsample;

#[cfg(feature = "egui-task")]
mod egui_integration;
#[cfg(feature = "egui-task")]
pub use crate::egui_integration::{EGuiTask, EGuiWinitIntegration};
#[cfg(feature = "egui-task")]
pub use egui_winit::egui;
#[cfg(feature = "egui-task")]
pub use egui_winit::winit;

#[derive(Error, Debug)]
pub struct NoTaskError;

impl Display for NoTaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NoError")
    }
}

///Typedef for a error that has no task specific version.
pub type RmgTaskError = TaskError<NoTaskError>;

///Allows you to specify from which part of either your task, or MarpII an error originated.
/// Usually a distinction between direct MarpII calls and MrapII's task graph (RMG) is made.
#[derive(Error, Debug)]
pub enum TaskError<TaskErr: std::error::Error> {
    #[error("Task Error: {0}")]
    Task(TaskErr),

    #[error("Marpii internal error: {0}")]
    Marpii(#[from] MarpiiError),

    #[error("Task graph error: {0}")]
    RmgError(#[from] RmgError),
}
