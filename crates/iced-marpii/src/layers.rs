use std::usize;

use iced::{Background, Color, Point, Rectangle, Transformation};
use iced_core::renderer::Quad;
use iced_graphics::{
    layer,
    text::{Editor, Paragraph},
    Mesh,
};
use iced_marpii_shared::{CmdQuad, CmdQuadGradient};

use crate::{
    batch_cache, custom, mesh, quad,
    shape::{self, Shape},
    text,
};

pub type Stack = layer::Stack<Layer>;

pub struct Layer {
    pub bounds: Rectangle,
    pub solid_quads: batch_cache::Batch<CmdQuad>,
    pub gradient_quads: batch_cache::Batch<CmdQuadGradient>,
    pub text: text::Batch,
    pub custom: custom::primitive::Batch,
    pub mesh: mesh::Batch,
    pub shapes: shape::Batch,
    //todo: other things on the layer
}

impl iced_graphics::Layer for Layer {
    fn with_bounds(bounds: Rectangle) -> Self {
        Self {
            bounds,
            ..Self::default()
        }
    }

    fn bounds(&self) -> Rectangle {
        self.bounds
    }

    fn start(&self) -> usize {
        if !self.solid_quads.is_empty() || !self.gradient_quads.is_empty() {
            return 1;
        }

        if !self.shapes.is_empty() {
            return 2;
        }

        if !self.mesh.is_empty() {
            return 3;
        }

        if !self.text.is_empty() {
            return 4;
        }

        if !self.custom.is_empty() {
            return 5;
        }
        usize::MAX
    }

    fn end(&self) -> usize {
        if !self.custom.is_empty() {
            return 5;
        }
        if !self.text.is_empty() {
            return 4;
        }
        if !self.mesh.is_empty() {
            return 3;
        }
        if !self.shapes.is_empty() {
            return 2;
        }
        if !self.solid_quads.is_empty() || !self.gradient_quads.is_empty() {
            return 1;
        }

        0
    }

    fn merge(&mut self, layer: &mut Self) {
        self.solid_quads.append(&mut layer.solid_quads);
        self.gradient_quads.append(&mut layer.gradient_quads);
        self.shapes.append(&mut layer.shapes);
        self.mesh.append(&mut layer.mesh);
        self.text.append(&mut layer.text);
        self.custom.append(&mut layer.custom);
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
        self.shapes.clear();
        self.text.clear();
        self.custom.clear();
        self.mesh.clear();
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            bounds: Rectangle::INFINITE,
            solid_quads: batch_cache::Batch::default(),
            gradient_quads: batch_cache::Batch::default(),
            text: text::Batch::default(),
            //NOTE: init without alloc since _most_ layers won't use that.
            custom: custom::primitive::Batch::with_capacity(0),
            mesh: mesh::Batch::with_capacity(0),
            shapes: shape::Batch::with_capacity(0),
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
            align_x: text.align_x,
            align_y: text.align_y,
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

    pub fn draw_mesh(&mut self, mut mesh: iced_graphics::Mesh, transformation: Transformation) {
        //Update the meshe's transform to the layers
        match &mut mesh {
            Mesh::Solid {
                transformation: local_transformation,
                clip_bounds: local_bounds,
                ..
            }
            | Mesh::Gradient {
                transformation: local_transformation,
                clip_bounds: local_bounds,
                ..
            } => {
                *local_transformation = *local_transformation * transformation;
                local_bounds.x += local_transformation.translation().x;
                local_bounds.y += local_transformation.translation().y;
                local_bounds.width *= local_transformation.scale_factor();
                local_bounds.height *= local_transformation.scale_factor();
            }
        }

        //and push to the layer
        self.mesh.push(mesh);
    }

    pub fn draw_mesh_cache(
        &mut self,
        cache: iced_graphics::mesh::Cache,
        transformation: Transformation,
    ) {
        self.flush_meshes();

        log::debug!("Ignoring mesh cache");

        //update transformation for all meshes and push them
        for mesh in cache.batch() {
            //TODO: implement mesh caching
            self.draw_mesh(mesh.clone(), transformation);
        }
    }

    #[allow(dead_code)]
    pub fn draw_mesh_group(
        &mut self,
        meshes: Vec<iced_graphics::Mesh>,
        transformation: Transformation,
    ) {
        self.flush_meshes();
        for mesh in meshes {
            self.draw_mesh(mesh, transformation.clone());
        }
    }

    pub fn draw_text_group(
        &mut self,
        text: Vec<iced_graphics::Text>,
        transformation: Transformation,
    ) {
        self.flush_text();
        for mut text in text {
            match &mut text {
                iced_graphics::Text::Cached {
                    bounds,
                    clip_bounds,
                    ..
                } => {
                    *bounds = *bounds * transformation;
                    *clip_bounds = *clip_bounds * transformation;
                }
                iced_graphics::Text::Raw {
                    transformation: subtrans,
                    ..
                }
                | iced_graphics::Text::Editor {
                    transformation: subtrans,
                    ..
                }
                | iced_graphics::Text::Paragraph {
                    transformation: subtrans,
                    ..
                } => *subtrans = *subtrans * transformation,
            }
            self.text.push(text);
        }
    }

    #[cfg(feature = "geometry")]
    pub fn draw_text_cache(
        &mut self,
        text: Vec<iced_graphics::Text>,
        transformation: Transformation,
    ) {
        //TODO: implement this type of caching?
        self.draw_text_group(text, transformation);
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

    pub fn draw_shape(&mut self, shape: Shape, bounds: Rectangle, transformation: Transformation) {
        let bounds = bounds * transformation;
        //Morph bounds into scree-space via layer's transformation
        let shape_command = shape.into_command(bounds, transformation);
        self.shapes.push(shape_command);
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
