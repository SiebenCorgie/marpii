mod compositor;
mod layers;
mod quad;
pub use compositor::Compositor;
use iced::{Pixels, Rectangle, Size};

///MarpII based Iced renderer.
//Note: Most of the gpu sided _logic_ resides in the [Compositor]. The
//      struct mostly handles the Iced sided collection/caching of all data that
//      we supply the compositor _on draw_.
pub struct Renderer {
    default_font: iced_core::Font,
    default_font_size: iced_core::Pixels,
    layers: layers::Stack,
}

impl Renderer {
    pub fn new(settings: &iced_graphics::Settings) -> Self {
        Renderer {
            default_font: settings.default_font.clone(),
            default_font_size: settings.default_text_size,
            layers: layers::Stack::new(),
        }
    }

    fn draw_overlay(&mut self, overlay: &[impl AsRef<str>], viewport: &iced_graphics::Viewport) {
        use iced_core::alignment;
        use iced_core::text::Renderer as _;
        use iced_core::Renderer as _;

        self.with_layer(Rectangle::with_size(viewport.logical_size()), |renderer| {
            for (i, line) in overlay.iter().enumerate() {
                let text = iced_core::Text {
                    content: line.as_ref().to_owned(),
                    bounds: viewport.logical_size(),
                    size: Pixels(20.0),
                    line_height: iced_core::text::LineHeight::default(),
                    font: iced_core::Font::MONOSPACE,
                    horizontal_alignment: alignment::Horizontal::Left,
                    vertical_alignment: alignment::Vertical::Top,
                    shaping: iced_core::text::Shaping::Basic,
                    wrapping: iced_core::text::Wrapping::Word,
                };

                renderer.fill_text(
                    text.clone(),
                    iced::Point::new(11.0, 11.0 + 25.0 * i as f32),
                    iced::Color::new(0.9, 0.9, 0.9, 1.0),
                    Rectangle::with_size(Size::INFINITY),
                );

                renderer.fill_text(
                    text,
                    iced::Point::new(11.0, 11.0 + 25.0 * i as f32) + iced::Vector::new(-1.0, -1.0),
                    iced::Color::BLACK,
                    Rectangle::with_size(Size::INFINITY),
                );
            }
        });
    }
}

impl iced_core::Renderer for Renderer {
    fn start_layer(&mut self, bounds: iced::Rectangle) {
        self.layers.push_clip(bounds);
    }
    fn end_layer(&mut self) {
        self.layers.pop_clip();
    }

    fn start_transformation(&mut self, transformation: iced::Transformation) {
        self.layers.push_transformation(transformation);
    }

    fn end_transformation(&mut self) {
        self.layers.pop_transformation();
    }

    fn fill_quad(
        &mut self,
        quad: iced_core::renderer::Quad,
        background: impl Into<iced::Background>,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_quad(quad, background.into(), transformation);
    }

    fn clear(&mut self) {
        self.layers.clear();
    }
}

impl iced_core::text::Renderer for Renderer {
    type Font = iced_core::Font;
    type Paragraph = iced_graphics::text::Paragraph;
    type Editor = iced_graphics::text::Editor;

    const ICON_FONT: iced_core::Font = iced_core::Font::with_name("Iced-Icons");
    const CHECKMARK_ICON: char = '\u{f00c}';
    const ARROW_DOWN_ICON: char = '\u{e800}';

    fn default_font(&self) -> Self::Font {
        self.default_font
    }

    fn default_size(&self) -> iced::Pixels {
        self.default_font_size
    }

    fn fill_paragraph(
        &mut self,
        text: &Self::Paragraph,
        position: iced::Point,
        color: iced::Color,
        clip_bounds: iced::Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();

        layer.draw_paragraph(text, position, color, clip_bounds, transformation);
    }

    fn fill_editor(
        &mut self,
        editor: &Self::Editor,
        position: iced::Point,
        color: iced::Color,
        clip_bounds: iced::Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_editor(editor, position, color, clip_bounds, transformation);
    }

    fn fill_text(
        &mut self,
        text: iced_core::Text<String, Self::Font>,
        position: iced::Point,
        color: iced::Color,
        clip_bounds: iced::Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_text(text, position, color, clip_bounds, transformation);
    }
}

impl iced_graphics::compositor::Default for crate::Renderer {
    type Compositor = compositor::Compositor;
}
