//Shows how one can access the underlying RMG framework within a custom widget

use std::sync::{Arc, Mutex};

use iced::widget::{button, column, text};
use iced::Length::Fill;
use iced::{Center, Element, Theme};
use iced_marpii_shared::ResourceHandle;
use marpii::ash::vk;
use marpii::resources::{ComputePipeline, PushConstant, ShaderModule};
use marpii::OoS;
use marpii_rmg::{ImageHandle, Task};

type MElement<'a, M> = Element<'a, M, Theme, iced_marpii::Renderer>;

pub fn main() -> iced::Result {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();

    iced::run("A cool counter", Counter::update, Counter::view)
}

#[repr(C)]
#[derive(Debug)]
struct CSPush {
    target_color: ResourceHandle,
    target_depth: ResourceHandle,
    resolution: [u32; 2],

    bound_offset: [f32; 2],
    bound_size: [f32; 2],

    layer_depth: f32,
    pad0: [f32; 3],
}

impl Default for CSPush {
    fn default() -> Self {
        Self {
            target_color: ResourceHandle::INVALID,
            target_depth: ResourceHandle::INVALID,
            resolution: [0; 2],
            bound_size: [0.0; 2],
            bound_offset: [0.0; 2],
            layer_depth: 0.0,
            pad0: [0.0; 3],
        }
    }
}

struct MyRmgPrimitive {
    pipeline: Arc<Mutex<Option<ComputePipeline>>>,
    color_image: Option<ImageHandle>,
    depth_image: Option<ImageHandle>,
    push: PushConstant<CSPush>,
}

impl MyRmgPrimitive {
    const SHADER_COMP: &[u8] = include_bytes!("custom_shader.spirv");
    fn dispatch_count(&self) -> [u32; 3] {
        [
            ((self.color_image.as_ref().unwrap().extent_2d().width as f32 / 8.0).ceil() as u32)
                .max(1),
            ((self.color_image.as_ref().unwrap().extent_2d().height as f32 / 8.0).ceil() as u32)
                .max(1),
            1,
        ]
    }
}

impl Task for MyRmgPrimitive {
    fn name(&self) -> &'static str {
        "RmgPrimitive"
    }
    fn pre_record(
        &mut self,
        resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        self.push.get_content_mut().target_color =
            resources.resource_handle_or_bind(self.color_image.as_ref().unwrap())?;
        self.push.get_content_mut().target_depth =
            resources.resource_handle_or_bind(self.depth_image.as_ref().unwrap())?;
        Ok(())
    }
    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_image(
                self.color_image.as_ref().unwrap(),
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
            )
            .unwrap();
        registry
            .request_image(
                self.depth_image.as_ref().unwrap(),
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
            )
            .unwrap();
        registry.register_asset(self.pipeline.clone());
    }
    fn queue_flags(&self) -> marpii::ash::vk::QueueFlags {
        marpii::ash::vk::QueueFlags::COMPUTE | marpii::ash::vk::QueueFlags::GRAPHICS
    }
    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &marpii::ash::vk::CommandBuffer,
        _resources: &marpii_rmg::Resources,
    ) {
        //bind commandbuffer, setup push constant and execute
        unsafe {
            device.inner.cmd_bind_pipeline(
                *command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline.lock().unwrap().as_ref().unwrap().pipeline,
            );
            device.inner.cmd_push_constants(
                *command_buffer,
                self.pipeline
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .layout
                    .layout,
                vk::ShaderStageFlags::ALL,
                0,
                self.push.content_as_bytes(),
            );

            let [dx, dy, dz] = self.dispatch_count();

            device.inner.cmd_dispatch(*command_buffer, dx, dy, dz);
        }
    }
}

