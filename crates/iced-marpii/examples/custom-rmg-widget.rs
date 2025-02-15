//Shows how one can access the underlying RMG framework within a custom widget

use iced::widget::{button, column, text};
use iced::Length::Fill;
use iced::{Center, Element, Theme};

type MElement<'a, M> = Element<'a, M, Theme, iced_marpii::Renderer>;
//type MElement<'a, M> = Element<'a, M, Theme, iced_wgpu::Renderer>;

pub fn main() -> iced::Result {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();

    iced::run("A cool counter", Counter::update, Counter::view)
}

#[derive(Debug)]
struct MyRmgPrimitive;

impl iced_marpii::custom::Primitive for MyRmgPrimitive {
    fn render<'a>(
        &mut self,
        recorder: marpii_rmg::Recorder<'a>,
        color_image: marpii_rmg::ImageHandle,
        depth_image: marpii_rmg::ImageHandle,
        clip_bounds: &iced::Rectangle<u32>,
        layer_depth: f32,
    ) -> marpii_rmg::Recorder<'a> {
        println!("Rendering");
        recorder
    }

    fn prepare(
        &self,
        _rmg: &mut marpii_rmg::Rmg,
        _color_image: marpii_rmg::ImageHandle,
        _depth_image: marpii_rmg::ImageHandle,
        _bounds: &iced::Rectangle,
        _viewport: &iced_graphics::Viewport,
    ) {
        println!("Preparing for RMG!");
    }
}

#[derive(Default)]
struct MyRmgProgram;

impl iced_marpii::custom::Program<Message> for MyRmgProgram {
    type State = ();
    type Primitive = MyRmgPrimitive;
    fn draw(
        &self,
        state: &Self::State,
        cursor: iced_core::mouse::Cursor,
        bounds: iced::Rectangle,
    ) -> Self::Primitive {
        println!("Emitting primitive for {:?}", bounds);
        MyRmgPrimitive
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
    Other,
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
            _ => {}
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
            shader,
        ]
        .padding(20)
        .align_x(Center)
        .into()
    }
}
