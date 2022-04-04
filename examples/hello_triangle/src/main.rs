//! A simple marpii application that uses marpii's `Ctx` abstraction to automaticaly create a context for a window.
//! For each frame a compute shader is executed that writes to a swapchain image.

use anyhow::Result;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii::resources::{
    CommandBuffer, CommandBufferAllocator, CommandPool, ComputePipeline, DescriptorAllocator,
    DescriptorPool, DescriptorSet,
};
use marpii::{
    ash::{
        self,
        vk::{Extent2D, Offset3D},
    },
    context::Ctx,
    resources::{
        Image, ImageView, ImgDesc, PipelineLayout, PushConstant, SafeImageView, ShaderModule,
    },
    swapchain::{Swapchain, SwapchainImage},
};
use std::sync::Arc;
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

#[repr(C)]
pub struct PushConst {
    floatconst: f32,
    color: [f32; 3],
}

struct PassData {
    //image that is rendered to
    image: Arc<Image<Allocator>>,
    #[allow(dead_code)]
    image_view: ImageView<Allocator>,

    command_buffer: CommandBuffer<CommandPool>,

    descriptor_set: DescriptorSet<Arc<DescriptorPool>>,

    pipeline: ComputePipeline,
    push_constant: PushConstant<PushConst>,

    is_first_time: bool,

    last_draw_fence: Option<Arc<marpii::sync::Fence<()>>>,
}

impl PassData {
    pub fn new(ctx: &Ctx<Allocator>, width: u32, height: u32) -> Result<Self, anyhow::Error> {
        println!("Recreate image for: {}x{}", width, height);

        let image = Arc::new(Image::new(
            &ctx.device,
            &ctx.allocator,
            ImgDesc::color_attachment_2d(width, height, ash::vk::Format::R8G8B8A8_UNORM)
                .add_usage(ash::vk::ImageUsageFlags::TRANSFER_SRC)
                .add_usage(ash::vk::ImageUsageFlags::STORAGE),
            marpii::allocator::MemoryUsage::GpuOnly,
            Some("RenderTarget"),
            None,
        )?);
        let image_view = image.view(ctx.device.clone(), image.view_all())?;

        let push_constant = PushConstant::new(
            PushConst {
                color: [0.1, 0.75, 0.0],
                floatconst: 1.0,
            },
            ash::vk::ShaderStageFlags::COMPUTE,
        );

        //load shader from file
        let shader_module = ShaderModule::new_from_file(&ctx.device, "resources/test_shader.spv")?;

        let descriptor_set_layouts = shader_module.create_descriptor_set_layouts()?;

        let descriptor_set = {
            //NOTE bad practise, should be done per app.
            let pool = DescriptorPool::new_for_module(
                &ctx.device,
                ash::vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET,
                &shader_module,
                1,
            )?;
            let pool = Arc::new(pool); //wrap since the trait allocation depends on it

            let mut set = pool.allocate(&descriptor_set_layouts[0].1.inner)?;

            set.write(
                ash::vk::WriteDescriptorSet::builder()
                    .dst_set(set.inner)
                    .dst_binding(0)
                    .dst_array_element(0)
                    .descriptor_type(ash::vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&[ash::vk::DescriptorImageInfo {
                        sampler: ash::vk::Sampler::null(),
                        image_view: image_view.view,
                        image_layout: ash::vk::ImageLayout::GENERAL,
                    }]),
            );

            set
        };

        let pipeline = {
            let pipeline_layout = PipelineLayout::from_layout_push(
                &ctx.device,
                &descriptor_set_layouts,
                &push_constant,
            )
            .unwrap();

            let pipeline = ComputePipeline::new(
                &ctx.device,
                shader_module
                    .into_shader_stage(ash::vk::ShaderStageFlags::COMPUTE, "main".to_owned()),
                None,
                pipeline_layout,
            )?;

            pipeline
        };

        //Time to create the command buffer and descriptor set
        let cb = {
            let command_pool = CommandPool::new(
                &ctx.device,
                ctx.device.queues[0].family_index,
                ash::vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
            )?;

            let command_buffer =
                command_pool.allocate_buffer(ash::vk::CommandBufferLevel::PRIMARY)?;

            command_buffer
        };

        Ok(PassData {
            command_buffer: cb,
            image,
            image_view,
            pipeline,
            push_constant,

            descriptor_set,

            is_first_time: true,
            last_draw_fence: None,
        })
    }

