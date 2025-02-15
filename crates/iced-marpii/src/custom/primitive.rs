//! Draw custom primitives.
use iced_core::{self, Rectangle};
use iced_graphics::Viewport;
use marpii_rmg::{ImageHandle, Recorder, Rmg};

use std::fmt::Debug;

/// A batch of primitives.
pub type Batch = Vec<Instance>;

/// A set of methods which allows a [`Primitive`] to be rendered.
pub trait Primitive: Debug + Send + Sync + 'static {
    /// Processes the [`Primitive`], allowing for GPU buffer allocation.
    fn prepare(
        &self,
        rmg: &mut Rmg,
        color_image: ImageHandle,
        depth_image: ImageHandle,
        bounds: &Rectangle,
        viewport: &Viewport,
    );

    ///If this returns true, the primitive will be considered the background
    ///renderer of the application.
    ///
    ///This mean that the background is not cleared at the start of a frame,
    ///but `render` of this primitive is called. The primitive's job is then to
    ///initialize the whole `color_image`, and reset the `depth_image` to _something_, usually `1.0`.
    fn is_background(&self) -> bool {
        false
    }

    /// Renders the [`Primitive`].
    ///
    ///The `layer_depth` represents the expected `depth_image` value this primitive
    ///would/should be compared too. If you don't draw your pixels to that depth value
    /// content might glitch. Depending on what you are doing though, this might be wanted.
    fn render<'a>(
        &mut self,
        recorder: Recorder<'a>,
        color_image: ImageHandle,
        depth_image: ImageHandle,
        clip_bounds: &Rectangle<u32>,
        layer_depth: f32,
    ) -> Recorder<'a>;
}

#[derive(Debug)]
/// An instance of a specific [`Primitive`].
pub struct Instance {
    /// The bounds of the [`Instance`].
    pub bounds: Rectangle,

    /// The [`Primitive`] to render.
    pub primitive: Box<dyn Primitive>,
}

impl Instance {
    /// Creates a new [`Instance`] with the given [`Primitive`].
    pub fn new(bounds: Rectangle, primitive: impl Primitive) -> Self {
        Instance {
            bounds,
            primitive: Box::new(primitive),
        }
    }
}

/// A renderer that can draw custom primitives.
pub trait Renderer: iced_core::Renderer {
    /// Draws a custom primitive.
    fn draw_primitive(&mut self, bounds: Rectangle, primitive: impl Primitive);
}
