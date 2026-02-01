use crate::{Compositor, renderer::Renderer};
use iced::Transformation;
use iced_graphics::text::font_system;
use marpii::ash::vk;
use marpii_rmg_tasks::SwapchainPresent;

struct LayerDepth {
    layer_count: usize,
}

impl LayerDepth {
    ///Each layer can have that many sub layer.
    ///
    ///This basically handels, that on a given layer `n`, all quads are _behind_ all text elements etc.
    ///
    ///The implicit ordering is:
    /// Back
    /// - Quad
    /// - Shape
    /// - Mesh,
    /// - Text
    /// - Others
    /// Front
    const SUB_LAYER_COUNT: usize = 5;
    fn layer_depth(&self, layer_index: usize, in_layer_offset: usize) -> f32 {
        let t = (layer_index * Self::SUB_LAYER_COUNT + in_layer_offset) as f32
            / (self.layer_count * Self::SUB_LAYER_COUNT) as f32;

        1.0 - t
    }

    fn quad_depth(&self, layer_index: usize) -> f32 {
        self.layer_depth(layer_index, 0)
    }

    fn shape_depth(&self, layer_index: usize) -> f32 {
        self.layer_depth(layer_index, 1)
    }

    fn mesh_depth(&self, layer_index: usize) -> f32 {
        self.layer_depth(layer_index, 2)
    }

    fn text_depth(&self, layer_index: usize) -> f32 {
        self.layer_depth(layer_index, 3)
    }

    fn custom_depth(&self, layer_index: usize) -> f32 {
        self.layer_depth(layer_index, 4)
    }
}

impl Compositor {
    //Returns true, if the colors need to be gamma corrected
    pub(crate) fn must_gamma_correct_color(format: vk::Format) -> bool {
        !marpii::util::is_srgb(&format)
    }

    ///Data setup step before actually rendering something
    pub fn prepare(&mut self, renderer: &mut Renderer, viewport: &iced_graphics::Viewport) {
        self.quads.begin_new_frame(viewport);
        self.shape.begin_new_frame(viewport);
        self.text.new_frame();
        self.mesh.new_frame();

        let mut font_system = font_system().write().unwrap();
        let font_system = font_system.raw();

        let depth_calc = LayerDepth {
            layer_count: renderer.layers.iter().count(),
        };

        let gamma_correct = Self::must_gamma_correct_color(*self.color_buffer.format());
        self.quads.set_gamma_correct(gamma_correct);
        self.shape.set_gamma_correct(gamma_correct);
        self.mesh.set_gamma_correct(gamma_correct);

        //setup all layers
        for (layer_index, layer) in renderer.layers.iter().enumerate() {
            let quad_depth = depth_calc.quad_depth(layer_index);
            //push all quads of this layer into the quads renderer
            if !layer.solid_quads.is_empty() {
                //TODO: take scaling factor and stuff like that into account
                self.quads.push_solid_batch(
                    &mut self.rmg,
                    &layer.solid_quads,
                    layer.bounds,
                    quad_depth,
                );
            }
            if !layer.gradient_quads.is_empty() {
                //TODO: take scaling factor and stuff like that into account
                self.quads.push_gradient_batch(
                    &mut self.rmg,
                    &layer.gradient_quads,
                    layer.bounds,
                    quad_depth,
                );
            }

            let solid_depth = depth_calc.shape_depth(layer_index);
            if !layer.shapes.is_empty() {
                self.shape.push_solid_batch(
                    &mut self.rmg,
                    &layer.shapes,
                    layer.bounds,
                    solid_depth,
                );
            }

            let mesh_layer = depth_calc.mesh_depth(layer_index);
            if !layer.mesh.is_empty() {
                self.mesh.push_mesh_batch(&layer.mesh, mesh_layer);
            }

            //NOTE: for the custom renderers we don't cache / batch anything,
            //      so we can just call them.
            let custom_layer = depth_calc.custom_depth(layer_index);
            for custom in layer.custom.iter() {
                custom
                    .primitive
                    .try_borrow_mut()
                    .expect("Could not lock custom renderer")
                    .prepare(
                        &mut self.rmg,
                        self.color_buffer.clone(),
                        self.depth_buffer.clone(),
                        &mut self.persistent_data,
                        &custom.bounds,
                        viewport,
                        custom.transformation,
                        custom_layer,
                    );
            }

            let text_depth = depth_calc.text_depth(layer_index);
            if !layer.text.is_empty() {
                self.text.push_batch(
                    &layer.text,
                    &layer.bounds,
                    Transformation::scale(viewport.scale_factor()),
                    text_depth,
                    font_system,
                );
            }
        }

        //after setting up all initial data, schedule all uploads
        self.quads.prepare(&mut self.rmg);
        self.shape.prepare(&mut self.rmg);
        self.text.prepare(&mut self.rmg);
        self.mesh.prepare(&mut self.rmg);
    }