    ///Records a new command buffer that renders to `image` and blits it to the swapchain's `image_idx` image.
    pub unsafe fn record(
        &mut self,
        ctx: &Ctx<Allocator>,
        swapchain_image: &SwapchainImage,
    ) -> Result<(), anyhow::Error> {
        //For now define what queue family we are on. This should usually be checked.
        let queue_graphics_family = ctx.device.queues[0].family_index;

        //first of all, reset our command buffer. Should be save since recording occures after waiting for the
        //acquire operation.
        ctx.device.inner.reset_command_buffer(
            self.command_buffer.inner,
            ash::vk::CommandBufferResetFlags::empty(),
        )?;

        //record the command buffer to execute our pipeline, then blit the image to the swapchain image
        ctx.device.inner.begin_command_buffer(
            self.command_buffer.inner,
            &ash::vk::CommandBufferBeginInfo::builder(),
        )?;

        //Bind descriptor set and pipeline
        ctx.device.inner.cmd_bind_pipeline(
            self.command_buffer.inner,
            ash::vk::PipelineBindPoint::COMPUTE,
            self.pipeline.pipeline,
        );
        ctx.device.inner.cmd_bind_descriptor_sets(
            self.command_buffer.inner,
            ash::vk::PipelineBindPoint::COMPUTE,
            self.pipeline.layout.layout,
            0,
            &[self.descriptor_set.inner],
            &[],
        );

        ctx.device.inner.cmd_push_constants(
            self.command_buffer.inner,
            self.pipeline.layout.layout,
            ash::vk::ShaderStageFlags::COMPUTE,
            0,
            self.push_constant.content_as_bytes(),
        );

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
            ctx.device.inner.cmd_pipeline_barrier(
                self.command_buffer.inner,
                ash::vk::PipelineStageFlags::TOP_OF_PIPE,
                ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                ash::vk::DependencyFlags::empty(),
                &[], //mem
                &[], //buffer
                &[
                    //Transfer attachment image from UNDEFINED to SHADER_WRITE
                    ash::vk::ImageMemoryBarrier {
                        image: self.image.inner,
                        src_access_mask: ash::vk::AccessFlags::NONE,
                        dst_access_mask: ash::vk::AccessFlags::NONE,
                        old_layout: ash::vk::ImageLayout::UNDEFINED,
                        new_layout: ash::vk::ImageLayout::GENERAL,
                        subresource_range: self.image.subresource_all(),
                        src_queue_family_index: queue_graphics_family,
                        dst_queue_family_index: queue_graphics_family,
                        ..Default::default()
                    },
                    //Move swapchain image to presetn src, since the later barrier will move it into transfer
                    //dst assuming it was on present src khr.
                    ash::vk::ImageMemoryBarrier {
                        image: swapchain_image.image.inner,
                        src_access_mask: ash::vk::AccessFlags::NONE,
                        dst_access_mask: ash::vk::AccessFlags::NONE,
                        old_layout: ash::vk::ImageLayout::UNDEFINED,
                        new_layout: ash::vk::ImageLayout::PRESENT_SRC_KHR,
                        subresource_range: self.image.subresource_all(),
                        src_queue_family_index: queue_graphics_family,
                        dst_queue_family_index: queue_graphics_family,
                        ..Default::default()
                    },
                ],
            );
            //null flag to not do this again.
            self.is_first_time = false;
        }

        //Dispatch cs
        ctx.device.inner.cmd_dispatch(
            self.command_buffer.inner,
            submit_size[0],
            submit_size[1],
            submit_size[2],
        );

        //Issue a barrier to wait for the compute shader and move the images to transfer src/dst
        ctx.device.inner.cmd_pipeline_barrier(
            self.command_buffer.inner,
            ash::vk::PipelineStageFlags::COMPUTE_SHADER,
            ash::vk::PipelineStageFlags::TRANSFER,
            ash::vk::DependencyFlags::empty(),
            &[],
            &[],
            &[
                ash::vk::ImageMemoryBarrier {
                    image: self.image.inner,
                    src_access_mask: ash::vk::AccessFlags::NONE,
                    dst_access_mask: ash::vk::AccessFlags::NONE,
                    old_layout: ash::vk::ImageLayout::GENERAL,
                    new_layout: ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    subresource_range: self.image.subresource_all(),
                    src_queue_family_index: queue_graphics_family,
                    dst_queue_family_index: queue_graphics_family,
                    ..Default::default()
                },
                //Move swapchain image to transfer dst from present layout
                ash::vk::ImageMemoryBarrier {
                    image: swapchain_image.image.inner,
                    src_access_mask: ash::vk::AccessFlags::NONE,
                    dst_access_mask: ash::vk::AccessFlags::NONE,
                    old_layout: ash::vk::ImageLayout::PRESENT_SRC_KHR,
                    new_layout: ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    subresource_range: self.image.subresource_all(),
                    src_queue_family_index: queue_graphics_family,
                    dst_queue_family_index: queue_graphics_family,
                    ..Default::default()
                },
            ],
        );

