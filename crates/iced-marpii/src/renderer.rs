use iced::{Pixels, Rectangle, Size};

use crate::{layers, shape::Frame};

///MarpII based Iced renderer.
//Note: Most of the gpu sided _logic_ resides in the [Compositor]. The
//      struct mostly handles the Iced sided collection/caching of all data that
//      we supply the compositor _on draw_.
pub struct Renderer {
    default_font: iced_core::Font,
    default_font_size: iced_core::Pixels,
    pub(crate) layers: layers::Stack,
}

impl Renderer {
    pub fn new(settings: &iced_graphics::Settings) -> Self {
        let default_font = settings.default_font.clone();

        Renderer {
            default_font,
            default_font_size: settings.default_text_size,
            layers: layers::Stack::new(),
        }
    }

    pub(crate) fn draw_overlay(
        &mut self,
        overlay: &[impl AsRef<str>],
        viewport: &iced_graphics::Viewport,
    ) {
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
                    align_x: iced_core::text::Alignment::Left,
                    align_y: alignment::Vertical::Top,
                    shaping: iced_core::text::Shaping::Basic,
                    wrapping: iced_core::text::Wrapping::Word,
                };

                renderer.fill_text(
                    text.clone(),
                    iced::Point::new(11.0, 11.0 + 25.0 * i as f32),
                    iced::Color::from_linear_rgba(0.9, 0.9, 0.9, 1.0),
                    Rectangle::with_size(Size::INFINITE),
                );

                renderer.fill_text(
                    text,
                    iced::Point::new(11.0, 11.0 + 25.0 * i as f32) + iced::Vector::new(-1.0, -1.0),
                    iced::Color::BLACK,
                    Rectangle::with_size(Size::INFINITE),
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

    fn reset(&mut self, new_bounds: Rectangle) {
        self.layers.reset(new_bounds);
    }

    fn allocate_image(
        &mut self,
        _handle: &iced_core::image::Handle,
        _callback: impl FnOnce(Result<iced_core::image::Allocation, iced_core::image::Error>)
            + Send
            + 'static,
    ) {
        log::error!("image-allocation unimplemented!");
    }
}

impl iced_core::text::Renderer for Renderer {
    type Font = iced_core::Font;
    type Paragraph = iced_graphics::text::Paragraph;
    type Editor = iced_graphics::text::Editor;

    const ICON_FONT: iced_core::Font = iced_core::Font::with_name("Iced-Icons");
    const CHECKMARK_ICON: char = '\u{f00c}';
    const ARROW_DOWN_ICON: char = '\u{e800}';
    const ICED_LOGO: char = '\u{e801}';
    const SCROLL_UP_ICON: char = '\u{e802}';
    const SCROLL_DOWN_ICON: char = '\u{e803}';
    const SCROLL_LEFT_ICON: char = '\u{e804}';
    const SCROLL_RIGHT_ICON: char = '\u{e805}';

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

impl iced_graphics::mesh::Renderer for crate::renderer::Renderer {
    fn draw_mesh(&mut self, mesh: iced_graphics::Mesh) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_mesh(mesh, transformation);
    }

    fn draw_mesh_cache(&mut self, cache: iced_graphics::mesh::Cache) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_mesh_cache(cache, transformation);
    }
}

#[cfg(feature = "geometry")]
impl iced_graphics::geometry::Renderer for crate::renderer::Renderer {
    type Frame = crate::geometry::Frame;
    type Geometry = crate::geometry::Geometry;

    fn new_frame(&self, bounds: Rectangle) -> Self::Frame {
        crate::geometry::Frame::new(bounds)
    }

    fn draw_geometry(&mut self, geometry: Self::Geometry) {
        let (layer, transformation) = self.layers.current_mut();

        match geometry {
            crate::geometry::Geometry::Live {
                meshes,
                images,
                text,
            } => {
                layer.draw_mesh_group(meshes, transformation);

                for image in images {
                    layer.draw_image(image, transformation);
                }

                layer.draw_text_group(text, transformation);
            }
            crate::geometry::Geometry::Cached(cache) => {
                if !cache.meshes.is_empty() {
                    layer.draw_mesh_group(cache.meshes, transformation);
                    //layer.draw_mesh_cache(cache., transformation);
                }

                /*TODO: Image
                if let Some(images) = cache.images {
                    for image in images.iter().cloned() {
                        layer.draw_image(image, transformation);
                    }
                }
                */

                if !cache.text.is_empty() {
                    layer.draw_text_cache(cache.text, transformation);
                }
            }
        }
    }
}

impl crate::shape::Renderer for Renderer {
    fn draw_frame(&mut self, frame: crate::shape::Frame) {
        let Frame {
            clip_bounds,
            shape,
            quads,
            text,
        } = frame;

        //get current layer
        let (layer, transformation) = self.layers.current_mut();

        //add the frame's local transformation to the layer's
        //submit each
        for shape in shape {
            layer.draw_shape(shape, clip_bounds, transformation);
        }
        for (quad, background) in quads {
            layer.draw_quad(quad, background, transformation);
        }

        layer.draw_text_group(text, transformation);
    }
}

impl crate::custom::Renderer for Renderer {
    fn draw_primitive(&mut self, bounds: iced::Rectangle, primitive: impl super::Primitive) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_primitive(primitive, bounds, transformation);
    }
}

impl iced_graphics::compositor::Default for crate::renderer::Renderer {
    type Compositor = crate::Compositor;
}
