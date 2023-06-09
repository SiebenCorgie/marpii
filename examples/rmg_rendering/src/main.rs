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

use std::path::PathBuf;

use anyhow::Result;
use camera_controller::Camera;
use copy_buffer::CopyToGraphicsBuffer;
use forward_pass::ForwardPass;
use marpii::{ash::vk, context::Ctx};
use marpii_rmg::Rmg;
use marpii_rmg_tasks::{DynamicBuffer, SwapchainPresent};
use shared::Ubo;
use simulation::Simulation;
use winit::event::{ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};
mod camera_controller;
mod copy_buffer;
mod forward_pass;
mod gltf_loader;
mod model_loading;
mod simulation;

pub const OBJECT_COUNT: usize = 8192;

fn main() -> Result<(), anyhow::Error> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();

    let mut args = std::env::args();
    let _progname = args.next();
    let mesh_path = if let Some(path) = args.next() {
        let path = PathBuf::from(path);
        if !path.exists() {
            anyhow::bail!("Gltf-file @ {:?} does not exist!", path);
        }

        path
    } else {
        anyhow::bail!(
            "No gltf path provided, try $cargo run --bin rmg_rendering -- path/to/gltf/name.gltf!"
        );
    };

    let gltf = easy_gltf::load(mesh_path).unwrap();

    let ev = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&ev).unwrap();
    let (context, surface) = Ctx::default_with_surface(&window, true)?;
    let mut rmg = Rmg::new(context)?;

    let mut camera = Camera::default();
    let mut ubo_update = DynamicBuffer::new(&mut rmg, &[camera.to_ubo(&window)])?;
    let mut simulation = Simulation::new(&mut rmg)?;
    let mut buffer_copy = CopyToGraphicsBuffer::new(&mut rmg, simulation.sim_buffer.clone())?;
    let mut forward =
        ForwardPass::new(&mut rmg, ubo_update.buffer_handle().clone(), &gltf).unwrap();
    let mut swapchain_blit = SwapchainPresent::new(&mut rmg, surface)?;

    ev.run(move |ev, _, cf| {
        *cf = ControlFlow::Poll;

        camera.on_event(&ev);

        match ev {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                camera.tick();

                //update framebuffer extent to current one.
                let framebuffer_ext = swapchain_blit.extent().unwrap_or(vk::Extent2D {
                    width: window.inner_size().width,
                    height: window.inner_size().height,
                });

                log::info!("Start frame for {:?}", framebuffer_ext);
                forward.target_img_ext = framebuffer_ext;

                //update camera
                ubo_update.write(&[camera.to_ubo(&window)], 0).unwrap();

                //set the *oldest* valid simulation src for the forward pass
                forward.sim_src = Some(buffer_copy.last_buffer());

                //setup src image and blit
                swapchain_blit.push_image(forward.color_image.clone(), framebuffer_ext);

                rmg.record()
                    .add_task(&mut simulation)
                    .unwrap()
                    .add_task(&mut buffer_copy)
                    .unwrap()
                    .add_task(&mut ubo_update)
                    .unwrap()
                    .add_task(&mut forward)
                    .unwrap()
                    .add_task(&mut swapchain_blit)
                    .unwrap()
                    .execute()
                    .unwrap();

                //*cf = ControlFlow::Exit;
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
