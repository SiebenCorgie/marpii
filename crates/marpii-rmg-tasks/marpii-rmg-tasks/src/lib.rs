//! # Tasks
//!
//! Collects a number of tasks that are usefull in multiple rendering/GPGPU contexts.
//!
//! ## Building
//!
//! Note that you need `glslangVaildator` in your `$PATH` to be able to build the crate.


mod dynamic_buffer;
pub use dynamic_buffer::DynamicBuffer;
mod swapchain_blit;
pub use swapchain_blit::SwapchainBlit;
mod upload_buffer;
pub use upload_buffer::UploadBuffer;
mod upload_image;
pub use upload_image::UploadImage;
mod dynamic_image;
pub use dynamic_image::DynamicImage;
#[cfg(feature="egui-task")]
mod egui_integration;
#[cfg(feature="egui-task")]
pub use crate::egui_integration::{EGuiRender, EGuiWinitIntegration};
#[cfg(feature="egui-task")]
pub use egui_winit::egui;

///Rust shader byte code. Compiled ahead of the crate and included for *save* distribution.
pub const SHADER_RUST: &'static [u8] = include_bytes!("../../resources/rshader.spv");
