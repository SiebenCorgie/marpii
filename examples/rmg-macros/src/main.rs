//! Example that showcases the use of the RMG helper traits and macros.
//! Implements the same application as rmg_rendering does

use std::path::PathBuf;

use anyhow::Result;
use camera_controller::Camera;
use copy_buffer::CopyToGraphicsBuffer;
use easy_gltf::Scene;
use forward_pass::ForwardPass;
use marpii::{ash::vk, context::Ctx};
use marpii_rmg::Rmg;
use marpii_rmg_tasks::{DynamicBuffer, SwapchainPresent};
use shared::Ubo;
use simulation::Simulation;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes};
use winit::{event::WindowEvent, event_loop::ControlFlow};

mod copy_buffer;
mod forward_pass;
mod simulation;

//use the other example's model / camera controlling code
#[path = "../../rmg_rendering/src/camera_controller.rs"]
mod camera_controller;
#[path = "../../rmg_rendering/src/gltf_loader.rs"]
mod gltf_loader;
#[path = "../../rmg_rendering/src/model_loading.rs"]
mod model_loading;

pub const OBJECT_COUNT: usize = 8192;

enum AppState {
    Active {
        window: Window,
        camera: Camera,

        rmg: Rmg,

        ubo_update: DynamicBuffer<Ubo>,
        simulation: Simulation,
        buffer_copy: CopyToGraphicsBuffer,
        forward: ForwardPass,
        swapchain_present: SwapchainPresent,
    },
    Inactive,
}

struct App {
    state: AppState,
    scene: Vec<Scene>,
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let AppState::Active { .. } = self.state {
            log::error!("Already running!");
            return;
        }

        //create a RMG instance for the window, and setup all passes

        let window = event_loop
            .create_window(WindowAttributes::default().with_title("RMG Macros"))
            .unwrap();

        let (context, surface) = Ctx::default_with_surface(&window, true).unwrap();
        let mut rmg = Rmg::new(context).unwrap();

        let camera = Camera::default();
        let ubo_update = DynamicBuffer::new(&mut rmg, &[camera.to_ubo(&window)]).unwrap();
        let simulation = Simulation::new(&mut rmg).unwrap();
        let buffer_copy =
            CopyToGraphicsBuffer::new(&mut rmg, simulation.sim_buffer.clone()).unwrap();
        let forward =
            ForwardPass::new(&mut rmg, ubo_update.buffer_handle().clone(), &self.scene).unwrap();
        let swapchain_present = SwapchainPresent::new(&mut rmg, surface).unwrap();

        self.state = AppState::Active {
            window,
            rmg,
            camera,
            ubo_update,
            simulation,
            buffer_copy,
            forward,
            swapchain_present,
        }
    }

    fn suspended(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        self.state = AppState::Inactive;
    }

    fn device_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        event_loop.set_control_flow(ControlFlow::Poll);
        let meta_event = winit::event::Event::DeviceEvent {
            device_id,
            event: event.clone(),
        };

        if let AppState::Active { camera, .. } = &mut self.state {
            camera.on_event(&meta_event);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        event_loop.set_control_flow(ControlFlow::Poll);

        let meta_event = winit::event::Event::WindowEvent {
            window_id,
            event: event.clone(),
        };

        if let AppState::Active {
            window,
            camera,
            rmg,
            ubo_update,
            simulation,
            buffer_copy,
            forward,
            swapchain_present,
        } = &mut self.state
        {
            camera.on_event(&meta_event);

            match event {
                WindowEvent::RedrawRequested => {
                    camera.tick();
                    //update framebuffer extent to current one.
                    let framebuffer_ext = swapchain_present.extent().unwrap_or(vk::Extent2D {
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
                    swapchain_present.push_image(forward.color_image.clone(), framebuffer_ext);

                    rmg.record()
                        .add_task(simulation)
                        .unwrap()
                        .add_task(buffer_copy)
                        .unwrap()
                        .add_task(ubo_update)
                        .unwrap()
                        .add_task(forward)
                        .unwrap()
                        .add_task(swapchain_present)
                        .unwrap()
                        .execute()
                        .unwrap();

                    window.request_redraw();
                }
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            physical_key: PhysicalKey::Code(KeyCode::Escape),
                            state: ElementState::Pressed,
                            ..
                        },
                    ..
                }
                | WindowEvent::CloseRequested => {
                    rmg.wait_for_idle().unwrap();
                    event_loop.exit();
                }
                _ => {}
            }
        }
    }
}

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

    let ev = winit::event_loop::EventLoop::new().unwrap();
    ev.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        state: AppState::Inactive,
        scene: gltf,
    };

    ev.run_app(&mut app)?;

    Ok(())
}
