use std::path::PathBuf;

use anyhow::Result;
use marpii::{ash::vk, context::Ctx};
use marpii_rmg_tasks::egui::epaint::{Primitive, Vertex};
use marpii_rmg_tasks::egui::{ClippedPrimitive, Rect, Mesh, Color32, CentralPanel, SidePanel, ScrollArea};
use marpii_rmg_tasks::{DynamicBuffer, SwapchainBlit, EGuiRender, EGuiWinitIntegration};
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
        .with_level(log::LevelFilter::Trace)
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
                    SidePanel::left("my_side_panel").show(ctx, |ui| {
                        ui.heading("Hello World!");
                        if ui.button("Quit").clicked() {
                            quit = true;
                        }
                    });

                    CentralPanel::default().show(ctx, |ui| {
                        ScrollArea::vertical().show(ui, |ui| {
                            ui.heading("Ello teddy")
                        });
                    });
                }).unwrap();

                //setup src image and blit
                swapchain_blit.next_blit(egui.target_image().clone());

                rmg.record(window_extent(&window))
                    .add_task(egui.renderer())
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
