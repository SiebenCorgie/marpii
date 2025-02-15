impl super::Renderer for crate::renderer::Renderer {
    fn draw_primitive(&mut self, bounds: iced::Rectangle, primitive: impl super::Primitive) {
        println!("Hello from renderer!")
    }
}
