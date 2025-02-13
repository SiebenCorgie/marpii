use marpii_rmg_tasks::SwapchainPresent;

use crate::Renderer;

use super::Compositor;

impl Compositor {
    ///Data setup step before actually rendering something
    pub fn prepare(&mut self, renderer: &mut Renderer, viewport: &iced_graphics::Viewport) {
        self.quads.begin_new_frame(viewport);
        //setup all layers
        for layer in renderer.layers.iter() {
            //push all quads of this layer into the quads renderer
            self.quads
                .push_batch(&mut self.rmg, &layer.quads, layer.bounds);
        }

        //after setting up all initial data, schedule all uploads
        self.quads.prepare_data(&mut self.rmg);
    }

    pub fn render_to_surface(
        &mut self,
        surface: &mut SwapchainPresent,
        _viewport: &iced_graphics::Viewport,
        _background_color: iced::Color,
    ) {
        //setup new push-image
        surface.push_image(self.color_buffer.clone(), self.color_buffer.extent_2d());
        //now schedule all passes and flip the swapchain
        self.rmg
            .record()
            .add_meta_task(&mut self.quads)
            .unwrap()
            .add_task(surface)
            .unwrap()
            .execute()
            .unwrap();
    }

    ///Ends the frame
    pub fn end(&mut self) {
        self.quads.end_frame();
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
        //now notify all passes
        self.quads.notify_resize(self.color_buffer.clone());
    }
}
