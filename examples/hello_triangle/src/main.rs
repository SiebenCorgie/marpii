//! A simple marpii application that uses marpii's `Ctx` abstraction to automatically create a context for a window.
//! For each frame a compute shader is executed that writes to a swapchain image.

use anyhow::Result;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::{CommandBufferAllocator, CommandPool, ComputePipeline, DescriptorPool};
use marpii::OoS;
use marpii::{
    ash::{
        self,
        vk::{Extent2D, Offset3D},
    },
    context::Ctx,
    resources::{Image, ImgDesc, PipelineLayout, PushConstant, SafeImageView, ShaderModule},
    swapchain::{Swapchain, SwapchainImage},
};
use marpii_commands::ManagedCommands;
use marpii_descriptor::managed_descriptor::{Binding, ManagedDescriptorSet};
use std::sync::{Arc, Mutex};
use winit::event::{ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

const SHADER: &[u8] = include_bytes!("../resources/rust_shader.spv");

#[repr(C)]
pub struct PushConst {
    radius: f32,
    opening: f32,
    offset: [f32; 2],
}

struct PassData {
    //image that is rendered to
    image: OoS<Image>,

    command_buffer: ManagedCommands,

    descriptor_set: Arc<ManagedDescriptorSet>,

    pipeline: Arc<ComputePipeline>,
    push_constant: Arc<Mutex<PushConstant<PushConst>>>,

    is_first_time: bool,
}

impl PassData {
    pub fn new(ctx: &Ctx<Allocator>, width: u32, height: u32) -> Result<Self, anyhow::Error> {
        println!("Recreate image for: {}x{}", width, height);

        let mut image = OoS::new(Image::new(
            &ctx.device,
            &ctx.allocator,
            ImgDesc::color_attachment_2d(width, height, ash::vk::Format::R8G8B8A8_UNORM)
                .add_usage(ash::vk::ImageUsageFlags::TRANSFER_SRC)
                .add_usage(ash::vk::ImageUsageFlags::STORAGE),
            marpii::allocator::MemoryUsage::GpuOnly,
            Some("RenderTarget"),
        )?);
        let image_view = Arc::new(image.share().view(image.view_all())?);

        let push_constant = Arc::new(Mutex::new(PushConstant::new(
            PushConst {
                offset: [500.0, 500.0],
                opening: (10.0f32).to_radians(),
                radius: 450.0,
            },
            ash::vk::ShaderStageFlags::COMPUTE,
        )));

        //load shader from file
        let shader_module = ShaderModule::new_from_bytes(&ctx.device, SHADER)?;

        let descriptor_set_layouts = shader_module.create_descriptor_set_layouts()?;

        let descriptor_set = {
            //NOTE bad practise, should be done per app.
            let pool = DescriptorPool::new_for_module(
                &ctx.device,
                ash::vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET,
                &shader_module,
                1,
            )?;

            let set = ManagedDescriptorSet::new(
                &ctx.device,
                pool.into(),
                [Binding::new_image(
                    image_view,
                    ash::vk::ImageLayout::GENERAL,
                )],
                ash::vk::ShaderStageFlags::ALL,
            )?;

            Arc::new(set)
        };

        let pipeline = {
            let pipeline_layout = PipelineLayout::from_layout_push(
                &ctx.device,
                &descriptor_set_layouts,
                &push_constant.lock().unwrap(),
            )
            .unwrap();

            let pipeline = ComputePipeline::new(
                &ctx.device,
                &shader_module
                    .into_shader_stage(ash::vk::ShaderStageFlags::COMPUTE, "main".to_owned()),
                None,
                pipeline_layout,
            )?;

            Arc::new(pipeline)
        };

        //Time to create the command buffer and descriptor set
        let cb = {
            let command_pool = CommandPool::new(
                &ctx.device,
                ctx.device.queues[0].family_index,
                ash::vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
            )?;

            let command_buffer =
                OoS::new(command_pool).allocate_buffer(ash::vk::CommandBufferLevel::PRIMARY)?;

            ManagedCommands::new(&ctx.device, command_buffer)?
        };

        Ok(PassData {
            command_buffer: cb,
            image,
            pipeline,
            push_constant,

            descriptor_set,

            is_first_time: true,
        })
    }

    ///Records a new command buffer that renders to `image` and blits it to the swapchain's `image_idx` image.
    pub fn record(
        &mut self,
        ctx: &Ctx<Allocator>,
        swapchain_image: &SwapchainImage,
    ) -> Result<(), anyhow::Error> {
        //For now define what queue family we are on. This should usually be checked.
        let queue_graphics_family = ctx.device.queues[0].family_index;

        //resets and starts command buffer
        let mut recorder = self.command_buffer.start_recording()?;

        recorder.record({
            let pipe = self.pipeline.clone();
            move |dev, cmd| unsafe {
                dev.cmd_bind_pipeline(*cmd, ash::vk::PipelineBindPoint::COMPUTE, pipe.pipeline)
            }
        });

        recorder.record({
            let pipe = self.pipeline.clone();
            let descset = self.descriptor_set.clone();
            move |dev, cmd| unsafe {
                dev.cmd_bind_descriptor_sets(
                    *cmd,
                    ash::vk::PipelineBindPoint::COMPUTE,
                    pipe.layout.layout,
                    0,
                    &[*descset.raw()],
                    &[],
                );
            }
        });

        recorder.record({
            let pipe = self.pipeline.clone();
            let push = self.push_constant.clone();
            move |dev, cmd| unsafe {
                dev.cmd_push_constants(
                    *cmd,
                    pipe.layout.layout,
                    ash::vk::ShaderStageFlags::COMPUTE,
                    0,
                    push.lock().unwrap().content_as_bytes(),
                )
            }
        });

        let ext = swapchain_image.image.extent_2d();
        //now submit for the extend. We know that the shader is executing in 8x8x1, therefore conservatifly use the dispatch size.
        let submit_size = [
            (ext.width as f32 / 8.0).ceil() as u32,
            (ext.height as f32 / 8.0).ceil() as u32,
            1,
        ];

        if self.is_first_time {
            //Since this is the record for first time submit:
            //Move the attachment image and the swapchain image from undefined to shader_write / transfer_dst
            recorder.record({
                let swimg = swapchain_image.image.clone();
                let image = self.image.share();

                move |dev, cmd| unsafe {
                    dev.cmd_pipeline_barrier(
                        *cmd,
                        ash::vk::PipelineStageFlags::TOP_OF_PIPE,
                        ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                        ash::vk::DependencyFlags::empty(),
                        &[], //mem
                        &[], //buffer
                        &[
                            //Transfer attachment image from UNDEFINED to SHADER_WRITE
                            ash::vk::ImageMemoryBarrier {
                                image: image.inner,
                                src_access_mask: ash::vk::AccessFlags::NONE,
                                dst_access_mask: ash::vk::AccessFlags::NONE,
                                old_layout: ash::vk::ImageLayout::UNDEFINED,
                                new_layout: ash::vk::ImageLayout::GENERAL,
                                subresource_range: image.subresource_all(),
                                src_queue_family_index: queue_graphics_family,
                                dst_queue_family_index: queue_graphics_family,
                                ..Default::default()
                            },
                            //Move swapchain image to presetn src, since the later barrier will move it into transfer
                            //dst assuming it was on present src khr.
                            ash::vk::ImageMemoryBarrier {
                                image: swimg.inner,
                                src_access_mask: ash::vk::AccessFlags::NONE,
                                dst_access_mask: ash::vk::AccessFlags::NONE,
                                old_layout: ash::vk::ImageLayout::UNDEFINED,
                                new_layout: ash::vk::ImageLayout::PRESENT_SRC_KHR,
                                subresource_range: swimg.subresource_all(),
                                src_queue_family_index: queue_graphics_family,
                                dst_queue_family_index: queue_graphics_family,
                                ..Default::default()
                            },
                        ],
                    )
                }
            });

            //null flag to not do this again.
            self.is_first_time = false;
        }

        //actual dispatch.
        recorder.record({
            move |dev, cmd| unsafe {
                dev.cmd_dispatch(*cmd, submit_size[0], submit_size[1], submit_size[2]);
            }
        });

        //Issue a barrier to wait for the compute shader and move the images to transfer src/dst
        recorder.record({
            let img = self.image.share();
            let swimg = swapchain_image.image.clone();
            move |dev, cmd| unsafe {
                dev.cmd_pipeline_barrier(
                    *cmd,
                    ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                    ash::vk::PipelineStageFlags::TRANSFER,
                    ash::vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[
                        ash::vk::ImageMemoryBarrier {
                            image: img.inner,
                            src_access_mask: ash::vk::AccessFlags::NONE,
                            dst_access_mask: ash::vk::AccessFlags::NONE,
                            old_layout: ash::vk::ImageLayout::GENERAL,
                            new_layout: ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                            subresource_range: img.subresource_all(),
                            ..Default::default()
                        },
                        //Move swapchain image to transfer dst from present layout
                        ash::vk::ImageMemoryBarrier {
                            image: swimg.inner,
                            src_access_mask: ash::vk::AccessFlags::NONE,
                            dst_access_mask: ash::vk::AccessFlags::NONE,
                            old_layout: ash::vk::ImageLayout::PRESENT_SRC_KHR,
                            new_layout: ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            subresource_range: swimg.subresource_all(),
                            ..Default::default()
                        },
                    ],
                )
            }
        });

        //now blit to the swapchain image
        recorder.record({
            let img = self.image.share();
            let swimg = swapchain_image.image.clone();
            move |dev, cmd| unsafe {
                dev.cmd_blit_image(
                    *cmd,
                    img.inner,
                    ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    swimg.inner,
                    ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[ash::vk::ImageBlit {
                        //Note we are using blit mainly for format transfer
                        src_offsets: [
                            Offset3D { x: 0, y: 0, z: 0 },
                            Offset3D {
                                x: ext.width as i32,
                                y: ext.height as i32,
                                z: 1,
                            },
                        ],
                        dst_offsets: [
                            Offset3D { x: 0, y: 0, z: 0 },
                            Offset3D {
                                x: ext.width as i32,
                                y: ext.height as i32,
                                z: 1,
                            },
                        ],
                        src_subresource: img.subresource_layers_all(),
                        dst_subresource: swimg.subresource_layers_all(),
                        ..Default::default()
                    }],
                    ash::vk::Filter::LINEAR,
                );
            }
        });

        //finally move swapchain image back to present and compute image back to general
        recorder.record({
            let img = self.image.share();
            let swimg = swapchain_image.image.clone();
            move |dev, cmd| unsafe {
                dev.cmd_pipeline_barrier(
                    *cmd,
                    ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                    ash::vk::PipelineStageFlags::TRANSFER,
                    ash::vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[
                        //Transfer attachment image from COMPUTE to SHADER_WRITE
                        ash::vk::ImageMemoryBarrier {
                            image: img.inner,
                            src_access_mask: ash::vk::AccessFlags::NONE,
                            dst_access_mask: ash::vk::AccessFlags::NONE,
                            old_layout: ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                            new_layout: ash::vk::ImageLayout::GENERAL,
                            subresource_range: img.subresource_all(),
                            ..Default::default()
                        },
                        //Move swapchain image to transfer dst from present layout
                        ash::vk::ImageMemoryBarrier {
                            image: swimg.inner,
                            src_access_mask: ash::vk::AccessFlags::NONE,
                            dst_access_mask: ash::vk::AccessFlags::NONE,
                            old_layout: ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            new_layout: ash::vk::ImageLayout::PRESENT_SRC_KHR,
                            subresource_range: img.subresource_all(),
                            ..Default::default()
                        },
                    ],
                );
            }
        });

        //End recording.
        recorder.finish_recording().unwrap();

        Ok(())
    }

    fn push(&mut self, new: PushConst) {
        *self.push_constant.lock().unwrap().get_content_mut() = new;
    }
}

///Collects all runtime state for the application. Basically the context, swapchain and pipeline used for drawing.
struct App {
    ctx: Ctx<Allocator>, //NOTE: This is the default allocator.
    swapchain: Swapchain,

    pass_data: Vec<PassData>,
}

impl App {
    pub fn new(window: &winit::window::Window) -> anyhow::Result<Self> {
        //now test context setup
        let (ctx, surface) = Ctx::default_with_surface(&window, true)?;
        let swapchain = Swapchain::builder(&ctx.device, surface)?
            .with(|b| {
                b.create_info.usage = ash::vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | ash::vk::ImageUsageFlags::TRANSFER_DST
            })
            .with(|b| {
                println!("Formats");
                for f in b.format_preference.iter() {
                    println!("  {:#?}", f);
                }
            })
            .build()?;

        let width = 512;
        let height = 512;
        //create our rendering image
        let pass_data = (0..swapchain.images.len())
            .map(|_idx| PassData::new(&ctx, width, height).unwrap())
            .collect();

        Ok(App {
            ctx,
            swapchain,
            pass_data,
        })
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
        //now recreate pass data as well with new swapchain
        self.pass_data = (0..self.swapchain.images.len())
            .map(|_idx| PassData::new(&self.ctx, extent.width, extent.height).unwrap())
            .collect();
    }
    //Enques a new draw event.
    pub fn draw(&mut self, window: &winit::window::Window, push: PushConst) {
        let extent = self
            .swapchain
            .surface
            .get_capabilities(&self.ctx.device.physical_device)
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
        if swext != extent || self.pass_data[0].image.extent_2d() != swext {
            self.resize(extent);
        }

        //Get next image. Note that acquiring is handled by the swapchain itself
        let swimage = self.swapchain.acquire_next_image().unwrap();

        self.pass_data[swimage.index as usize]
            .command_buffer
            .wait()
            .unwrap();

        self.pass_data[swimage.index as usize].push(push);

        //record new frame based on this image
        self.pass_data[swimage.index as usize]
            .record(&self.ctx, &swimage)
            .unwrap();

        if let Err(e) = self.pass_data[swimage.index as usize]
            .command_buffer
            .submit_present(
                &self.ctx.device,
                &self.ctx.device.queues[0],
                swimage,
                &self.swapchain,
                &[],
                &[],
            )
        {
            println!("Error queue submit: {}", e);
        }
    }
}

fn main() -> Result<()> {
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
