//! Copy of Iced's Gradient example, but using the
//! MarpII renderer.

use iced::application;
use iced::gradient;
use iced::widget::button;
use iced::widget::Row;
use iced::widget::{checkbox, column, container, horizontal_space, row, slider, text};
use iced::Length;
use iced::{Center, Color, Element, Fill, Radians, Theme};

type MElement<'a, M> = Element<'a, M, Theme, iced_marpii::Renderer>;

pub fn main() -> iced::Result {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();

    iced::application("Gradient - Iced", Gradient::update, Gradient::view)
        .style(Gradient::style)
        .transparent(true)
        .run()
}

#[derive(Debug, Clone, Copy)]
struct Gradient {
    //All stops
    stops: [Option<(Color, f32)>; 8],
    angle: Radians,
    transparent: bool,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    StopChanged {
        stop: usize,
        color: Color,
        offset: f32,
    },
    None,
    AngleChanged(Radians),
    TransparentToggled(bool),
}

impl Gradient {
    fn new() -> Self {
        Self {
            stops: [
                Some((Color::from_rgb(0.8, 0.2, 0.22), 0.0)),
                Some((Color::from_rgb(0.3, 0.2, 0.8), 1.0)),
                None,
                None,
                None,
                None,
                None,
                None,
            ],
            angle: Radians(0.0),
            transparent: false,
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::StopChanged {
                stop,
                color,
                offset,
            } => self.stops[stop] = Some((color, offset)),
            Message::AngleChanged(angle) => self.angle = angle,
            Message::TransparentToggled(transparent) => {
                self.transparent = transparent;
            }
            Message::None => {}
        }
    }

    fn view(&self) -> MElement<'_, Message> {
        let Self {
            stops,
            angle,
            transparent,
        } = *self;

        let gradient_box = container(horizontal_space())
            .style(move |_theme| {
                let mut gradient = gradient::Linear::new(angle);
                for stop in &stops {
                    if let Some(stop) = stop {
                        gradient = gradient.add_stop(stop.1, stop.0);
                    }
                }

                gradient.into()
            })
            .width(Fill)
            .height(Fill);

        let mut stop_row = Row::new().spacing(10).padding(10);
        for (idx, stop) in stops.clone().into_iter().enumerate() {
            if let Some((color, offset)) = stop {
                let of = offset;
                let midx = idx;
                let picker: MElement<Message> =
                    color_picker("Color", color).map(move |new_color| Message::StopChanged {
                        stop: midx,
                        color: new_color,
                        offset: of,
                    });
                let stop_edit: MElement<Message> = column![
                    picker,
                    row![
                        text("offset:").width(64),
                        slider(0.0..=1.0, offset, move |new| Message::StopChanged {
                            stop: idx,
                            color: color,
                            offset: new
                        })
                        .step(0.05)
                    ]
                ]
                .into();

                stop_row = stop_row.push(stop_edit);
            }
        }

        //now add the _add_ thingy
        stop_row = stop_row
            .push(button("Add").on_press({
                let index = stops.iter().enumerate().find_map(|(idx, stop)| {
                    if stop.is_none() {
                        Some(idx)
                    } else {
                        None
                    }
                });

                if let Some(index) = index {
                    //add default stop
                    Message::StopChanged {
                        stop: index,
                        color: Color::from_rgb(0.5, 0.2, 0.2),
                        offset: 1.0,
                    }
                } else {
                    //Do nothing on press, if ther is no stop
                    Message::None
                }
            }))
            .height(Length::Shrink);

        let angle_picker = row![
            text("Angle").width(64),
            slider(Radians::RANGE, self.angle, Message::AngleChanged).step(0.01)
        ]
        .spacing(8)
        .padding(8)
        .align_y(Center);

        let transparency_toggle = iced::widget::Container::new(
            checkbox("Transparent window", transparent).on_toggle(Message::TransparentToggled),
        )
        .padding(8);

        column![stop_row, angle_picker, transparency_toggle, gradient_box].into()
    }

    fn style(&self, theme: &Theme) -> application::Appearance {
        use application::DefaultStyle;

        if self.transparent {
            application::Appearance {
                background_color: Color::TRANSPARENT,
                text_color: theme.palette().text,
            }
        } else {
            Theme::default_style(theme)
        }
    }
}

impl Default for Gradient {
    fn default() -> Self {
        Self::new()
    }
}

fn color_picker(label: &str, color: Color) -> MElement<'_, Color> {
    row![
        text(label).width(64),
        column![
            slider(0.0..=1.0, color.r, move |r| { Color { r, ..color } }).step(0.01),
            slider(0.0..=1.0, color.g, move |g| { Color { g, ..color } }).step(0.01),
            slider(0.0..=1.0, color.b, move |b| { Color { b, ..color } }).step(0.01),
            slider(0.0..=1.0, color.a, move |a| { Color { a, ..color } }).step(0.01)
        ],
    ]
    .spacing(8)
    .padding(8)
    .align_y(Center)
    .into()
}
