use std::time::Instant;

use iced::widget::container;
use iced::{Border, Color, Element, Fill, Point, Rectangle, Shadow, Subscription, Theme};

type MElement<'a, M> = Element<'a, M, Theme, iced_marpii::Renderer>;
//type MElement<'a, M> = Element<'a, M, Theme, iced_wgpu::Renderer>;

pub fn main() -> iced::Result {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();
    iced::application("A cool counter", Shapes::update, Shapes::view)
        .subscription(Shapes::subscription)
        .run()
}

struct ShapeRenderer {
    start: Instant,
}

impl Default for ShapeRenderer {
    fn default() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl iced_marpii::shape::Program<Message> for ShapeRenderer {
    type State = ();
    fn draw(
        &self,
        _state: &Self::State,
        _renderer: &iced_marpii::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<iced_marpii::shape::Frame> {
        let mut frame = iced_marpii::shape::Frame::with_clip(bounds.shrink(100.0));
        let frame_size = frame.size();
        let center = iced::Point::new(frame_size.width / 2.0, frame_size.height / 2.0);
        let anim = self.start.elapsed().as_secs_f32();
        frame = frame
            .draw_text(iced_marpii::shape::Text {
                content: "Graphics design is my passion!".to_owned(),
                position: center - iced::Vector::new(350.0, -200.0),
                size: 50.into(),
                ..Default::default()
            })
            .draw_line(
                iced_marpii::shape::Line::new(
                    iced::Point::ORIGIN,
                    iced::Point::new(frame_size.width, frame_size.height),
                )
                .thickness(6.0),
            )
            .draw_line(
                iced_marpii::shape::Line::new(
                    center - iced::Vector::new(50.0, 50.0),
                    center + iced::Vector::new(50.0, 50.0),
                )
                .border(
                    Border::default()
                        .color(Color::from_rgb(1.0, 0.0, 1.0))
                        .width(2.0),
                )
                .color(Color::from_rgb(1.0, 1.0, 0.0))
                .thickness(6.0),
            )
            .draw_bezier_spline(
                iced_marpii::shape::Bezier::new(
                    center + iced::Vector::new(-150.0, 50.0),
                    center + iced::Vector::new(-100.0, -50.0),
                    center + iced::Vector::new(-50.0, 50.0),
                )
                .border(Border::default().color(Color::WHITE).width(1.0))
                .shadow(Shadow {
                    color: Color::from_rgba(0.2, 0.2, 0.2, 1.0),
                    offset: iced::Vector::new(20.0, 20.0),
                    blur_radius: 10.0,
                })
                .thickness(6.0),
            )
            .draw_quad(
                iced_core::renderer::Quad {
                    bounds: Rectangle::new(Point::ORIGIN, frame_size),
                    shadow: Shadow {
                        color: Color::from([0.2; 4]),
                        offset: iced::Vector::new(20.0, 20.0),
                        blur_radius: 10.0,
                    },
                    ..Default::default()
                },
                Color::from_rgb(0.25, 0.85, 0.9).into(),
            )
            .draw_quad(
                iced_core::renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new(50.0, 50.0),
                        frame_size - iced::Size::new(100.0, 100.0),
                    ),
                    ..Default::default()
                },
                Color::from_rgb(0.85, 0.25, 0.9).into(),
            )
            .draw_quad(
                iced_core::renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new(100.0, 100.0),
                        frame_size - iced::Size::new(200.0, 200.0),
                    ),
                    ..Default::default()
                },
                Color::from_rgb(0.9, 0.25, 0.25).into(),
            )
            .draw_quad(
                iced_core::renderer::Quad {
                    bounds: Rectangle::new(
                        center - iced::Vector::new(50.0, 5.0),
                        iced::Size {
                            width: 100.0,
                            height: 100.0,
                        },
                    ),
                    ..Default::default()
                },
                Color::from_rgb(0.95, 0.85, 0.0).into(),
            )
            .draw_text(iced_marpii::shape::Text {
                content: "Center".to_owned(),
                position: center,
                ..Default::default()
            });

        vec![frame]
    }
}

#[derive(Default)]
struct Shapes {
    renderer: ShapeRenderer,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Update,
}

impl Shapes {
    fn update(&mut self, _message: Message) {}

    fn view(&self) -> MElement<Message> {
        container(
            iced_marpii::shape::ShapeCanvas::new(&self.renderer)
                .height(Fill)
                .width(Fill),
        )
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(std::time::Duration::from_millis(10)).map(|_| Message::Update)
    }
}
