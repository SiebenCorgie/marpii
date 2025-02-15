use crate::{renderer::Renderer, Compositor};
use iced::Transformation;
use iced_graphics::text::font_system;
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
    /// - Text
    /// - Others
    /// Front
    const SUB_LAYER_COUNT: usize = 3;
    fn layer_depth(&self, layer_index: usize, in_layer_offset: usize) -> f32 {
        let t = (layer_index * Self::SUB_LAYER_COUNT + in_layer_offset) as f32
            / (self.layer_count * Self::SUB_LAYER_COUNT) as f32;

        1.0 - t
    }

    fn quad_depth(&self, layer_index: usize) -> f32 {
        self.layer_depth(layer_index, 0)
    }

    fn text_depth(&self, layer_index: usize) -> f32 {
        self.layer_depth(layer_index, 1)
    }

    fn custom_depth(&self, layer_index: usize) -> f32 {
        self.layer_depth(layer_index, 2)
    }
}

impl Compositor {
    ///Data setup step before actually rendering something
    pub fn prepare(&mut self, renderer: &mut Renderer, viewport: &iced_graphics::Viewport) {
        self.quads.begin_new_frame(viewport);
        self.text.new_frame();

        let mut font_system = font_system().write().unwrap();
        let font_system = font_system.raw();

        let depth_calc = LayerDepth {
            layer_count: renderer.layers.iter().count(),
        };

        //setup all layers
        for (layer_index, layer) in renderer.layers.iter_mut().enumerate() {
            let quad_depth = depth_calc.quad_depth(layer_index);
            //push all quads of this layer into the quads renderer
            if layer.quads.order.len() > 0 {
                //TODO: take scaling factor and stuff like that into account
                self.quads
                    .push_batch(&mut self.rmg, &layer.quads, layer.bounds, quad_depth);
            }

            //NOTE: for the custom renderers we don't cache / batch anything,
            //      so we can just call them.
            let custom_layer = depth_calc.custom_depth(layer_index);
            for custom in layer.custom.iter_mut() {
                custom.primitive.prepare(
                    &mut self.rmg,
                    self.color_buffer.clone(),
                    self.depth_buffer.clone(),
                    &custom.bounds,
                    viewport,
                    custom.transformation,
                    custom_layer,
                );
            }

            let text_depth = depth_calc.text_depth(layer_index);
            if layer.text.len() > 0 {
                self.text.push_batch(
                    &layer.text,
                    &layer.bounds,
                    Transformation::scale(viewport.scale_factor() as f32),
                    text_depth,
                    font_system,
                );
            }
        }

        //after setting up all initial data, schedule all uploads
        self.quads.prepare_data(&mut self.rmg);
        self.text.prepare(&mut self.rmg);
    }

    pub fn render_to_surface(
        &mut self,
        renderer: &mut Renderer,
        surface: &mut SwapchainPresent,
        _viewport: &iced_graphics::Viewport,
        background_color: iced::Color,
    ) {
        self.quads
            .set_clear_color(Some(background_color.into_linear()));
        //setup new push-image
        surface.push_image(self.color_buffer.clone(), self.color_buffer.extent_2d());

        let mut recorder = self.rmg.record();
        //first cycle all custom recording
        for layer in renderer.layers.iter_mut() {
            for custom in &mut layer.custom {
                if custom.primitive.is_background() {
                    self.quads.set_clear_color(None);
                }

                recorder = custom.primitive.render(
                    recorder,
                    self.color_buffer.clone(),
                    self.depth_buffer.clone(),
                    &layer.bounds,
                    custom.transformation,
                );
            }
        }

        //now schedule all _normal_ passes and flip the swapchain
        recorder
            .add_meta_task(&mut self.quads)
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
        self.text.end_frame();
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
        self.text
            .notify_resize(self.color_buffer.clone(), self.depth_buffer.clone());
    }
}
