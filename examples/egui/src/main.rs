use std::path::PathBuf;

use anyhow::Result;
use marpii::resources::ImgDesc;
use marpii::{ash::vk, context::Ctx};
use marpii_rmg_tasks::egui::epaint::{Primitive, Vertex};
use marpii_rmg_tasks::egui::{ClippedPrimitive, Rect, Mesh, Color32, CentralPanel, SidePanel, ScrollArea};
use marpii_rmg_tasks::{DynamicBuffer, SwapchainBlit, EGuiRender, EGuiWinitIntegration, egui, ImageBlit};
use marpii_rmg::Rmg;

use winit::event::{ElementState, KeyboardInput, VirtualKeyCode};
use winit::window::Window;
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};


pub const OBJECT_COUNT: usize = 8192;



///```ignore
///
///  0         1
///  x---------x
///  |         |
///  |         |
///  |         |
///  |         |
///  x---------x
///  2          3
///```
fn quat() -> ClippedPrimitive{
    ClippedPrimitive{
        clip_rect: Rect::EVERYTHING,
        primitive: Primitive::Mesh(Mesh{
            indices: vec![
                0,1,2,
                1,2,3
            ],
            vertices: vec![
                Vertex{
                    pos: [-100.0, -100.0].into(),
                    uv: [0.0, 0.0].into(),
                    color: Color32::BLACK
                },
                Vertex{
                    pos: [100.0, -100.0].into(),
                    uv: [1.0, 0.0].into(),
                    color: Color32::BLUE
                },
                Vertex{
                    pos: [-100.0, 100.0].into(),
                    uv: [0.0, 1.0].into(),
                    color: Color32::GREEN
                },
                Vertex{
                    pos: [100.0, 100.0].into(),
                    uv: [1.0, 1.0].into(),
                    color: Color32::RED
                },
            ],
            ..Default::default()
        })
    }
}

fn main() -> Result<(), anyhow::Error> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    let ev = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&ev).unwrap();
    let (context, surface) = Ctx::default_with_surface(&window, true)?;
    let mut rmg = Rmg::new(context, &surface)?;

    let mut egui = EGuiWinitIntegration::new(&mut rmg, &ev)?;

    let mut swapchain_blit = SwapchainBlit::new();

    let mut name = "Teddy".to_string();
    let mut age = 10u32;

    ev.run(move |ev, _, cf| {
        *cf = ControlFlow::Poll;
        let mut quit = false;
        egui.handle_event(&ev);
        match ev {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                egui.run(&mut rmg, &window, |ctx|{
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
                }).unwrap();

                //setup src image and blit
                swapchain_blit.next_blit(egui.renderer().target_image().clone());

                let recorder = rmg.record(window_extent(&window))
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

        if quit{
            *cf = ControlFlow::Exit;
        }
    })
}

fn window_extent(window: &Window) -> vk::Extent2D {
    vk::Extent2D {
        width: window.inner_size().width,
        height: window.inner_size().height,
    }
}
