//! Simple app that takes one command line argument (the path to a gltf model), and tries to load it as a scene.
//!
//! Other features:
//!
//! - Custom context via `Ctx::custom_context`
//! - DynamicRendering based Graphics pipeline
//! - Simple forward rendering pass
//! - Bindless style texture binding.
//!

#[deny(warnings)]
use anyhow::Result;
use forward_pass::{ForwardPass, Mesh};
use glam::{Quat, Vec3};
use marpii::ash::vk::SamplerMipmapMode;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::{SafeImageView, Sampler};
use marpii::{
    ash::{self, vk, vk::Extent2D},
    context::Ctx,
};
use marpii_commands::image::{DynamicImage, ImageBuffer, Rgba};
use marpii_commands::image_from_image;
use std::path::PathBuf;
use std::sync::Arc;
use winit::event::{DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

mod forward_pass;
mod gltf_loader;


struct App {
    ctx: Ctx<Allocator>,
    meshes: Vec<Mesh>,
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
//dummy swapchain image, will be set per recording.
        let app = App {
            ctx,
            meshes: Vec::new(),
        };

        Ok(app)
    }


    //Enques a new draw event.
    pub fn draw(&mut self, window: &winit::window::Window) {
        panic!("No frame building yet!");
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
/*
    for scene in gltf_file {
        println!("Loading scene");

        for model in scene.models {
            println!("Loading mesh with {} verts", model.vertices().len());

            let texture_sampler = Arc::new(
                Sampler::new(
                    &app.ctx.device,
                    &vk::SamplerCreateInfo::builder().mipmap_mode(SamplerMipmapMode::LINEAR),
                )
                .unwrap(),
            );

            //Load albedo texture
            let albedo: ImageBuffer<Rgba<f32>, Vec<f32>> = DynamicImage::from(
                model
                    .material()
                    .pbr
                    .base_color_texture
                    .as_ref()
                    .unwrap()
                    .deref()
                    .clone(),
            )
            .into_rgba32f();
            let albedo_texture = Arc::new(
                image_from_image(
                    &app.ctx.device,
                    &app.ctx.allocator,
                    app.ctx
                        .device
                        .first_queue_for_attribute(true, false, false)
                        .unwrap(),
                    vk::ImageUsageFlags::SAMPLED,
                    marpii_commands::image::DynamicImage::from(albedo),
                )
                .unwrap(),
            );

            let albedo_view = Arc::new(
                albedo_texture
                    .view(&app.ctx.device, albedo_texture.view_all())
                    .unwrap(),
            );

            let albedo_handle = if let Ok(hdl) = app
                .bindless
                .bindless_descriptor
                .bind_sampled_image(albedo_view, texture_sampler.clone())
            {
                hdl
            } else {
                panic!("Couldn't bind!")
            };

            let mesh = Mesh::from_vertex_index_buffers(
                &app.ctx,
                model.vertices(),
                model.indices().expect("Model has no index buffer!"),
                Some(albedo_handle),
                None,
                None,
            );
            app.meshes.push(mesh);
        }
    }
    app.update_meshes();
*/
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
                //app.update_cam(cam_loc, cam_rot);
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
