use anyhow::Result;
use marpii::context::Ctx;
use marpii_rmg::Rmg;
use marpii_rmg_tasks::{egui, EGuiWinitIntegration, SwapchainPresent};

use winit::event::{ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

fn main() -> Result<(), anyhow::Error> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    let ev = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&ev).unwrap();
    let (context, surface) = Ctx::default_with_surface(&window, true)?;
    let mut rmg = Rmg::new(context)?;

    let mut egui = EGuiWinitIntegration::new(&mut rmg, &ev)?;

    let mut swapchain_blit = SwapchainPresent::new(&mut rmg, &surface)?;

    let mut name = "Teddy".to_string();
    let mut age = 10u32;

    ev.run(move |ev, _, cf| {
        *cf = ControlFlow::Poll;
        egui.handle_event(&ev);
        match ev {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
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
                swapchain_blit.push_image(
                    egui.renderer().target_image().clone(),
                    swapchain_blit
                        .extent()
                        .unwrap_or(marpii::ash::vk::Extent2D {
                            width: window.inner_size().width,
                            height: window.inner_size().height,
                        }),
                );

                rmg.record()
                    .add_task(egui.renderer_mut())
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
