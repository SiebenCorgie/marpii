//! Example that shows how to use RMG and the EGUI standard task
//! to render a simple user interface.

use marpii::util::FormatProperties;
use marpii_rmg::Rmg;
use marpii_rmg_tasks::winit::event::WindowEvent;
use marpii_rmg_tasks::{EGuiWinitIntegration, SwapchainPresent, egui};

use marpii_rmg_tasks::winit;

enum App {
    Idle,
    Running {
        rmg: Rmg,
        egui: EGuiWinitIntegration,
        swapchain: SwapchainPresent,
        window: winit::window::Window,
        name: String,
        age: u32,
    },
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Self::Running { .. } = self {
            log::error!("Already running!");
            return;
        }

        let windowattr = winit::window::Window::default_attributes().with_title("Egui Example");
        let window = event_loop.create_window(windowattr).unwrap();

        let mut rmg = Rmg::init_for_window(&window).unwrap();
        let surface = rmg.create_surface(&window).unwrap();

        let mut egui = EGuiWinitIntegration::new(&mut rmg, &event_loop).unwrap();
        let swapchain = SwapchainPresent::new(&mut rmg, surface).unwrap();

        let swapchain_properties = FormatProperties::parse(swapchain.format());
        if swapchain_properties.is_srgb {
            egui.set_gamma(2.2);
        } else {
            egui.set_gamma(1.0);
        }

        let name = "Teddy".to_string();
        let age = 10u32;

        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        *self = Self::Running {
            rmg,
            egui,
            swapchain,
            window,
            name,
            age,
        };
    }

    fn suspended(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        *self = Self::Idle
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        let Self::Running { window, .. } = self else {
            return;
        };
        //Currently using polling, a UI-First app might do some kind of state
        // tracking and only request redraws, if the UI actually changed.
        window.request_redraw();
    }

    fn device_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        let Self::Running { egui, window, .. } = self else {
            return;
        };

        egui.handle_event::<()>(
            &window,
            &winit::event::Event::DeviceEvent {
                device_id,
                event: event.clone(),
            },
        );

        if egui.needs_redraw() {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Self::Running {
            rmg,
            egui,
            swapchain,
            window,
            name,
            age,
        } = self
        else {
            log::warn!("Not running!");
            return;
        };

        egui.handle_event::<()>(
            &window,
            &winit::event::Event::WindowEvent {
                window_id,
                event: event.clone(),
            },
        );

        if egui.needs_redraw() {
            window.request_redraw();
        }

        match event {
            WindowEvent::RedrawRequested => {
                let framebuffer_extent = swapchain.extent().unwrap_or(marpii::ash::vk::Extent2D {
                    width: window.inner_size().width,
                    height: window.inner_size().height,
                });

                egui.run(rmg, &window, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.heading("My egui Application");
                        ui.horizontal(|ui| {
                            ui.label("Your name: ");
                            ui.text_edit_singleline(name);
                        });
                        ui.add(egui::Slider::new(age, 0u32..=120u32).text("age"));
                        if ui.button("Click each year").clicked() {
                            *age += 1;
                        }
                        ui.label(format!("Hello '{}', age {}", name, age));
                    });
                })
                .unwrap();

                //setup src image and blit
                swapchain.push_image(egui.renderer().target_image().clone(), framebuffer_extent);

                rmg.record()
                    .add_meta_task(egui.renderer_mut())
                    .unwrap()
                    .add_task(swapchain)
                    .unwrap()
                    .execute()
                    .unwrap();
            }
            WindowEvent::CloseRequested => {
                rmg.wait_for_idle().expect("Failed to wait for idle!");
                event_loop.exit()
            }
            _ => {}
        }
    }
}

fn main() {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    let ev = winit::event_loop::EventLoop::new().unwrap();
    ev.run_app(&mut App::Idle)
        .expect("failed to run event loop");
}
