//! # Rmg Rendering example
//!
//! Showcases how the rendergraph library can be used to easily schedule tasks that make up
//! a complex, async executed frame.
//!
//! The example uses a asynchronous "Simulation" task to *simulate* particle movement via a compute shader for
//! frame N+1. Simultaneously the simulation result from N is used to render a objects using the "ForwardPass" which are
//! then presented on screen.
//!
//! On GPUs that support async compute this is done in parallel.
//!
//! The number of object can be changed by changing the global constant "OBJECT_COUNT".
//!
//! The execution graph:
//!
//!  graphics |------------------------------|    |-------------------|
//!  ---------| Forward render to attachment |----| Blit to swapchain |---------
//!        /  |------------------------------|    |-------------------|
//!    .../                                                              / ... (acquired in next frame by graphics)
//!  compute  |------------------------|     |-------------------------|/
//!  ---------| Compute Simulation N+1 |-----| Copy to graphics buffer |--------
//!           |------------------------|     |-------------------------|
//!
//! NOTE: The execution is not perfect, for instance the copy to the buffer after the compute command is not necessarily needed.
//!       Similarly the rendering could happen directly to the swapchain image. However, this example tries to showcase the scheduling as
//!       simple as possible. So its left that way :) ... Also maybe we add a post progress pass later or something :D
//!
//!



use anyhow::Result;
use image::EncodableLayout;
use marpii::resources::ImgDesc;
use marpii::{
    ash::vk,
    context::Ctx,
};
use marpii_rmg::tasks::{SwapchainBlit, UploadImage};
use marpii_rmg::{BufferKey, ImageKey, ResourceRegistry, Resources, Rmg, SamplerKey, Task};
use simulation::Simulation;
use winit::event::{DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode};
use winit::window::Window;
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};


mod simulation;
mod forward_pass;
mod gltf_loader;


pub const OBJECT_COUNT: usize = 32;

fn main() -> Result<(), anyhow::Error> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .init()
        .unwrap();

    let ev = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&ev).unwrap();

    let (context, surface) = Ctx::default_with_surface(&window, true)?;

    let mut rmg = Rmg::new(context, &surface)?;


    let mut simulation = Simulation::new(&mut rmg)?;


    let image_data = image::open("test.png").unwrap();
    let image_data = image_data.to_rgba32f();

    let mut swapchain_blit = SwapchainBlit::new();

    let mut counter = 0;
    ev.run(move |ev, _, cf| {
        *cf = ControlFlow::Poll;

        match ev {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {

                counter += 1;

                if counter > 1024{
                    *cf = ControlFlow::Exit;
                }

                let swimage_image = rmg.new_image_uninitialized(
                    ImgDesc::storage_image_2d(
                        image_data.width(),
                        image_data.height(),
                        vk::Format::R32G32B32A32_SFLOAT,
                    ),
                    Some("SwImage"),
                ).unwrap();
                let mut init_image = UploadImage::new(swimage_image, image_data.as_bytes());

                //setup src image and blit
                swapchain_blit.next_blit(swimage_image);

                rmg.record(window_extent(&window))
                   .add_task(&mut init_image, &[])
                   .unwrap()
                   .add_task(&mut simulation, &[])
                   .unwrap()
                   .add_task(&mut swapchain_blit, &[])
                   .unwrap()
                   .execute()
                   .unwrap();

                rmg.delete(swimage_image).unwrap();
            }
            _ => {}
        }
    })
}

fn window_extent(window: &Window) -> vk::Extent2D {
    vk::Extent2D {
        width: window.inner_size().width,
        height: window.inner_size().height,
    }
}
