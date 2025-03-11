//! The classic counter example from Iced's Readme, but using the marpii based renderer.

use iced::widget::{button, column, text};
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

#[derive(Default)]
struct Counter {
    value: i64,
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
            button("Decrement").on_press(Message::Decrement)
        ]
        .padding(20)
        .align_x(Center)
        .into()
    }
}
