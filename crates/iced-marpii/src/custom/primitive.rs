//! Draw custom primitives.
use iced::Transformation;
use iced_core::{self, Rectangle};
use marpii_rmg::{ImageHandle, Recorder, Rmg};

use super::Persistent;

/// A batch of primitives.
pub type Batch = Vec<Instance>;

pub type Viewport = iced_graphics::Viewport;

/// A set of methods which allows a [`Primitive`] to be rendered.
///
/// Note that instance of this [Primitive] are rapidly created and destroyed while rendering. So any persistant
/// data schould be stored in the `State` component of the emitting [Program](crate::custom::Program).
pub trait Primitive: Send + Sync + 'static {
    /// Processes the [`Primitive`], allowing for GPU buffer allocation.
    fn prepare(
        &mut self,
        rmg: &mut Rmg,
        color_image: ImageHandle,
        depth_image: ImageHandle,
        persistent: &mut Persistent,
        bounds: &Rectangle,
        viewport: &Viewport,
        transform: Transformation,
        layer_depth: f32,
        should_gamma_correct: bool,
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
        &'a mut self,
        recorder: Recorder<'a>,
        color_image: ImageHandle,
        depth_image: ImageHandle,
        persistent: &Persistent,
        clip_bounds: &Rectangle,
        transform: Transformation,
    ) -> Recorder<'a>;
}

/// An instance of a specific [`Primitive`].
pub struct Instance {
    /// The bounds of the [`Instance`].
    pub bounds: Rectangle,

    /// The [`Primitive`] to render.
    pub primitive: Box<dyn Primitive>,

    pub transformation: Transformation,
}

impl Instance {
    /// Creates a new [`Instance`] with the given [`Primitive`].
    pub fn new(
        bounds: Rectangle,
        transformation: Transformation,
        primitive: impl Primitive,
    ) -> Self {
        Instance {
            bounds,
            primitive: Box::new(primitive),
            transformation,
        }
    }
}

/// A renderer that can draw custom primitives.
pub trait Renderer: iced_core::Renderer {
    /// Draws a custom primitive.
    fn draw_primitive(&mut self, bounds: Rectangle, primitive: impl Primitive);
}
