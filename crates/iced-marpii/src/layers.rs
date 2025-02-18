use iced::{Background, Color, Point, Rectangle, Transformation};
use iced_core::renderer::Quad;
use iced_graphics::{
    layer,
    text::{Editor, Paragraph},
};
use iced_marpii_shared::{CmdQuad, CmdQuadGradient};

use crate::{custom, quad, text};

pub type Stack = layer::Stack<Layer>;

pub struct Layer {
    pub bounds: Rectangle,
    pub solid_quads: quad::Batch<CmdQuad>,
    pub gradient_quads: quad::Batch<CmdQuadGradient>,
    pub text: text::Batch,
    pub custom: custom::primitive::Batch,
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

        self.solid_quads.clear();
        self.gradient_quads.clear();
        self.text.clear();
        self.custom.clear();
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
            solid_quads: quad::Batch::default(),
            gradient_quads: quad::Batch::default(),
            text: text::Batch::default(),
            //NOTE: init without alloc since _most_ layers won't use that.
            custom: custom::primitive::Batch::with_capacity(0),
            //triangles: triangle::Batch::default(),
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

        match background {
            Background::Color(c) => {
                let color = c.into_linear();
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
                //push as solid quad
                self.solid_quads.push(quad);
            }
            Background::Gradient(gradient) => {
                //prepack the gradient-quad, then update all other
                //fields
                let pack = quad::gradient::pack_gradient_quad(gradient, bounds);

                //now fill such a command
                let quad = iced_marpii_shared::CmdQuadGradient {
                    position: [bounds.x, bounds.y],
                    size: [bounds.width, bounds.height],
                    border_color: quad.border.color.into_linear(),
                    border_radius: quad.border.radius.into(),
                    border_width: quad.border.width,
                    shadow_color: quad.shadow.color.into_linear(),
                    shadow_offset: quad.shadow.offset.into(),
                    shadow_blur_radius: quad.shadow.blur_radius,
                    ..pack
                };
                //and push it into the layer
                self.gradient_quads.push(quad);
            }
        }
    }

    pub fn draw_paragraph(
        &mut self,
        paragraph: &Paragraph,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let paragraph = iced_graphics::Text::Paragraph {
            paragraph: paragraph.downgrade(),
            position,
            color,
            clip_bounds,
            transformation,
        };
        self.text.push(paragraph);
    }

    pub fn draw_editor(
        &mut self,
        editor: &Editor,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let editor = iced_graphics::Text::Editor {
            editor: editor.downgrade(),
            position,
            color,
            clip_bounds,
            transformation,
        };

        self.text.push(editor);
    }
    pub fn draw_text(
        &mut self,
        text: iced_core::Text,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let text = iced_graphics::Text::Cached {
            content: text.content,
            bounds: Rectangle::new(position, text.bounds) * transformation,
            color,
            size: text.size * transformation.scale_factor(),
            line_height: text.line_height.to_absolute(text.size) * transformation.scale_factor(),
            font: text.font,
            horizontal_alignment: text.horizontal_alignment,
            vertical_alignment: text.vertical_alignment,
            shaping: text.shaping,
            clip_bounds: clip_bounds * transformation,
        };

        self.text.push(text);
    }

    #[allow(unused)]
    pub fn draw_image(&mut self, _image: iced_graphics::Image, _transformation: Transformation) {
        log::error!("implement image")
    }

    #[allow(unused)]
    pub fn draw_raster(
        &mut self,
        _image: iced_core::Image,
        _bounds: Rectangle,
        _transformation: Transformation,
    ) {
        log::error!("implement raster")
    }

    #[allow(unused)]
    pub fn draw_svg(
        &mut self,
        _svg: iced_core::Svg,
        _bounds: Rectangle,
        _transformation: Transformation,
    ) {
        log::error!("implement svg")
    }

    #[allow(unused)]
    pub fn draw_mesh(&mut self, mut _mesh: iced_graphics::Mesh, _transformation: Transformation) {
        log::error!("implement mesh")
    }

    #[allow(unused)]
    pub fn draw_mesh_group(
        &mut self,
        _meshes: Vec<iced_graphics::Mesh>,
        _transformation: Transformation,
    ) {
        self.flush_meshes();
    }

    #[allow(unused)]
    pub fn draw_text_group(
        &mut self,
        _text: Vec<iced_graphics::Text>,
        _transformation: Transformation,
    ) {
        self.flush_text();
    }

    pub fn draw_primitive(
        &mut self,
        primitive: impl crate::Primitive,
        bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.custom.push(crate::custom::primitive::Instance::new(
            bounds,
            transformation,
            primitive,
        ));
    }

    fn flush_meshes(&mut self) {
        //TODO: Use
        //log::error!("No flush meshes")
    }

    fn flush_text(&mut self) {
        //TODO: use
        //log::error!("No flush text")
    }
}
