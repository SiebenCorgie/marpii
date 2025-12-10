use std::future::Future;

use crate::Renderer;

impl iced_core::renderer::Headless for Renderer {
    async fn new(
        default_font: iced::Font,
        default_text_size: iced::Pixels,
        backend: Option<&str>,
    ) -> Option<Self> {
        None
    }

    fn name(&self) -> String {
        "MarpII-Iced headless".to_string()
    }

    fn screenshot(
        &mut self,
        size: iced::Size<u32>,
        scale_factor: f32,
        background_color: iced::Color,
    ) -> Vec<u8> {
        vec![]
    }
}
