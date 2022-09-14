use anyhow::Result;
use marpii::ash::vk::SamplerMipmapMode;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::{ImgDesc, Sampler, ImageView, SafeImageView};
use marpii::{
    ash::{self, vk, vk::Extent2D},
    context::Ctx,
};
use marpii_rmg::Rmg;
use winit::event::{DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};


fn main() -> Result<(), anyhow::Error> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .init()
        .unwrap();


    let ev = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&ev).unwrap();


    let (context, surface) = Ctx::default_with_surface(&window, true)?;

    let mut rmg = Rmg::new(context, &surface)?;

    let buffer = rmg.new_buffer::<usize>(1024, Some("TestBuffer"))?;
    let storage_image = rmg.new_image_uninitialized(
        ImgDesc::storage_image_2d(1024, 1024, vk::Format::R8G8B8A8_UINT),
        false,
        Some("StorageImage")
    )?;
    let sampled_image = rmg.new_image_uninitialized(
        ImgDesc::texture_2d(1024, 1024, vk::Format::R8G8B8A8_UINT),
        true,
        Some("SampledImage")
    )?;
    let sampler = rmg.new_sampler(&vk::SamplerCreateInfo::builder())?;


    Ok(())
}
