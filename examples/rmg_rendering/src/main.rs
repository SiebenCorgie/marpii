use anyhow::Result;
use marpii::ash::vk::SamplerMipmapMode;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::{ImgDesc, Sampler, ImageView, SafeImageView};
use marpii::{
    ash::{self, vk, vk::Extent2D},
    context::Ctx,
};
use marpii_rmg::{Rmg, Task, ResourceAccess, ResourceRegistry, ImageKey, BufferKey, SamplerKey};
use winit::event::{DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};


struct ShadowPass{
    shadow: ImageKey,
    param: BufferKey,
    sampler: SamplerKey
}

impl Task for ShadowPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        registry.request_image(self.shadow);
        registry.request_buffer(self.param);
        registry.request_sampler(self.sampler);
    }
    fn record(&mut self, device: &std::sync::Arc<marpii::context::Device>, command_buffer: &vk::CommandBuffer, resources: &ResourceAccess) {
        println!("Shadow pass")
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }
    fn name(&self) -> &'static str {
        "ShadowPass"
    }
}

struct ForwardPass{
    shadow: ImageKey,
    target: ImageKey,
    meshes: BufferKey
}

impl Task for ForwardPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        registry.request_image(self.shadow);
        registry.request_image(self.target);
        registry.request_buffer(self.meshes);
    }

    fn record(&mut self, device: &std::sync::Arc<marpii::context::Device>, command_buffer: &vk::CommandBuffer, resources: &ResourceAccess) {
        println!("Forward pass")
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }
    fn name(&self) -> &'static str {
        "ForwardPass"
    }
}
struct PostPass{
    swimage: ImageKey,
    src: ImageKey
}

impl Task for PostPass {
    fn register(&self, registry: &mut ResourceRegistry) {
        registry.request_image(self.swimage);
        registry.request_image(self.src);
    }
    fn record(&mut self, device: &std::sync::Arc<marpii::context::Device>, command_buffer: &vk::CommandBuffer, resources: &ResourceAccess) {
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

    let mesh_buffer = rmg.new_buffer::<usize>(1024, Some("MeshBuffer"))?;
    let param_buffer = rmg.new_buffer::<usize>(1024, Some("ParamBuffer"))?;

    let swimage_image = rmg.new_image_uninitialized(
        ImgDesc::storage_image_2d(1024, 1024, vk::Format::R8G8B8A8_UINT),
        false,
        Some("SwImage")
    )?;
    let shadow_image = rmg.new_image_uninitialized(
        ImgDesc::texture_2d(1024, 1024, vk::Format::R8G8B8A8_UINT),
        false,
        Some("ShadowImage")
    )?;
    let target_image = rmg.new_image_uninitialized(
        ImgDesc::storage_image_2d(1024, 1024, vk::Format::R8G8B8A8_UINT),
        false,
        Some("TargetImage")
    )?;
    let sampled_image = rmg.new_image_uninitialized(
        ImgDesc::texture_2d(1024, 1024, vk::Format::R8G8B8A8_UINT),
        true,
        Some("SampledImage")
    )?;

    let sampler = rmg.new_sampler(&vk::SamplerCreateInfo::builder())?;

    let shadow_pass = ShadowPass{
        shadow: shadow_image,
        sampler,
        param: param_buffer
    };

    let forward = ForwardPass{
        shadow: shadow_image,
        target: target_image,
        meshes: mesh_buffer
    };

    let post = PostPass{
        src: target_image,
        swimage: swimage_image
    };

    rmg.record()
        .add_task(&shadow_pass, &["ShadowImg"])?
        .add_task(&forward, &["ForwardImg"])?
        .add_task(&post, &[])?
        .execute()?;


    Ok(())
}