        //now blit to the swapchain image
        ctx.device.inner.cmd_blit_image(
            self.command_buffer.inner,
            self.image.inner,
            ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            swapchain_image.image.inner,
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
                src_subresource: self.image.subresource_layers_all(),
                dst_subresource: swapchain_image.image.subresource_layers_all(),
                ..Default::default()
            }],
            ash::vk::Filter::LINEAR,
        );

        //finally move swapchain image back to present and compute image back to general
        ctx.device.inner.cmd_pipeline_barrier(
            self.command_buffer.inner,
            ash::vk::PipelineStageFlags::COMPUTE_SHADER,
            ash::vk::PipelineStageFlags::TRANSFER,
            ash::vk::DependencyFlags::empty(),
            &[],
            &[],
            &[
                //Transfer attachment image from COMPUTE to SHADER_WRITE
                ash::vk::ImageMemoryBarrier {
                    image: self.image.inner,
                    src_access_mask: ash::vk::AccessFlags::NONE,
                    dst_access_mask: ash::vk::AccessFlags::NONE,
                    old_layout: ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    new_layout: ash::vk::ImageLayout::GENERAL,
                    subresource_range: self.image.subresource_all(),
                    src_queue_family_index: queue_graphics_family,
                    dst_queue_family_index: queue_graphics_family,
                    ..Default::default()
                },
                //Move swapchain image to transfer dst from present layout
                ash::vk::ImageMemoryBarrier {
                    image: swapchain_image.image.inner,
                    src_access_mask: ash::vk::AccessFlags::NONE,
                    dst_access_mask: ash::vk::AccessFlags::NONE,
                    old_layout: ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    new_layout: ash::vk::ImageLayout::PRESENT_SRC_KHR,
                    subresource_range: self.image.subresource_all(),
                    src_queue_family_index: queue_graphics_family,
                    dst_queue_family_index: queue_graphics_family,
                    ..Default::default()
                },
            ],
        );
        //End recording.
        ctx.device
            .inner
            .end_command_buffer(self.command_buffer.inner)?;

        Ok(())
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

        let swapchain = Swapchain::builder(&ctx.device, &surface)?
            .with(|b| {
                b.usage = ash::vk::ImageUsageFlags::COLOR_ATTACHMENT
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
        println!("Resizeing swapchain and pass data to: {:?}", extent);

        //Resize swapchain. Initial transition of the images will be handled by the
        // pass data.
        self.swapchain.recreate(extent).unwrap();
        //now recreate pass data as well with new swapchain
        self.pass_data = (0..self.swapchain.images.len())
            .map(|_idx| PassData::new(&self.ctx, extent.width, extent.height).unwrap())
            .collect();
    }
    //Enques a new draw event.
    pub fn draw(&mut self, window: &winit::window::Window) {
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
        if swext != extent || self.pass_data[0].image.extent_2d() != swext {
            self.resize(extent);
        }
        //Get next image. Note that acquiring is handled by the swapchain itself
        let swimage = self.swapchain.acquire_next_image().unwrap();

        let fence = if let Some(f) = self.pass_data[swimage.index as usize]
            .last_draw_fence
            .take()
        {
            f.wait(u64::MAX).unwrap();
            f.reset().unwrap();
            f
        } else {
            marpii::sync::Fence::new(self.ctx.device.clone(), false, None).unwrap()
        };

        //record new frame based on this image
        unsafe {
            self.pass_data[swimage.index as usize]
                .record(&self.ctx, &swimage)
                .unwrap()
        };

        //execute recorded command buffer, signaling the present semaphore of the swapchain
        if let Err(e) = unsafe {
            self.ctx.device.inner.queue_submit(
                self.ctx.device.queues[0].inner,
                &[*ash::vk::SubmitInfo::builder()
                    .command_buffers(&[self.pass_data[swimage.index as usize].command_buffer.inner])
                    .signal_semaphores(&[swimage.sem_present.inner])],
                fence.inner,
            )
        } {
            println!("Error queue submit: {}", e);
        }

        //set fence so we can wait in the next frame
        self.pass_data[swimage.index as usize].last_draw_fence = Some(fence);

        //now enqueue for present
        if let Err(e) = self
            .swapchain
            .present_image(swimage, &self.ctx.device.queues[0].inner)
        {
            println!("Present error: {}", e);
        }
    }
}

fn main() -> Result<()> {
    simple_logger::SimpleLogger::new().init().unwrap();

    let ev = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&ev).unwrap();

    let mut app = App::new(&window)?;

    ev.run(move |event, _, ctrl| {
        *ctrl = ControlFlow::Poll;

        match event {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                app.draw(&window);
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *ctrl = ControlFlow::Exit;
                    println!("============\nBye Bye============");

                    unsafe {
                        app.ctx
                            .device
                            .inner
                            .device_wait_idle()
                            .expect("Failed to wait")
                    };
                }
                _ => {}
            },
            _ => {}
        }
    });
}
