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


pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
