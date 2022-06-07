//! Simple app that takes one command line argument (the path to a gltf model), and tries to load it as a scene.
//!
//! Other features:
//!
//! - Custom context via `Ctx::custom_context`
//! - DynamicRendering based Graphics pipeline
//! - Simple forward rendering pass
//! - Bindless style texture binding.
//!
use anyhow::Result;
use forward_pass::{ForwardPass, Mesh};
use glam::{Mat4, Quat, Vec3};
use marpii::ash::vk::SamplerMipmapMode;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::{ImgDesc, Sampler, ImageView, SafeImageView};
use marpii::{
    ash::{self, vk, vk::Extent2D},
    context::Ctx,
};
use marpii_command_graph::pass::{ImageBlit, SwapchainPresent, BindlessBind};
use marpii_command_graph::{ExecutionFence, Graph};
use marpii_commands::image::{ImageBuffer, Rgba, Rgb, DynamicImage};
use marpii_commands::image_from_image;
use marpii_descriptor::bindless::{BindlessDescriptor, SampledImageHandle};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use winit::event::{DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

mod forward_pass;
mod gltf_loader;

struct FrameData {
    forward_pass: ForwardPass,
    wait_fence: Option<ExecutionFence>,
}

struct App {
    ctx: Ctx<Allocator>,
    swapchain: SwapchainPresent,
    current_extent: vk::Extent2D,

    frame_data: Vec<FrameData>,

    bindless: BindlessBind,

    //all loaded meshes. We load those once and give a reference to the forward pass
    meshes: Vec<Mesh>,

    graph: Graph,
}

impl App {
    pub fn new(window: &winit::window::Window) -> anyhow::Result<Self> {
        //for this test, setup maximum context. We therefore have to activate features needed for rust shaders and
        //dynamicRendering our self.

        //NOTE: By default we setup extensions in a way that we can load rust shaders.
        let vulkan_memory_model = ash::vk::PhysicalDeviceVulkan12Features::builder()
            //bindless
            .descriptor_binding_partially_bound(true)
            .descriptor_binding_sampled_image_update_after_bind(true)
            .descriptor_binding_storage_buffer_update_after_bind(true)
            .descriptor_binding_storage_image_update_after_bind(true)
            .descriptor_binding_variable_descriptor_count(true)
            .runtime_descriptor_array(true)

            //for Rust-GPU
            .shader_int8(true)
            .vulkan_memory_model(true);
        //NOTE: used for late bind of acceleration structure
        let acceleration_late_bind = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::builder()
            .descriptor_binding_acceleration_structure_update_after_bind(true);

        //NOTE: used for dynamic rendering based pipelines which are preffered over renderpass based graphics queues.
        let dynamic_rendering =
            ash::vk::PhysicalDeviceDynamicRenderingFeatures::builder().dynamic_rendering(true);

        let (ctx, surface) = Ctx::custom_context(Some(&window), true, |devbuilder| {
            devbuilder
                .push_extensions(ash::extensions::khr::Swapchain::name())
                .push_extensions(ash::vk::KhrVulkanMemoryModelFn::name())
                .push_extensions(ash::extensions::khr::DynamicRendering::name())
                .with(|b| b.features.shader_int16 = 1)
                .with_additional_feature(vulkan_memory_model)
                .with_additional_feature(dynamic_rendering)
                .with_additional_feature(acceleration_late_bind)
        })?;

        let graphics_queue = ctx.device.queues[0].clone();
        assert!(graphics_queue
            .properties
            .queue_flags
            .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::TRANSFER));

        let bindless = BindlessDescriptor::new_default(&ctx.device)?;

        let swapchain = SwapchainPresent::new(&ctx.device, Arc::new(surface.unwrap()))?;
        //dummy swapchain image, will be set per recording.

        let extent = swapchain.image_extent();

        //Rebuild passes.
        let frame_data = swapchain
            .swapchain()
            .images
            .iter()
            .map(|_i| {
                let forward_pass = ForwardPass::new(&ctx, extent, bindless.new_pipeline_layout(128)).unwrap();

                FrameData {
                    forward_pass,
                    wait_fence: None,
                }
            })
            .collect();


        let graph = Graph::new(&ctx.device);

        let app = App {
            ctx,
            swapchain,
            graph,
            bindless: BindlessBind::new(bindless, vk::PipelineBindPoint::GRAPHICS),
            current_extent: extent,
            frame_data,
            meshes: Vec::new(),
        };

        Ok(app)
    }

    fn update_meshes(&mut self) {
        for fpass in &mut self.frame_data {
            fpass.forward_pass.objects = self.meshes.clone();
        }
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
            .map(|_i| FrameData {
                forward_pass: ForwardPass::new(&self.ctx, extent, self.bindless.bindless_descriptor.new_pipeline_layout(128)).unwrap(),
                wait_fence: None,
            })
            .collect();

        self.update_meshes();
        self.current_extent = extent;
    }

    fn update_cam(&mut self, cam_pos: Vec3, cam_rot: Quat) {
        for frame in &self.frame_data {
            frame.forward_pass.push_camera(cam_pos, cam_rot);
        }
    }

    //Enques a new draw event.
    pub fn draw(&mut self, window: &winit::window::Window) {
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
        //setup image blit and prepare pass
        let mut blit = ImageBlit::new(
            self.frame_data[self.swapchain.next_index()]
                .forward_pass
                .target_color
                .clone(),
            self.swapchain.next_image().clone(),
        );

        //Build graph and execute
        let execute_fence = self
            .graph
            .record()
            .insert_pass(
                "BindlessBind",
                &mut self.bindless,
                graphics_queue.family_index
            )
            .insert_pass(
                "ForwardPass",
                &mut self.frame_data[self.swapchain.next_index()].forward_pass,
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
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
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
        anyhow::bail!("No gltf path provided!");
    };

    let gltf_file = easy_gltf::load(&mesh_path).expect("Failed to load gltf file!");

    let ev = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&ev).unwrap();

    let mut app = App::new(&window)?;

    for scene in gltf_file {
        println!("Loading scene");

        for model in scene.models {
            println!("Loading mesh with {} verts", model.vertices().len());

            let texture_sampler = Arc::new(Sampler::new(
                &app.ctx.device,
                &vk::SamplerCreateInfo::builder()
                    .mipmap_mode(SamplerMipmapMode::LINEAR)
            ).unwrap());

            //Load albedo texture
            let albedo: ImageBuffer<Rgba<f32>, Vec<f32>> = DynamicImage::from(model.material().pbr.base_color_texture.as_ref().unwrap().deref().clone()).into_rgba32f();
            let albedo_texture = Arc::new(image_from_image(
                &app.ctx.device,
                &app.ctx.allocator,
                app.ctx.device.first_queue_for_attribute(true, false, false).unwrap(),
                vk::ImageUsageFlags::SAMPLED,
                marpii_commands::image::DynamicImage::from(albedo),
            ).unwrap());

            let albedo_view = Arc::new(albedo_texture.view(&app.ctx.device, albedo_texture.view_all()).unwrap());

            let albedo_handle = if let Ok(hdl) = app.bindless.bindless_descriptor.bind_sampled_image(
                albedo_view,
                texture_sampler.clone()
            ){
                hdl
            }else{
                panic!("Couldn't bind!")
            };

            let mesh = Mesh::from_vertex_index_buffers(
                &app.ctx,
                model.vertices(),
                model.indices().expect("Model has no index buffer!"),
                Some(albedo_handle),
                None,
                None
            );
            app.meshes.push(mesh);
        }
    }
    app.update_meshes();

    let mut last_frame = std::time::Instant::now();

    let mut cam_loc = Vec3::new(0.0, 0.0, 2.0);
    let mut cam_rot = Quat::IDENTITY;

    ev.run(move |event, _, ctrl| {
        *ctrl = ControlFlow::Poll;
        let delta = last_frame.elapsed().as_secs_f32();
        match event {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                last_frame = std::time::Instant::now();
                app.update_cam(cam_loc, cam_rot);
                app.draw(&window);
            }
            Event::DeviceEvent {
                event: DeviceEvent::MouseMotion { delta: (x, y) },
                ..
            } => {
                let right = cam_rot.mul_vec3(Vec3::new(1.0, 0.0, 0.0));
                let rot_yaw = Quat::from_rotation_y(x as f32 * 0.001);
                let rot_pitch = Quat::from_axis_angle(right, -y as f32 * 0.001);

                let to_add = rot_yaw * rot_pitch;
                cam_rot = to_add * cam_rot;
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
                    (ElementState::Pressed, VirtualKeyCode::A) => cam_loc.x += 50.0 * delta,
                    (ElementState::Pressed, VirtualKeyCode::D) => cam_loc.x -= 50.0 * delta,
                    (ElementState::Pressed, VirtualKeyCode::E) => cam_loc.y += 50.0 * delta,
                    (ElementState::Pressed, VirtualKeyCode::Q) => cam_loc.y -= 50.0 * delta,
                    (ElementState::Pressed, VirtualKeyCode::S) => cam_loc.z += 50.0 * delta,
                    (ElementState::Pressed, VirtualKeyCode::W) => cam_loc.z -= 50.0 * delta,
                    (ElementState::Pressed, VirtualKeyCode::Escape) => *ctrl = ControlFlow::Exit,
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }
    });
}
