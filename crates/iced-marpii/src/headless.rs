use crate::Renderer;

impl iced_core::renderer::Headless for Renderer {
    async fn new(
        _default_font: iced::Font,
        _default_text_size: iced::Pixels,
        _backend: Option<&str>,
    ) -> Option<Self> {
        None
    }

    fn name(&self) -> String {
        "MarpII-Iced headless".to_string()
    }

    fn screenshot(
        &mut self,
        _size: iced::Size<u32>,
        _scale_factor: f32,
        _background_color: iced::Color,
    ) -> Vec<u8> {
        vec![]
    }
}