    pub fn render_to_surface(
        &mut self,
        renderer: &mut Renderer,
        surface: &mut SwapchainPresent,
        _viewport: &iced_graphics::Viewport,
        background_color: iced::Color,
        on_pre_present: impl FnOnce(),
    ) {
        //NOTE: while all colors used when rendering _stuff_ are corrected on the GPU,
        // the clear color is directed from the host-site, so we have to make that
        // decission here.
        let bg_color = if Self::must_gamma_correct_color(*self.color_buffer.format()) {
            crate::util::gamma_correct(background_color.into_linear())
        } else {
            background_color.into_linear()
        };

        self.quads.set_clear_color(Some(bg_color));
        //setup new push-image
        surface.push_image(self.color_buffer.clone(), self.color_buffer.extent_2d());

        let mut recorder = self.rmg.record();

        //Custom layers generally direct the rendering themselfs. In order to extend
        let mut custom_layers = renderer
            .layers
            .iter()
            .map(|layer| {
                layer.custom.iter().map(|custom_layer| {
                    let custom_borrow = custom_layer
                        .primitive
                        .try_borrow_mut()
                        .expect("Could not borrow custom-layer's renderer");

                    (
                        layer.bounds.clone(),
                        custom_borrow,
                        custom_layer.transformation.clone(),
                    )
                })
            })
            .flatten()
            .collect::<Vec<_>>();

        for (bounds, custom, transformation) in custom_layers.iter_mut() {
            if custom.is_background() {
                self.quads.set_clear_color(None);
            }

            recorder = custom.render(
                recorder,
                self.color_buffer.clone(),
                self.depth_buffer.clone(),
                &self.persistent_data,
                &bounds,
                *transformation,
            );
        }

        on_pre_present();

        //now schedule all _normal_ passes and flip the swapchain
        recorder
            .add_meta_task(&mut self.quads)
            .unwrap()
            .add_meta_task(&mut self.shape)
            .unwrap()
            .add_meta_task(&mut self.mesh)
            .unwrap()
            .add_task(&mut self.text.renderpass)
            .unwrap()
            .add_task(surface)
            .unwrap()
            .execute()
            .unwrap();
    }

    ///Ends the frame
    pub fn end(&mut self) {
        self.quads.end_frame();
        self.shape.end_frame();
        self.text.end_frame();
        self.mesh.end_frame();
    }

    pub fn notify_resize(&mut self, width: u32, height: u32) {
        //re-create color buffer
        let mut color_desc = self.color_buffer.image_desc().clone();
        color_desc.extent.width = width;
        color_desc.extent.height = height;
        self.color_buffer = self
            .rmg
            .new_image_uninitialized(color_desc, Some("color-buffer"))
            .unwrap();
        //re-create depth buffer
        let mut depth_desc = self.depth_buffer.image_desc().clone();
        depth_desc.extent.width = width;
        depth_desc.extent.height = height;
        self.depth_buffer = self
            .rmg
            .new_image_uninitialized(depth_desc, Some("depth-buffer"))
            .unwrap();

        //now notify all passes
        self.quads
            .notify_resize(self.color_buffer.clone(), self.depth_buffer.clone());
        self.shape
            .notify_resize(self.color_buffer.clone(), self.depth_buffer.clone());
        self.text
            .notify_resize(self.color_buffer.clone(), self.depth_buffer.clone());
        self.mesh
            .notify_resize(self.color_buffer.clone(), self.depth_buffer.clone());
    }
}
