//! # Command graph
//!
//! High-level abstraction to easily build frame/rendering/execution graphs.
//!
//! On a really highlevel view Vulkan is used to bring data to the GPU, execute some program (shader) and possibly read this data back, for instance as a presented
//! image, or as a "result" buffer.
//!
//! This crate provides a convenient, but really opinionated abstraction that easily maps to vulkan itself.
//!
//! # Graph
//! The highest level is the [Graph]. It schedules execution of different [CommandBuffers](marpii_commands::ManagedCommands).
//! It handles queue transitions of resources as well as synchronisation between command buffers.
//!
//! # Pass
//! A pass is a node within a [Graph]. It is characterized by a command buffer that might be recorded and executed on a queue.
//!

#![deny(warnings)]
mod graph;
pub use graph::{ExecutionFence, Graph};

mod graph_builder;
pub use graph_builder::{GraphBuilder, Resource};
mod graph_optimizer;
pub use graph_optimizer::{OptGraph, Submit};

///Subpass definition, as well as a collection of already implemented subpasses
pub mod pass;
mod state;
pub use state::{BufferState, ImageState, StBuffer, StImage};

///Defines a "undefined" queue family. Used whenever no decission can be made about a queue.
//NOTE: While in parctise this could overlab with Queue::family_index, this is most likely never the case.
//      The indices are given "per-device" and are usually sub 10. So this should work even in a HPC context :D.
pub const UNDEFINED_QUEUE: u32 = u32::MAX;
