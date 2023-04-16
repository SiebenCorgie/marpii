//! # MarpII
//!
//! 🚧 Todo: Write some general words and a small example on how to get started. 🚧
//!
//! # Usage
//!
//! In general MarpII provides you with transparent wrappers around the main Vulkan objects. This includes the [Instance](context::Instance), [Device](context::Device) and other lifetime sensitive structures. Those wrappers, if used, keep track of lifetimes
//! and destruction of those objects when not needed anymore. Usually there are some helpers to simplify the creation of
//! those. They can however also be created by hand.
//!
//!
//! Structures that are not sensitive to lifetime requirements (like create info) are not wrapped.
#![deny(warnings)]
#![feature(vec_into_raw_parts)]

pub use ash;
#[cfg(feature = "default_allocator")]
pub use gpu_allocator;

#[cfg(feature = "bytemuck")]
pub use bytemuck;

///Owned-Or-Shared wrapper. Allows us to implement generic over a type that might be owned or shared via [Arc](std::sync::Arc).
///
/// Note that you can convert from and into this type from Arcs and any value T.
///
/// If the context allows for the assumption that something is shared, a normal Arc should be preffered.
pub use oos::OoS;

///Allocator related details. MarpII allows for custom allocators (usually the `A` parameter on the [Context](context::Ctx)).
pub mod allocator;

///Structures you need to get starting. Basically [Instance](context::Instance) and [Device](context::Device) creation.
/// Also includes the [Ctx](context::Ctx) struct, which also keeps track of a memory allocator and "in use" resources.
pub mod context;

///Allocatable resources. Mostly [Image](resources::Image) and [Buffer](resources::Buffer).
pub mod resources;

///Window surface related structures. Includes a self managed [Surface](surface::Surface) type.
pub mod surface;

/// [Swapchain](swapchain::Swapchain) type that can be created from a [Surface](surface::Surface). Includes some helper function.
///To search for suitable formats, image layout transition of swapchain images etc.
pub mod swapchain;

///Vulkan synchronisation primitives
pub mod sync;

mod error;
pub use error::{
    CommandBufferError, DescriptorError, DeviceError, InstanceError, MarpiiError, PipelineError,
    ShaderError,
};

///The infamous utility module contains all sorts of nice-to-have functions. Stuff like type converters etc.
pub mod util;
