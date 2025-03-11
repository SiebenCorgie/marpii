use iced::{alignment, Rectangle, Transformation};
use std::sync::Arc;

pub enum BufferAllocation {
    Paragraph(iced_graphics::text::Paragraph),
    Editor(iced_graphics::text::Editor),
    Raw(Arc<cosmic_text::Buffer>),
    Cache(iced_graphics::text::cache::KeyHash),
}

///Designates an area, backed by the cosmic-text buffer, where the system want
///to draw text.
pub struct TextArea {
    pub buffer: BufferAllocation,
    pub bounds: Rectangle,
    pub color: iced_core::Color,
    pub scale: f32,
    pub left: f32,
    pub top: f32,
}

impl TextArea {
    pub fn from_text(
        section: &iced_graphics::Text,
        layer_bounds: Rectangle,
        layer_transformation: Transformation,
        cache: &mut iced_graphics::text::Cache,
        font_system: &mut cosmic_text::FontSystem,
    ) -> Option<TextArea> {
        let (buffer, bounds, halign, valign, color, clip_bounds, transformation) = match section {
            iced_graphics::Text::Paragraph {
                paragraph,
                position,
                color,
                clip_bounds,
                transformation,
            } => {
                let valign = paragraph.vertical_alignment.clone();
                let halign = paragraph.horizontal_alignment.clone();
                let minbounds = paragraph.min_bounds.clone();

                //after extracting everything, snack the reference and be done!
                let buffer = BufferAllocation::Paragraph(paragraph.upgrade().unwrap());
                (
                    buffer,
                    Rectangle::new(*position, minbounds),
                    halign,
                    valign,
                    *color,
                    *clip_bounds,
                    *transformation,
                )
            }
            iced_graphics::Text::Editor {
                editor,
                position,
                color,
                clip_bounds,
                transformation,
            } => {
                let valign = alignment::Vertical::Top;
                let halign = alignment::Horizontal::Left;

                (
                    BufferAllocation::Editor(editor.upgrade().unwrap()),
                    Rectangle::new(*position, editor.bounds),
                    halign,
                    valign,
                    *color,
                    *clip_bounds,
                    *transformation,
                )
            }
            iced_graphics::Text::Raw {
                raw,
                transformation,
            } => {
                let (width, height) = raw.buffer.upgrade().unwrap().size();

                (
                    BufferAllocation::Raw(raw.buffer.upgrade().unwrap()),
                    Rectangle::new(
                        raw.position,
                        iced_core::Size::new(
                            width.unwrap_or(layer_bounds.width),
                            height.unwrap_or(layer_bounds.height),
                        ),
                    ),
                    alignment::Horizontal::Left,
                    alignment::Vertical::Top,
                    raw.color,
                    raw.clip_bounds,
                    *transformation,
                )
            }
            iced_graphics::Text::Cached {
                content,
                bounds,
                color,
                size,
                line_height,
                font,
                horizontal_alignment,
                vertical_alignment,
                shaping,
                clip_bounds,
            } => {
                //some adhock text. Use the cache to setup this buffer
                let (key, _) = cache.allocate(
                    font_system,
                    iced_graphics::text::cache::Key {
                        content,
                        size: f32::from(*size),
                        line_height: f32::from(*line_height),
                        font: *font,
                        bounds: iced_core::Size {
                            width: bounds.width,
                            height: bounds.height,
                        },
                        shaping: *shaping,
                    },
                );

                //directly load it, in order to read out the bounds
                let entry = cache.get(&key).expect("Expected it to be cached!");

                (
                    BufferAllocation::Cache(key),
                    Rectangle::new(bounds.position(), entry.min_bounds),
                    *horizontal_alignment,
                    *vertical_alignment,
                    *color,
                    *clip_bounds,
                    Transformation::IDENTITY,
                )
            }
        };

        //TODO: find out what glyphon is doing with the top/left thingy and alignment.

        let bounds = bounds * transformation * layer_transformation;
        let left = match halign {
            alignment::Horizontal::Left => bounds.x,
            alignment::Horizontal::Center => bounds.x - bounds.width / 2.0,
            alignment::Horizontal::Right => bounds.x - bounds.width,
        };

        let top = match valign {
            alignment::Vertical::Top => bounds.y,
            alignment::Vertical::Center => bounds.y - bounds.height / 2.0,
            alignment::Vertical::Bottom => bounds.y - bounds.height,
        };
        //NOTE: if there is no intersection, the area will be culled
        let clip_bounds =
            layer_bounds.intersection(&(clip_bounds * transformation * layer_transformation))?;
        let ta = TextArea {
            buffer,
            scale: transformation.scale_factor() * layer_transformation.scale_factor(),
            bounds: clip_bounds,
            color,
            left,
            top,
        };

        Some(ta)
    }

    #[allow(unused)]
    pub fn bound_left(&self) -> i32 {
        self.bounds.x as i32
    }

    #[allow(unused)]
    pub fn bound_top(&self) -> i32 {
        self.bounds.y as i32
    }

    #[allow(unused)]
    pub fn bound_right(&self) -> i32 {
        (self.bounds.x + self.bounds.width) as i32
    }

    #[allow(unused)]
    pub fn bound_bottom(&self) -> i32 {
        (self.bounds.y + self.bounds.height) as i32
    }

    pub fn buffer<'a>(&'a self, cache: &'a iced_graphics::text::Cache) -> &'a cosmic_text::Buffer {
        match &self.buffer {
            BufferAllocation::Paragraph(p) => p.buffer(),
            BufferAllocation::Editor(e) => e.buffer(),
            BufferAllocation::Raw(b) => b.as_ref(),
            BufferAllocation::Cache(key) => cache.get(key).map(|e| &e.buffer).unwrap(),
        }
    }
}
