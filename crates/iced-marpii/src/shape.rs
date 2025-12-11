//! provides a renderer for additional, SDF based primitives that can be rendered.

use iced::{Background, Border, Color, Point, Rectangle, Shadow, Size, Transformation};
use iced_core::renderer::Quad;
use iced_marpii_shared::{CmdShape, ShapeType};

mod renderer;
pub use renderer::ShapeRenderer;
mod program;
mod solid;
pub use program::{Event, Program, Status};
mod widget;
pub use widget::ShapeCanvas;
mod text;
pub use text::Text;

///Batch of primitives
pub type Batch = crate::batch_cache::Batch<CmdShape>;

///Straight line from `start` to `end`.
#[derive(Debug)]
pub struct Line {
    pub start: Point,
    pub end: Point,
    pub thickness: f32,
    pub color: Color,
    pub border: Border,
    pub shadow: Shadow,
}

impl Line {
    pub fn new(start: Point, end: Point) -> Self {
        Self {
            start,
            end,
            thickness: 1.0,
            color: Color::BLACK,
            border: Border::default(),
            shadow: Shadow::default(),
        }
    }

    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness.max(0.0001);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn border(mut self, border: Border) -> Self {
        self.border = border;
        self
    }
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = shadow;
        self
    }
}

///Cubic bezier spline from `start` over `control_point` to `end`.
#[derive(Debug)]
pub struct Bezier {
    pub start: Point,
    pub control_point: Point,
    pub end: Point,
    pub thickness: f32,
    pub color: Color,
    pub border: Border,
    pub shadow: Shadow,
}

impl Bezier {
    pub fn new(start: Point, control_point: Point, end: Point) -> Self {
        Self {
            start,
            control_point,
            end,
            thickness: 1.0,
            color: Color::BLACK,
            border: Border::default(),
            shadow: Shadow::default(),
        }
    }
    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness.max(0.0001);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn border(mut self, border: Border) -> Self {
        self.border = border;
        self
    }
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = shadow;
        self
    }
}

#[derive(Debug)]
pub enum Shape {
    Line(Line),
    Bezier(Bezier),
}

impl Shape {
    pub fn into_command(self, bounds: Rectangle, transformation: Transformation) -> CmdShape {
        match self {
            Shape::Bezier(Bezier {
                start,
                control_point,
                end,
                thickness,
                color,
                border,
                shadow,
            }) => {
                let start = start * transformation;
                let ctrl = control_point * transformation;
                let end = end * transformation;

                CmdShape {
                    ty: ShapeType::Bezier as u32,
                    pad0: [0; 3],
                    color: color.into_linear(),
                    border_color: border.color.into_linear(),
                    shadow_color: shadow.color.into_linear(),
                    border_width: border.width,
                    shadow_offset: shadow.offset.into(),
                    shadow_blur_radius: shadow.blur_radius,
                    bound_position: [bounds.x, bounds.y],
                    bound_extent: [bounds.width, bounds.height],
                    //Layout from CmdShape docs
                    payload0: [start.x, start.y, ctrl.x, ctrl.y],
                    payload1: [end.x, end.y, thickness, 0.0],
                }
            }
            Shape::Line(Line {
                start,
                end,
                thickness,
                color,
                border,
                shadow,
            }) => {
                let start = start * transformation;
                let end = end * transformation;

                CmdShape {
                    ty: ShapeType::Line as u32,
                    pad0: [0; 3],
                    color: color.into_linear(),
                    border_color: border.color.into_linear(),
                    shadow_color: shadow.color.into_linear(),
                    border_width: border.width,
                    shadow_offset: shadow.offset.into(),
                    shadow_blur_radius: shadow.blur_radius,
                    bound_position: [bounds.x, bounds.y],
                    bound_extent: [bounds.width, bounds.height],
                    //Layout from CmdShape docs
                    payload0: [start.x, start.y, end.x, end.y],
                    payload1: [thickness, 0.0, 0.0, 0.0],
                }
            }
        }
    }
}

///Frame of multiple shapes layered onto each other.
///Coordinates are always relative to the top-left, anything
///in the negative space, or outside of [Self::size] is culled.
pub struct Frame {
    pub(crate) clip_bounds: Rectangle,
    pub(crate) shape: Vec<Shape>,
    pub(crate) quads: Vec<(Quad, Background)>,
    pub(crate) text: Vec<iced_graphics::Text>,
}

impl Frame {
    pub fn new(size: Size) -> Self {
        Self::with_clip(Rectangle::with_size(size))
    }

    pub fn with_clip(bounds: Rectangle) -> Frame {
        Frame {
            clip_bounds: bounds,
            shape: Vec::with_capacity(0),
            quads: Vec::with_capacity(0),
            text: Vec::with_capacity(0),
        }
    }

    pub fn size(&self) -> Size {
        self.clip_bounds.size()
    }

    fn local_transform(&self) -> iced::Transformation {
        iced::Transformation::translate(self.clip_bounds.x, self.clip_bounds.y)
    }

    pub fn draw_quad(mut self, mut quad: Quad, background: Background) -> Self {
        //Translate the quad into our frame's region
        let bound = quad.bounds * self.local_transform();

        if let Some(bound) = self.clip_bounds.intersection(&bound) {
            quad.bounds = bound;
            self.quads.push((quad, background));
        }
        self
    }

    pub fn draw_line(mut self, mut line: Line) -> Self {
        line.start = line.start * self.local_transform();
        line.end = line.end * self.local_transform();

        self.shape.push(Shape::Line(line));
        self
    }

    pub fn draw_bezier_spline(mut self, mut bezier: Bezier) -> Self {
        bezier.start = bezier.start * self.local_transform();
        bezier.control_point = bezier.control_point * self.local_transform();
        bezier.end = bezier.end * self.local_transform();
        self.shape.push(Shape::Bezier(bezier));
        self
    }

    pub fn draw_text(mut self, text: impl Into<Text>) -> Self {
        //Build the cach entry
        let text = text.into();

        let (mut position, size, line_height) = (text.position, text.size, text.line_height);
        position = position * self.local_transform();

        let bounds = Rectangle {
            x: position.x,
            y: position.y,
            width: f32::INFINITY,
            height: f32::INFINITY,
        };

        //Only push text, if it is _within_ our frame
        if let Some(bound) = self.clip_bounds.intersection(&bounds) {
            //Build the cache entry
            self.text.push(iced_graphics::Text::Cached {
                content: text.content,
                bounds: bound,
                color: text.color,
                size,
                line_height: line_height.to_absolute(size),
                font: text.font,
                align_x: text.horizontal_alignment,
                align_y: text.vertical_alignment,
                shaping: text.shaping,
                clip_bounds: self.clip_bounds,
            });
        }

        self
    }
}

pub trait Renderer {
    fn draw_frame(&mut self, frame: Frame);
}
