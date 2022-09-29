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
use copy_buffer::CopyToGraphicsBuffer;
use forward_pass::ForwardPass;
use image::EncodableLayout;
use marpii::resources::ImgDesc;
use marpii::{ash::vk, context::Ctx};
use marpii_rmg::tasks::{SwapchainBlit, UploadImage};
use marpii_rmg::Rmg;
use simulation::Simulation;
use winit::event::{ElementState, KeyboardInput, VirtualKeyCode};
use winit::window::Window;
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

mod forward_pass;
mod gltf_loader;
mod simulation;
mod copy_buffer;

pub const OBJECT_COUNT: usize = 64;

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
    let mut buffer_copy = CopyToGraphicsBuffer::new(&mut rmg, simulation.sim_buffer.clone())?;
    let mut forward = ForwardPass::new(&mut rmg).unwrap();
    let mut swapchain_blit = SwapchainBlit::new();

    ev.run(move |ev, _, cf| {
        *cf = ControlFlow::Poll;
        match ev {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {

                //set the *oldest* valid simulatio src for the forward pass
                forward.sim_src = Some(buffer_copy.last_buffer());

                //setup src image and blit
                swapchain_blit.next_blit(forward.color_image.clone());

                rmg.record(window_extent(&window))
                    .add_task(&mut simulation)
                    .unwrap()
                    .add_task(&mut buffer_copy)
                    .unwrap()
                    .add_task(&mut forward)
                    .unwrap()
                    .add_task(&mut swapchain_blit)
                    .unwrap()
                    .execute()
                    .unwrap();
            }
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    },
                ..
            } => *cf = ControlFlow::Exit,
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
