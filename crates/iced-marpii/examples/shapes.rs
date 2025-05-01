use iced::widget::container;
use iced::{Border, Color, Element, Fill, Shadow, Theme};

type MElement<'a, M> = Element<'a, M, Theme, iced_marpii::Renderer>;
//type MElement<'a, M> = Element<'a, M, Theme, iced_wgpu::Renderer>;

pub fn main() -> iced::Result {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .init()
        .unwrap();

    iced::run("A cool counter", Shapes::update, Shapes::view)
}

#[derive(Default)]
struct ShapeRenderer;
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
        let frame = iced_marpii::shape::Frame::with_clip(bounds)
            .draw_text(iced_marpii::shape::Text {
                content: "Graphics design is my passion!".to_owned(),
                position: bounds.center() - iced::Vector::new(350.0, -200.0),
                size: 50.into(),

                ..Default::default()
            })
            .draw_line(
                iced_marpii::shape::Line::new(
                    bounds.center() - iced::Vector::new(50.0, 50.0),
                    bounds.center() + iced::Vector::new(50.0, 50.0),
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
                    bounds.center() + iced::Vector::new(-150.0, 50.0),
                    bounds.center() + iced::Vector::new(-100.0, -50.0),
                    bounds.center() + iced::Vector::new(-50.0, 50.0),
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
                    bounds: bounds.shrink(100.0),
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
                    bounds: bounds.shrink(150.0),
                    ..Default::default()
                },
                Color::from_rgb(0.85, 0.25, 0.9).into(),
            )
            .draw_quad(
                iced_core::renderer::Quad {
                    bounds: bounds.shrink(200.0),
                    ..Default::default()
                },
                Color::from_rgb(0.9, 0.25, 0.25).into(),
            );

        vec![frame]
    }
}

#[derive(Default)]
struct Shapes {
    renderer: ShapeRenderer,
}

#[derive(Debug, Clone, Copy)]
enum Message {}

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
}
