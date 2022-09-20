use anyhow::Result;
use image::EncodableLayout;
use marpii::ash::vk::SamplerMipmapMode;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::{ImageView, ImgDesc, SafeImageView, Sampler};
use marpii::{
    ash::{self, vk, vk::Extent2D},
    context::Ctx,
};
use marpii_rmg::tasks::{SwapchainBlit, UploadImage};
use marpii_rmg::{BufferKey, ImageKey, ResourceRegistry, Resources, Rmg, SamplerKey, Task};
use winit::event::{DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode};
use winit::window::Window;
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

struct ShadowPass {
    shadow: ImageKey,
    param: BufferKey,
    sampler: SamplerKey,
}

impl Task for ShadowPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        registry.request_image(self.shadow);
        registry.request_buffer(self.param);
        registry.request_sampler(self.sampler);
    }
    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        println!("Shadow pass")
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }
    fn name(&self) -> &'static str {
        "ShadowPass"
    }
}

struct ForwardPass {
    shadow: ImageKey,
    target: ImageKey,
    meshes: BufferKey,
}

impl Task for ForwardPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        registry.request_image(self.shadow);
        registry.request_image(self.target);
        registry.request_buffer(self.meshes);
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        println!("Forward pass")
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }
    fn name(&self) -> &'static str {
        "ForwardPass"
    }
}
struct PostPass {
    swimage: ImageKey,
    src: ImageKey,
}

impl Task for PostPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        registry.request_image(self.swimage);
        registry.request_image(self.src);
    }
    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        println!("Post pass")
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }
    fn name(&self) -> &'static str {
        "PostPass"
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

    let image_data = image::open("test.png").unwrap();
    let image_data = image_data.to_rgba32f();

    let swimage_image = rmg.new_image_uninitialized(
        ImgDesc::storage_image_2d(
            image_data.width(),
            image_data.height(),
            vk::Format::R32G32B32A32_SFLOAT,
        ),
        false,
        Some("SwImage"),
    )?;

    let mut init_image = UploadImage::new(swimage_image, image_data.as_bytes());

    //init upload
    rmg.record(window_extent(&window))
        .add_task(&mut init_image, &[])
        .unwrap()
        .execute()
        .unwrap();

    let mut swapchain_blit = SwapchainBlit::new();

    ev.run(move |ev, _, cf| {
        *cf = ControlFlow::Poll;

        match ev {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                //setup src image and blit
                swapchain_blit.next_blit(swimage_image);

                rmg.record(window_extent(&window))
                    .add_task(&mut swapchain_blit, &[])
                    .unwrap()
                    .execute()
                    .unwrap();
            }
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
