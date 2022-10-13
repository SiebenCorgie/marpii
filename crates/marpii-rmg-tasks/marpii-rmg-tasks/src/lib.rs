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

#[cfg(feature="egui-task")]
mod egui;
#[cfg(feature="egui-task")]
pub use egui::EGuiRender;
