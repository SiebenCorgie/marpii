use anyhow::Result;
use marpii::{ash::vk, context::Ctx};
use marpii_rmg::Rmg;
use winit::window::Window;

use marpii::resources::ImgDesc;
use marpii_rmg::{
    recorder::task_scheduler::TaskSchedule, BufferHandle, ImageHandle, ResourceRegistry, Resources,
    Task,
};

#[derive(Clone)]
struct DummyTask {
    name: &'static str,
    caps: vk::QueueFlags,
    images: Vec<ImageHandle>,
    buffers: Vec<BufferHandle<u8>>,
}

impl Task for DummyTask {
    fn name(&self) -> &'static str {
        self.name
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        self.caps
    }
    fn register(&self, registry: &mut ResourceRegistry) {
        for i in self.images.iter() {
            println!("reg: {:?}", i);
            if registry
                .request_image(
                    i,
                    vk::PipelineStageFlags2::ALL_COMMANDS,
                    vk::AccessFlags2::empty(),
                    vk::ImageLayout::GENERAL,
                )
                .is_err()
            {
                println!("{:?} was registered", i);
            }
        }
        for b in self.buffers.iter() {
            registry
                .request_buffer(
                    b,
                    vk::PipelineStageFlags2::ALL_COMMANDS,
                    vk::AccessFlags2::empty(),
                )
                .unwrap();
        }
        println!("RegFin");
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
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

    let shadows_img = rmg
        .new_image_uninitialized(
            ImgDesc::texture_2d(64, 64, vk::Format::R8G8B8A8_UNORM),
            None,
        )
        .unwrap();
    let prev_post = rmg
        .new_image_uninitialized(
            ImgDesc::texture_2d(64, 64, vk::Format::R8G8B8A8_UNORM),
            None,
        )
        .unwrap();
    let lookup = rmg
        .new_image_uninitialized(
            ImgDesc::texture_2d(64, 64, vk::Format::R8G8B8A8_UNORM),
            None,
        )
        .unwrap();
    let lookup2 = rmg
        .new_image_uninitialized(
            ImgDesc::texture_2d(64, 64, vk::Format::R8G8B8A8_UNORM),
            None,
        )
        .unwrap();
    let target = rmg
        .new_image_uninitialized(
            ImgDesc::texture_2d(64, 64, vk::Format::R8G8B8A8_UNORM),
            None,
        )
        .unwrap();

    let buf1 = rmg.new_buffer(64, None).unwrap();
    let buf2 = rmg.new_buffer(64, None).unwrap();
    let buf3 = rmg.new_buffer(64, None).unwrap();

    let mut async_compute = DummyTask {
        name: "independent_compute",
        caps: vk::QueueFlags::COMPUTE,
        images: vec![lookup.clone()],
        buffers: vec![buf1.clone()],
    };

    let mut shadows = DummyTask {
        name: "shadows",
        caps: vk::QueueFlags::GRAPHICS,
        images: vec![shadows_img.clone()],
        buffers: vec![buf2.clone()],
    };

    let mut forward = DummyTask {
        name: "forward",
        caps: vk::QueueFlags::GRAPHICS,
        images: vec![shadows_img.clone(), lookup2.clone(), lookup.clone()],
        buffers: vec![buf2.clone(), buf3.clone()],
    };

    let mut final_pass = DummyTask {
        name: "final",
        caps: vk::QueueFlags::GRAPHICS,
        images: vec![prev_post.clone(), target.clone()],
        buffers: vec![buf3.clone()],
    };

    let tasks = rmg
        .record_compute_only()
        .add_task(&mut async_compute)
        .unwrap()
        .add_task(&mut shadows)
        .unwrap()
        .add_task(&mut forward)
        .unwrap()
        .add_task(&mut final_pass)
        .unwrap();

    let schedule = TaskSchedule::new_from_tasks(tasks.rmg, tasks.records)?;
    println!("{}", schedule);
    marpii_rmg::recorder::task_executor::Executor::execute(tasks.rmg, schedule)?;
    Ok(())
}

fn window_extent(window: &Window) -> vk::Extent2D {
    vk::Extent2D {
        width: window.inner_size().width,
        height: window.inner_size().height,
    }
}
