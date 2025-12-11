pub(crate) mod batch_cache;
pub(crate) mod compositor;
///Widget that allows you to use the underlying [Rmg](marpii_rmg::Rmg) framework.
pub mod custom;
#[cfg(feature = "geometry")]
pub(crate) mod geometry;
pub(crate) mod headless;
pub(crate) mod layers;
pub(crate) mod mesh;
pub(crate) mod quad;
pub(crate) mod renderer;
pub mod shape;
pub(crate) mod text;
pub(crate) mod util;

pub use compositor::Compositor;
pub use custom::{marpii_surface, Event, MarpiiSurface, Persistent, Primitive, Program};
pub use renderer::Renderer;

//Re-export all marpii related stuff, since a user might not want to pull those manually.
pub use marpii;
pub use marpii_rmg;
pub use marpii_rmg_tasks;

///Types you'll need to use the canvas feature with the custom renderer.
#[cfg(feature = "geometry")]
pub mod canvas {
    pub type Cache = iced_graphics::geometry::Cache<super::Renderer>;
    pub type Geometry = <super::Renderer as iced_graphics::geometry::Renderer>::Geometry;
    pub type Frame = iced_graphics::geometry::Frame<super::Renderer>;
}
