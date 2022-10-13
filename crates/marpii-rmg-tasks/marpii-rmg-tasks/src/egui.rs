//! # Egui integration
//!
//! Renders an user defined egui to its inner target image. Note that the `target` image uses an alpha channel. Therefore, the image can easily be
//! rendered on top of an existing image.
//!
//! Have a look at the egui example for an in depth integration.
//!



///Egui render task. Make sure to supply the renderer with all `winit` events that should be taken into account.
pub struct EGuiRender{
    //NOTE: egui uses three main resources to render its interface. A texture atlas, and a vertex/index buffer changing at a high rate
    //      we take our own DynamicBuffer and DynamicImage for those tasks.
    atlas: DynamicI
}