impl iced_marpii::custom::Primitive for MyRmgPrimitive {
    fn prepare(
        &mut self,
        rmg: &mut marpii_rmg::Rmg,
        color_image: marpii_rmg::ImageHandle,
        depth_image: marpii_rmg::ImageHandle,
        bounds: &iced::Rectangle,
        _viewport: &iced_graphics::Viewport,
        _transform: iced::Transformation,
        layer_depth: f32,
    ) {
        //prepare just pushes context info
        self.push.get_content_mut().bound_offset = [bounds.x, bounds.y];
        self.push.get_content_mut().bound_size = [bounds.width, bounds.height];
        self.push.get_content_mut().layer_depth = layer_depth;
        self.push.get_content_mut().resolution = [
            color_image.extent_2d().width,
            color_image.extent_2d().height,
        ];
        self.color_image = Some(color_image);
        self.depth_image = Some(depth_image);

        //if the pipeline is not yet loaded, do that now
        if let Ok(mut pipe_load) = self.pipeline.lock() {
            if pipe_load.is_none() {
                let shader_module =
                    ShaderModule::new_from_bytes(&rmg.ctx.device, Self::SHADER_COMP).unwrap();
                let shader_stage =
                    shader_module.into_shader_stage(vk::ShaderStageFlags::COMPUTE, "main");
                //No additional descriptors for us
                let layout = rmg.resources.bindless_layout();
                let pipeline = ComputePipeline::new(
                    &rmg.ctx.device,
                    &shader_stage,
                    None,
                    OoS::new_shared(layout),
                )
                .unwrap();
                *pipe_load = Some(pipeline);
            }
        } else {
            panic!("Could not lock pipe!");
        }
    }

    fn is_background(&self) -> bool {
        true
    }

    fn render<'a>(
        &'a mut self,
        recorder: marpii_rmg::Recorder<'a>,
        _color_image: marpii_rmg::ImageHandle,
        _depth_image: marpii_rmg::ImageHandle,
        _clip_bounds: &iced::Rectangle,
        _transform: iced::Transformation,
    ) -> marpii_rmg::Recorder<'a> {
        recorder.add_task(self).unwrap()
    }
}

#[derive(Default)]
struct MyRmgProgram {
    //preloaded pipeline we clone to the primitive each time
    pipeline: Arc<Mutex<Option<ComputePipeline>>>,
}

impl MyRmgProgram {}

impl iced_marpii::custom::Program<Message> for MyRmgProgram {
    type State = ();
    type Primitive = MyRmgPrimitive;
    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced_core::mouse::Cursor,
        _bounds: iced::Rectangle,
    ) -> Self::Primitive {
        MyRmgPrimitive {
            pipeline: self.pipeline.clone(),
            color_image: None,
            depth_image: None,
            push: PushConstant::new(CSPush::default(), vk::ShaderStageFlags::COMPUTE),
        }
    }
}

#[derive(Default)]
struct Counter {
    value: i64,
    my_rmg_surface: MyRmgProgram,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Increment,
    Decrement,
}

impl Counter {
    fn update(&mut self, message: Message) {
        match message {
            Message::Increment => {
                self.value += 1;
            }
            Message::Decrement => {
                self.value -= 1;
            }
        }
    }

    fn view(&self) -> MElement<Message> {
        let shader = iced_marpii::custom::marpii_surface(&self.my_rmg_surface)
            .width(Fill)
            .height(Fill);

        column![
            button("Increment")
                .on_press(Message::Increment)
                //Play around with the styles!
                .style(|t, s| {
                    let mut style = iced::widget::button::primary(t, s);
                    style.border.radius = iced::border::Radius::new(5.0);
                    style.border.width = 2.0;
                    style.border.color = iced::Color::from_rgb(0.6, 0.8, 0.6);
                    style.shadow.color = iced::Color::BLACK;
                    style.shadow.offset = iced::Vector::new(6.0, 6.0);
                    style.shadow.blur_radius = 5.0;
                    style
                }),
            text(self.value).size(50),
            button("Decrement").on_press(Message::Decrement),
            text("Look below!").style(iced::widget::text::secondary),
            shader,
        ]
        .spacing(10)
        .padding(20)
        .align_x(Center)
        .into()
    }
}
