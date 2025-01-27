//! Example that shows how to use RMG and the EGUI standard task
//! to render a simple user interface.

use anyhow::Result;
use marpii::context::Ctx;
use marpii::util::FormatProperties;
use marpii_rmg::Rmg;
use marpii_rmg_tasks::winit::event::WindowEvent;
use marpii_rmg_tasks::{egui, EGuiWinitIntegration, SwapchainPresent};

use marpii_rmg_tasks::winit;
use winit::event::Event;

fn main() -> Result<(), anyhow::Error> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();

    let ev = winit::event_loop::EventLoop::builder().build().unwrap();
    let windowattr = winit::window::Window::default_attributes().with_title("Egui Example");
    #[allow(deprecated)]
    let window = ev.create_window(windowattr).unwrap();
    let (context, surface) = Ctx::default_with_surface(&window, true)?;
    let mut rmg = Rmg::new(context)?;

    let mut egui = EGuiWinitIntegration::new(&mut rmg, &ev)?;
    let mut swapchain_blit = SwapchainPresent::new(&mut rmg, surface)?;

    let swapchain_properties = FormatProperties::parse(swapchain_blit.format());
    if swapchain_properties.is_srgb {
        egui.set_gamma(2.2);
    } else {
        egui.set_gamma(1.0);
    }

    let mut name = "Teddy".to_string();
    let mut age = 10u32;

    #[allow(deprecated)]
    let _ = ev.run(move |ev, ev_loop| {
        ev_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        // *cf = ControlFlow::Poll;
        egui.handle_event(&window, &ev);

        match ev {
            Event::LoopExiting => {
                rmg.wait_for_idle().unwrap();
            }
            Event::AboutToWait => window.request_redraw(),
            Event::WindowEvent {
                window_id: _,
                event: WindowEvent::RedrawRequested,
            } => {
                let framebuffer_extent =
                    swapchain_blit
                        .extent()
                        .unwrap_or(marpii::ash::vk::Extent2D {
                            width: window.inner_size().width,
                            height: window.inner_size().height,
                        });

                egui.run(&mut rmg, &window, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.heading("My egui Application");
                        ui.horizontal(|ui| {
                            ui.label("Your name: ");
                            ui.text_edit_singleline(&mut name);
                        });
                        ui.add(egui::Slider::new(&mut age, 0..=120).text("age"));
                        if ui.button("Click each year").clicked() {
                            age += 1;
                        }
                        ui.label(format!("Hello '{}', age {}", name, age));
                    });
                })
                .unwrap();

                //setup src image and blit
                swapchain_blit
                    .push_image(egui.renderer().target_image().clone(), framebuffer_extent);

                rmg.record()
                    .add_meta_task(egui.renderer_mut())
                    .unwrap()
                    .add_task(&mut swapchain_blit)
                    .unwrap()
                    .execute()
                    .unwrap();
            }
            // Event::WindowEvent {
            //     event:
            //         WindowEvent::KeyboardInput {
            //             input:
            //                 KeyboardInput {
            //                     state: ElementState::Pressed,
            //                     virtual_keycode: Some(VirtualKeyCode::Escape),
            //                     ..
            //                 },
            //             ..
            //         },
            //     ..
            // } => ev.clone_from(),
            _ => {}
        }
    });

    Ok(())
}
