//! # Render Graph
//! Similar function to the hello_triangle example, but uses the marpii-command-graph crate to handle
//! resource state and state transition.

///Collects all runtime state for the application. Basically the context, swapchain and pipeline used for drawing.
use anyhow::Result;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::{Image, ImgDesc};
use marpii::{
    ash::{self, vk, vk::Extent2D},
    context::Ctx,
    swapchain::Swapchain,
};
use marpii_command_graph::pass::{ImageBlit, SwapchainPrepare, WaitExternal};
use marpii_command_graph::{Graph, StImage};
use winit::event::{ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

mod compute_pass;
use compute_pass::{ComputeDispatch, PushConst};

struct App {
    ctx: Ctx<Allocator>,
    swapchain: Swapchain,
    current_extent: vk::Extent2D,

    target_images: Vec<StImage>,

    compute_pass: ComputeDispatch,
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

        let swapchain = Swapchain::builder(&ctx.device, &surface)?
            .with(|b| {
                b.usage = ash::vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | ash::vk::ImageUsageFlags::TRANSFER_DST
            })
            .build()?;

        //dummy swapchain image, will be set per recording.
        let swimg = swapchain.images[0].clone();
        let extent = swimg.extent_2d();

        //Rebuild passes.
        let compute_pass = ComputeDispatch::new(&ctx, &swapchain);

        let target_images = swapchain
            .images
            .iter()
            .map(|_i| {
                StImage::unitialized(
                    Image::new(
                        &ctx.device,
                        &ctx.allocator,
                        ImgDesc::color_attachment_2d(
                            extent.width,
                            extent.height,
                            ash::vk::Format::R8G8B8A8_UNORM,
                        )
                        .add_usage(ash::vk::ImageUsageFlags::TRANSFER_SRC)
                        .add_usage(ash::vk::ImageUsageFlags::STORAGE),
                        marpii::allocator::MemoryUsage::GpuOnly,
                        Some("TargetImage"),
                        None,
                    )
                    .unwrap(),
                )
            })
            .collect();

        let app = App {
            ctx,
            swapchain,
            current_extent: extent,
            target_images,
            compute_pass,
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
        self.swapchain.recreate(extent).unwrap();

        //Rebuild images
        self.target_images = self
            .swapchain
            .images
            .iter()
            .map(|_i| {
                StImage::unitialized(
                    Image::new(
                        &self.ctx.device,
                        &self.ctx.allocator,
                        ImgDesc::color_attachment_2d(
                            extent.width,
                            extent.height,
                            ash::vk::Format::R8G8B8A8_UNORM,
                        )
                        .add_usage(ash::vk::ImageUsageFlags::TRANSFER_SRC)
                        .add_usage(ash::vk::ImageUsageFlags::STORAGE),
                        marpii::allocator::MemoryUsage::GpuOnly,
                        Some("TargetImage"),
                        None,
                    )
                    .unwrap(),
                )
            })
            .collect();

        self.current_extent = extent;
    }
    //Enques a new draw event.
    pub fn draw(&mut self, window: &winit::window::Window, push: PushConst) {
        let extent = self
            .swapchain
            .surface
            .get_capabilities(self.ctx.device.physical_device)
            .unwrap()
            .current_extent;
        //if on wayland this will be wrong, therfore sanitize
        let extent = match extent {
            Extent2D {
                width: 0xFFFFFFFF,
                height: 0xFFFFFFFF,
            } => {
                //Choose based on the window.
                //Todo make robust agains hidpi scaling
                Extent2D {
                    width: window.inner_size().width,
                    height: window.inner_size().height,
                }
            }
            Extent2D { width, height } => Extent2D { width, height },
        };

        //Check if size still ok, otherwise resize
        let swext = self.swapchain.images[0].extent_2d();

        if swext != extent || self.current_extent != swext {
            self.resize(extent);
        }

        let graphics_queue = self.ctx.device.queues[0].clone();

        //Get next image and wrap it into a managed StImage
        let swimage = self.swapchain.acquire_next_image().unwrap();
        let st_swimage = StImage::shared(
            swimage.image.clone(),
            graphics_queue.family_index,
            vk::AccessFlags::empty(),
            vk::ImageLayout::UNDEFINED,
        );

        //Build new frame graph and submit

        //setup wait pass
        let wait_image = WaitExternal::new(swimage.sem_acquire.clone());
        //Setup compute pass
        let compute_submit = self.compute_pass.for_image(
            &self.ctx.device,
            self.target_images[swimage.index as usize].clone(),
        );
        compute_submit.push_const(push);

        //setup image blit and prepare pass
        let blit = ImageBlit::new(compute_submit.target_image.clone(), st_swimage.clone());
        //setup prepare including the seamphore that is signaled once the pass has finished.
        let present_prepare = SwapchainPrepare::new(st_swimage, swimage.sem_present.clone());

        //Rebuild graph
        let mut graph = Graph::new(&self.ctx.device)
            .insert_pass("ImageAcquireWait", wait_image, graphics_queue.family_index)
            .insert_pass("ComputePass", compute_submit, graphics_queue.family_index)
            .insert_pass("SwapchainBlit", blit, graphics_queue.family_index)
            .insert_pass(
                "SwapchainPrepare",
                present_prepare,
                graphics_queue.family_index,
            )
            .build()
            .unwrap();

        graph.submit().unwrap();

        //now enqueue for present
        if let Err(e) = self
            .swapchain
            .present_image(swimage, &self.ctx.device.queues[0].inner)
        {
            println!("Present error: {}", e);
        }
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
