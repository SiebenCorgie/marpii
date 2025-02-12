mod compositor;
mod layers;
mod quad;
pub use compositor::Compositor;

///MarpII based Iced renderer.
//Note: Most of the gpu sided _logic_ resides in the [Compositor]. The
//      struct mostly handles the Iced sided collection/caching of all data that
//      we supply the compositor _on draw_.
pub struct Renderer {
    default_font: iced_core::Font,
    default_font_size: iced_core::Pixels,
}

impl Renderer {
    pub fn new(settings: &iced_graphics::Settings) -> Self {
        Renderer {
            default_font: settings.default_font.clone(),
            default_font_size: settings.default_text_size,
        }
    }
}

impl iced_core::Renderer for Renderer {
    fn start_layer(&mut self, bounds: iced::Rectangle) {
        todo!()
    }
    fn end_layer(&mut self) {
        todo!()
    }

    fn start_transformation(&mut self, transformation: iced::Transformation) {
        todo!()
    }

    fn end_transformation(&mut self) {
        todo!()
    }

    fn fill_quad(
        &mut self,
        quad: iced_core::renderer::Quad,
        background: impl Into<iced::Background>,
    ) {
        todo!()
    }

    fn clear(&mut self) {
        todo!()
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
        todo!()
    }

    fn fill_editor(
        &mut self,
        editor: &Self::Editor,
        position: iced::Point,
        color: iced::Color,
        clip_bounds: iced::Rectangle,
    ) {
        todo!()
    }

    fn fill_text(
        &mut self,
        text: iced_core::Text<String, Self::Font>,
        position: iced::Point,
        color: iced::Color,
        clip_bounds: iced::Rectangle,
    ) {
        todo!()
    }
}

impl iced_graphics::compositor::Default for crate::Renderer {
    type Compositor = compositor::Compositor;
}
