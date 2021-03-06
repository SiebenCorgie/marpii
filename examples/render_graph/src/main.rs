//! # Render Graph
//! Similar function to the hello_triangle example, but uses the marpii-command-graph crate to handle
//! resource state and state transition.

///Collects all runtime state for the application. Basically the context, swapchain and pipeline used for drawing.
use anyhow::Result;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::{
    ash::{vk, vk::Extent2D},
    context::Ctx,
};
use marpii_command_graph::pass::{ImageBlit, SwapchainPresent};
use marpii_command_graph::{ExecutionFence, Graph};
use winit::event::{ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

mod compute_pass;
use compute_pass::{ComputeDispatch, PushConst};

struct FrameData {
    compute_pass: ComputeDispatch,
    wait_fence: Option<ExecutionFence>,
}

struct App {
    ctx: Ctx<Allocator>,
    swapchain: SwapchainPresent,
    current_extent: vk::Extent2D,

    frame_data: Vec<FrameData>,

    graph: Graph,
}

impl App {
    pub fn new(window: &winit::window::Window) -> anyhow::Result<Self> {
        //now test context setup
        let (ctx, surface) = Ctx::default_with_surface(&window, true)?;

        let graphics_queue = ctx.device.queues[0].clone();
        assert!(graphics_queue
            .properties
            .queue_flags
            .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::TRANSFER));

        let swapchain = SwapchainPresent::new(&ctx.device, surface)?;
        //dummy swapchain image, will be set per recording.

        let extent = swapchain.image_extent();
        //Rebuild passes.
        let frame_data = swapchain
            .swapchain()
            .images
            .iter()
            .map(|_i| {
                let compute_pass = ComputeDispatch::new(&ctx, extent);

                FrameData {
                    compute_pass,
                    wait_fence: None,
                }
            })
            .collect();

        let graph = Graph::new(&ctx.device);

        let app = App {
            ctx,
            swapchain,
            graph,
            current_extent: extent,
            frame_data,
        };

        Ok(app)
    }

    //Called if resizing needs to take place
    pub fn resize(&mut self, extent: Extent2D) {
        unsafe {
            self.ctx
                .device
                .inner
                .device_wait_idle()
                .expect("Could not wait for idle")
        };

        //Resize swapchain. Initial transition of the images will be handled by the
        // pass data.
        self.swapchain.resize(extent);

        println!("Resizing to: {extent:?}");
        //Rebuild images
        self.frame_data = self
            .swapchain
            .swapchain()
            .images
            .iter()
            .map(|_i| {
                let compute_pass = ComputeDispatch::new(&self.ctx, extent);
                FrameData {
                    compute_pass,
                    wait_fence: None,
                }
            })
            .collect();

        self.current_extent = extent;
    }
    //Enques a new draw event.
    pub fn draw(&mut self, window: &winit::window::Window, push: PushConst) {
        let extent = self.swapchain.current_extent();
        //if on wayland this will be wrong, therfore sanitize
        let extent = if let Some(ext) = extent {
            ext
        } else {
            //Choose based on the window.
            //Todo make robust agains hidpi scaling
            Extent2D {
                width: window.inner_size().width,
                height: window.inner_size().height,
            }
        };

        //Check if size still ok, otherwise resize
        let swext = self.swapchain.image_extent();

        if swext != extent || self.current_extent != swext {
            self.resize(extent);
        }

        let graphics_queue = self
            .ctx
            .device
            .first_queue_for_attribute(true, false, false)
            .unwrap();

        //wait for the frame data to become valid again
        if let Some(fence) = self.frame_data[self.swapchain.next_index()]
            .wait_fence
            .take()
        {
            fence.wait();
        }

        //Build new frame graph and submit
        //Setup compute pass
        self.frame_data[self.swapchain.next_index()]
            .compute_pass
            .push_const(push);

        //setup image blit and prepare pass
        let mut blit = ImageBlit::new(
            self.frame_data[self.swapchain.next_index()]
                .compute_pass
                .target_image
                .clone(),
            self.swapchain.next_image().clone(),
        );

        //Build graph and execute
        let execute_fence = self
            .graph
            .record()
            .insert_pass(
                "ComputePass",
                &mut self.frame_data[self.swapchain.next_index()].compute_pass,
                graphics_queue.family_index,
            )
            .insert_pass("SwapchainBlit", &mut blit, graphics_queue.family_index)
            .insert_pass(
                "SwapchainPresent",
                &mut self.swapchain,
                graphics_queue.family_index,
            )
            .finish()
            .execute()
            .unwrap();
        //Update fence for new submit
        self.frame_data[self.swapchain.next_index()].wait_fence = Some(execute_fence);
    }
}

fn main() -> Result<(), anyhow::Error> {
    simple_logger::SimpleLogger::new().init().unwrap();

    let ev = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&ev).unwrap();

    let mut app = App::new(&window)?;

    let mut rad = 45.0f32;
    let mut offset = [500.0, 500.0];

    let start = std::time::Instant::now();

    ev.run(move |event, _, ctrl| {
        *ctrl = ControlFlow::Poll;

        match event {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                rad = start.elapsed().as_secs_f32().sin() + 1.0;
                app.draw(
                    &window,
                    PushConst {
                        radius: 450.0,
                        opening: rad,
                        offset,
                    },
                );
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *ctrl = ControlFlow::Exit;
                    unsafe {
                        app.ctx
                            .device
                            .inner
                            .device_wait_idle()
                            .expect("Failed to wait")
                    };
                }
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state,
                            virtual_keycode: Some(kc),
                            ..
                        },
                    ..
                } => match (state, kc) {
                    (ElementState::Pressed, VirtualKeyCode::A) => offset[0] += 10.0,
                    (ElementState::Pressed, VirtualKeyCode::D) => offset[0] -= 10.0,
                    (ElementState::Pressed, VirtualKeyCode::W) => offset[1] += 10.0,
                    (ElementState::Pressed, VirtualKeyCode::S) => offset[1] -= 10.0,
                    (ElementState::Pressed, VirtualKeyCode::Escape) => *ctrl = ControlFlow::Exit,
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }

        rad = rad.clamp(1.0, 179.0);
    });
}
