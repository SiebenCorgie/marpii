//!Shows how one can access the underlying RMG framework within a custom widget.
//!
//! The interesting part is how we handle custom state in prepare() and render() of the primitive implementation.
//! The idea is to use the `persistent` utility to store data within the iced-renderer, that also hosts the RMG context.
//!
//! Note how we don't have to use Arcs for everything. The primitive is emitted as `unprepared`, and changed to `prepared`
//! in the handler. This is also where we initialize anything that has to do with the renderer.
//!
//! In a multithreaded setup that might also be where you'd synchronize / upload to the GPU :).

use iced::widget::shader::Viewport;
use iced::widget::{button, column, text};
use iced::Length::Fill;
use iced::{Center, Element, Subscription, Theme};
use iced_marpii::custom::Persistent;
use iced_marpii::marpii;
use iced_marpii::marpii_rmg;
use iced_marpii::marpii_rmg_tasks::ImageBlit;
use marpii::ash::vk;
use mypass::MyRenderPass;

mod mypass;

type MElement<'a, M> = Element<'a, M, Theme, iced_marpii::Renderer>;

pub fn main() -> iced::Result {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();

    iced::application("Custom Graphics Widget", Counter::update, Counter::view)
        .subscription(Counter::subscription)
        .run()
}

enum MyPrimitive {
    Unprepared,
    Prepared {
        pass: MyRenderPass,
        blit_color: ImageBlit<1>,
        blit_depth: ImageBlit<1>,
    },
}

impl iced_marpii::custom::Primitive for MyPrimitive {
    fn prepare(
        &mut self,
        rmg: &mut marpii_rmg::Rmg,
        color_image: marpii_rmg::ImageHandle,
        depth_image: marpii_rmg::ImageHandle,
        persistent: &mut Persistent,
        bounds: &iced::Rectangle,
        _viewport: &Viewport,
        _transform: iced::Transformation,
        layer_depth: f32,
    ) {
        //get or create our pass
        let compute_pass: &mut MyRenderPass =
            if let Some(cp) = persistent.get_named_mut("compute-pass") {
                cp
            } else {
                //create the compute pass and push it
                let cp = MyRenderPass::create(rmg, color_image.extent_2d());
                let key = persistent.store_named("compute-pass", cp);
                persistent.get_mut(&key).unwrap()
            };

        //Notify resolution.
        compute_pass.resize(rmg, color_image.extent_2d());

        //prepare just pushes context info
        compute_pass.push.get_content_mut().bound_offset = [bounds.x, bounds.y];
        compute_pass.push.get_content_mut().bound_size = [bounds.width, bounds.height];
        compute_pass.push.get_content_mut().layer_depth = layer_depth;

        let blit_color = ImageBlit::new(compute_pass.color_image.clone(), color_image.clone())
            .with_blits([(
                compute_pass.color_image.region_all(),
                color_image.region_all(),
            )]);

        let blit_depth = ImageBlit::new(compute_pass.depth_image.clone(), depth_image.clone())
            .with_blits([(
                compute_pass.depth_image.region_all(),
                depth_image.region_all(),
            )])
            .with_filter(vk::Filter::NEAREST);

        //now before ending, copy a instance of the pass into our own
        *self = MyPrimitive::Prepared {
            pass: compute_pass.clone(),
            blit_color,
            blit_depth,
        };
    }

    fn is_background(&self) -> bool {
        true
    }

    fn render<'a>(
        &'a mut self,
        recorder: marpii_rmg::Recorder<'a>,
        _color_image: marpii_rmg::ImageHandle,
        _depth_image: marpii_rmg::ImageHandle,
        _persistent: &Persistent,
        _clip_bounds: &iced::Rectangle,
        _transform: iced::Transformation,
    ) -> marpii_rmg::Recorder<'a> {
        match self {
            MyPrimitive::Prepared {
                pass,
                blit_color,
                blit_depth,
            } => recorder
                .add_task(pass)
                .unwrap()
                .add_task(blit_color)
                .unwrap()
                .add_task(blit_depth)
                .unwrap(),
            MyPrimitive::Unprepared => {
                log::error!("Failed to prepare custom renderpass :(");
                recorder
            }
        }
    }
}

#[derive(Default)]
struct MyRmgProgram;

impl iced_marpii::custom::Program<Message> for MyRmgProgram {
    type State = ();
    type Primitive = MyPrimitive;
    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced::mouse::Cursor,
        _bounds: iced::Rectangle,
    ) -> Self::Primitive {
        MyPrimitive::Unprepared
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
    None,
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
            Message::None => {}
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(iced::time::Duration::from_millis(16)).map(|_| Message::None)
    }

    fn view(&self) -> MElement<Message> {
        let shader = iced_marpii::custom::marpii_surface(&self.my_rmg_surface)
            .width(Fill)
            .height(Fill);

        column![
            button("Increment").on_press(Message::Increment),
            text(self.value).size(50).style(|t| {
                //turn text bright, to make it stand out
                let mut s = iced::widget::text::danger(t);
                s.color = Some(iced::Color::from_rgb8(220, 220, 220));
                s
            }),
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
