use iced::{Background, Color, Point, Rectangle, Transformation};
use iced_core::renderer::Quad;
use iced_graphics::{
    layer,
    text::{Editor, Paragraph},
};

use crate::quad;

pub type Stack = layer::Stack<Layer>;

pub struct Layer {
    pub bounds: Rectangle,
    pub quads: quad::Batch,
    //todo: other things on the layer
}

impl iced_graphics::Layer for Layer {
    fn with_bounds(bounds: Rectangle) -> Self {
        Self {
            bounds,
            ..Self::default()
        }
    }

    fn flush(&mut self) {
        self.flush_meshes();
        self.flush_text();
    }

    fn resize(&mut self, bounds: Rectangle) {
        self.bounds = bounds;
    }

    fn reset(&mut self) {
        self.bounds = Rectangle::INFINITE;

        self.quads.clear();
        /*
        self.triangles.clear();
        self.primitives.clear();
        self.text.clear();
        self.images.clear();
        self.pending_meshes.clear();
        self.pending_text.clear();
        */
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            bounds: Rectangle::INFINITE,
            quads: quad::Batch::default(),
            //triangles: triangle::Batch::default(),
            //primitives: primitive::Batch::default(),
            //text: text::Batch::default(),
            //images: image::Batch::default(),
            //pending_meshes: Vec::new(),
            //pending_text: Vec::new(),
        }
    }
}

impl Layer {
    pub fn draw_quad(
        &mut self,
        quad: Quad,
        background: Background,
        transformation: Transformation,
    ) {
        //Directly transform into object space
        let bounds = quad.bounds * transformation;

        let color = match background {
            Background::Color(c) => c.into_linear(),
            Background::Gradient(g) => {
                log::error!("Gradient not implemented!");
                [1.0, 0.0, 0.0, 1.0]
            }
        };

        //transform into a GPU quad and push
        let quad = iced_marpii_shared::CmdQuad {
            //transform: transformation.into(),
            color,
            position: [bounds.x, bounds.y],
            size: [bounds.width, bounds.height],
            border_color: quad.border.color.into_linear(),
            border_radius: quad.border.radius.into(),
            border_width: quad.border.width,
            shadow_color: quad.shadow.color.into_linear(),
            shadow_offset: quad.shadow.offset.into(),
            shadow_blur_radius: quad.shadow.blur_radius,
        };

        self.quads.add(quad)
    }

    pub fn draw_paragraph(
        &mut self,
        paragraph: &Paragraph,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        log::error!("implement text rendering")
    }

    pub fn draw_editor(
        &mut self,
        editor: &Editor,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        log::error!("implement editor")
    }
    pub fn draw_text(
        &mut self,
        text: iced_core::Text,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        log::error!("implement text")
    }
    pub fn draw_image(&mut self, image: iced_graphics::Image, transformation: Transformation) {
        log::error!("implement image")
    }
    pub fn draw_raster(
        &mut self,
        image: iced_core::Image,
        bounds: Rectangle,
        transformation: Transformation,
    ) {
        log::error!("implement raster")
    }

    pub fn draw_svg(
        &mut self,
        svg: iced_core::Svg,
        bounds: Rectangle,
        transformation: Transformation,
    ) {
        log::error!("implement svg")
    }
    pub fn draw_mesh(&mut self, mut mesh: iced_graphics::Mesh, transformation: Transformation) {
        log::error!("implement mesh")
    }

    pub fn draw_mesh_group(
        &mut self,
        meshes: Vec<iced_graphics::Mesh>,
        transformation: Transformation,
    ) {
        log::error!("implement mesh group")
    }

    pub fn draw_text_group(
        &mut self,
        text: Vec<iced_graphics::Text>,
        transformation: Transformation,
    ) {
        log::error!("implement text group")
    }

    fn flush_meshes(&mut self) {
        log::error!("No flush meshes")
    }

    fn flush_text(&mut self) {
        log::error!("No flush text")
    }
}
