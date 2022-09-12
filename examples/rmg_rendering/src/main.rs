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
use glam::{Quat, Vec3};
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::ImgDesc;
use marpii::{
    ash::{self, vk},
    context::Ctx,
};
use marpii_rmg::graph::TaskRecord;
use marpii_rmg::resources::{ImageKey, BufferKey, BufferHdl};
use marpii_rmg::task::{Attachment, Task, AttachmentType, AccessType};
use marpii_rmg::{Rmg, RmgError};
use std::path::PathBuf;
use winit::event::{DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

//mod forward_pass;
//mod gltf_loader;


pub struct DummyForward{
    attachments: [Attachment; 1],
    buffers: [BufferKey; 2],
    images: [ImageKey; 2],
}

pub struct DummyBackCopy{
    buffers: [BufferKey; 2],
    images: [ImageKey; 2],
}
pub struct DummyPost{
    attachments: [Attachment; 2],
    buffers: [BufferKey; 1],
}
pub struct DummyPreCompute{
    buffers: [BufferKey; 2],
}
pub struct DummyTransfer{
    buffers: [BufferKey; 2],
}


pub(crate) const READATT: Attachment = Attachment{
    ty: AttachmentType::Framebuffer,
    format: vk::Format::R32G32B32A32_SFLOAT,
    access: AccessType::Read,
    access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_READ,
    layout: vk::ImageLayout::ATTACHMENT_OPTIMAL
};
pub(crate) const WRITEATT: Attachment = Attachment{
    ty: AttachmentType::Framebuffer,
    format: vk::Format::R32G32B32A32_SFLOAT,
    access: AccessType::Write,
    access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_READ,
    layout: vk::ImageLayout::ATTACHMENT_OPTIMAL
};

impl Task for DummyForward {
    fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }

    fn buffers(&self) -> &[BufferKey] {
        &self.buffers
    }

    fn images(&self) -> &[ImageKey] {
        &self.images
    }
    fn record(&self, recorder: &mut TaskRecord) {
        println!("Record");
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }
}

impl Task for DummyBackCopy {
    fn buffers(&self) -> &[BufferKey] {
        &self.buffers
    }

    fn images(&self) -> &[ImageKey] {
        &self.images
    }
    fn record(&self, recorder: &mut TaskRecord) {
        println!("Record");
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
}

impl Task for DummyPost {
    fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }

    fn buffers(&self) -> &[BufferKey] {
        &self.buffers
    }
    fn record(&self, recorder: &mut TaskRecord) {
        println!("Record");
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
}
impl Task for DummyPreCompute {
    fn buffers(&self) -> &[BufferKey] {
        &self.buffers
    }
    fn record(&self, recorder: &mut TaskRecord) {
        println!("Record");
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }
}
impl Task for DummyTransfer {
    fn buffers(&self) -> &[BufferKey] {
        &self.buffers
    }
    fn record(&self, recorder: &mut TaskRecord) {
        println!("Record");
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
}

struct App {
    rmg: Rmg,

    forward: DummyForward,
    transfer: DummyTransfer,
    compute: DummyPreCompute,
    back_copy: DummyBackCopy,
    post: DummyPost,
}

impl App {
    pub fn new(window: &winit::window::Window) -> anyhow::Result<Self> {
        //for this test, setup maximum context. We therefore have to activate features needed for rust shaders and
        //dynamicRendering our self.

        //NOTE: By default we setup extensions in a way that we can load rust shaders.
        let vk12 = ash::vk::PhysicalDeviceVulkan12Features::builder()
            //timeline semaphore
            .timeline_semaphore(true)
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

        let vk13 = ash::vk::PhysicalDeviceVulkan13Features::builder()
            //NOTE: used for dynamic rendering based pipelines which are preffered over renderpass based graphics queues.
            .dynamic_rendering(true)
            //NOTE: For timeline semaphores
            .synchronization2(true);

        //NOTE: used for late bind of acceleration structure
        let acceleration_late_bind = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::builder()
            .descriptor_binding_acceleration_structure_update_after_bind(true);


        let (ctx, surface) = Ctx::custom_context(Some(&window), true, |devbuilder| {
            devbuilder
                .push_extensions(ash::extensions::khr::Swapchain::name())
                .push_extensions(ash::vk::KhrVulkanMemoryModelFn::name())
                .push_extensions(ash::extensions::khr::DynamicRendering::name())
                .with(|b| b.features.shader_int16 = 1)
                .with_additional_feature(vk12)
                .with_additional_feature(vk13)
                .with_additional_feature(acceleration_late_bind)
        })?;

        let surface = surface.ok_or(anyhow::anyhow!("Failed to create surface"))?;
        let mut rmg = Rmg::new(ctx, surface)?;

        let back_buffer1: BufferHdl<u64> = rmg.new_buffer(10, Some("BackBuffer1"))?;
        let back_buffer2: BufferHdl<u64> = rmg.new_buffer(10, Some("BackBuffer2"))?;
        let lookup: BufferHdl<u64> = rmg.new_buffer(10, Some("PostLookup"))?;

        let vertexbuffe: BufferHdl<u64> = rmg.new_buffer(10, Some("VertexBuffer"))?;
        let compute_dst: BufferHdl<u64> = rmg.new_buffer(10, Some("ComputeDst"))?;

        let tex1 = rmg.new_image_uninitialized(ImgDesc::texture_2d(1024, 1024, vk::Format::R8G8B8A8_UINT), None, Some("Tex1"))?;
        let tex2 = rmg.new_image_uninitialized(ImgDesc::texture_2d(1024, 1024, vk::Format::R8G8B8A8_UINT), None, Some("Tex2"))?;

        let transfer = DummyTransfer{
            buffers: [back_buffer1.into(), back_buffer2.into()],
        };

        let compute = DummyPreCompute { buffers: [back_buffer2.into(), compute_dst.into()] };
        let back_copy = DummyBackCopy {buffers: [back_buffer2.into(), back_buffer1.into()], images: [tex1.into(), tex2.into()] };
        let forward = DummyForward { attachments: [WRITEATT], buffers: [compute_dst.into(), vertexbuffe.into()], images: [tex1.into(), tex2.into()] };
        let post = DummyPost{attachments: [READATT, WRITEATT], buffers: [lookup.into()] };
        let app = App {
            transfer,
            compute,
            back_copy,
            post,
            forward,
            rmg,
        };

        Ok(app)
    }


    //Enques a new draw event.
    pub fn draw(&mut self) -> Result<(), RmgError> {

        //Builds the following graph:
        //
        // graphics:                      /- forward------post_progress
        //                               /          \
        //                              /            \
        //                             /              \
        // compute: ------compute ----/                \
        //                           /                 |
        //                          /                  |
        //                         /                   |
        // transfer: ----transfer-/                    |-- back_copy

        self.rmg.new_graph()
            .pass(&self.transfer, &[])?
            .pass(&self.compute, &[])?
            .pass(&self.forward, &["forward_dst"])?
            .pass(&self.post, &["forward_dst", "Post"])?
            .pass(&self.back_copy, &[])?
            .present("Post")?;

        Ok(())
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

    println!("Load gltf");
    //let gltf_file = easy_gltf::load(&mesh_path).expect("Failed to load gltf file!");
    let ev = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&ev).unwrap();
    let mut app = App::new(&window)?;



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
                app.draw().unwrap();
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
                        app.rmg.ctx
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
