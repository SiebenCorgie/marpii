#![feature(drain_filter)]
// BEGIN - Embark standard lints v6 for Rust 1.55+
// do not change or add/remove here, but one can add exceptions after this section
// for more info see: <https://github.com/EmbarkStudios/rust-ecosystem/issues/59>
//#![deny(unsafe_code)] //not practical when working with ash
#![warn(
    clippy::all,
    clippy::await_holding_lock,
    clippy::char_lit_as_u8,
    clippy::checked_conversions,
    clippy::dbg_macro,
    clippy::debug_assert_with_mut_call,
    clippy::doc_markdown,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::exit,
    clippy::expl_impl_clone_on_copy,
    clippy::explicit_deref_methods,
    clippy::explicit_into_iter_loop,
    clippy::fallible_impl_from,
    clippy::filter_map_next,
    clippy::flat_map_option,
    clippy::float_cmp_const,
    clippy::fn_params_excessive_bools,
    clippy::from_iter_instead_of_collect,
    clippy::if_let_mutex,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::inefficient_to_string,
    clippy::invalid_upcast_comparisons,
    clippy::large_digit_groups,
    clippy::large_stack_arrays,
    clippy::large_types_passed_by_value,
    clippy::let_unit_value,
    clippy::linkedlist,
    clippy::lossy_float_literal,
    clippy::macro_use_imports,
    clippy::manual_ok_or,
    clippy::map_err_ignore,
    clippy::map_flatten,
    clippy::map_unwrap_or,
    clippy::match_on_vec_items,
    clippy::match_same_arms,
    clippy::match_wild_err_arm,
    clippy::match_wildcard_for_single_variants,
    clippy::mem_forget,
    clippy::mismatched_target_os,
    clippy::missing_enforced_import_renames,
    clippy::mut_mut,
    clippy::mutex_integer,
    clippy::needless_borrow,
    clippy::needless_continue,
    clippy::needless_for_each,
    clippy::option_option,
    clippy::path_buf_push_overwrite,
    clippy::ptr_as_ptr,
    clippy::rc_mutex,
    clippy::ref_option_ref,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::same_functions_in_if_condition,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else,
    clippy::string_add_assign,
    clippy::string_add,
    clippy::string_lit_as_bytes,
    clippy::string_to_string,
    clippy::todo,
    clippy::trait_duplication_in_bounds,
    clippy::unimplemented,
    clippy::unnested_or_patterns,
    clippy::unused_self,
    clippy::useless_transmute,
    clippy::verbose_file_reads,
    clippy::zero_sized_map_values,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms
)]
// END - Embark standard lints v6 for Rust 1.55+
// crate-specific exceptions:

//! # ResourceManagingGraph (RMG)
//!
//! The RMG is a big abstraction layer over raw vulkan. It is therefore much more opinionated then the rest of MarpII.
//!
//! It handles the context creation as well as resource creation and binding. The user (you) primarily interacts in the form of [Task](recorder::Task)s. They can be scheduled
//! in an execution Graph using a [Recorder](recorder::Recorder). The tasks implementation is up to you and has full access to all resources and the Vulkan context.
//!
//! TODO: more docs on how to get started etc.

mod resources;
use fxhash::FxHashMap;
use recorder::Recorder;
pub use resources::{
    res_states::{AnyResKey, BufferKey, ImageKey, ResBuffer, ResImage, ResSampler, SamplerKey},
    ResourceError, Resources,
};

mod recorder;
pub use recorder::{
    task::{ResourceRegistry, Task},
    RecordError,
};

pub(crate) mod track;

///Pre implemented generic tasks
pub mod tasks;

use marpii::{
    allocator::MemoryUsage,
    ash::vk,
    context::Ctx,
    gpu_allocator::vulkan::Allocator,
    resources::{BufDesc, Buffer, Image, ImgDesc, Sampler, SharingMode},
    surface::Surface,
};
use std::sync::Arc;
use thiserror::Error;
use track::{Track, TrackId, Tracks};

///Top level Error structure.
#[derive(Debug, Error)]
pub enum RmgError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("anyhow")]
    Any(#[from] anyhow::Error),

    #[error("Recording error")]
    RecordingError(#[from] RecordError),

    #[error("Resource error")]
    ResourceError(#[from] ResourceError),
}

pub type CtxRmg = Ctx<Allocator>;

///Main RMG interface.
pub struct Rmg {
    ///Resource management
    pub(crate) res: resources::Resources,

    ///maps a capability pattern to a index in `Device`'s queue list. Each queue type defines a QueueTrack type.
    tracks: Tracks,

    pub ctx: CtxRmg,
}

impl Rmg {
    pub fn new(context: Ctx<Allocator>, surface: &Arc<Surface>) -> Result<Self, RmgError> {
        //Per definition we try to find at least one graphic, compute and transfer queue.
        // We then create the swapchain. It is used for image presentation and the start/end point for frame scheduling.

        //TODO: make the iterator return an error. Currently if track creation fails, everything fails
        let tracks = context.device.queues.iter().enumerate().fold(
            FxHashMap::default(),
            |mut set: FxHashMap<TrackId, Track>, (idx, q)| {
                #[cfg(feature = "logging")]
                log::info!("QueueType: {:#?}", q.properties.queue_flags);
                //Make sure to only add queue, if we don't have a queue with those capabilities yet.
                if !set.contains_key(&TrackId(q.properties.queue_flags)) {
                    set.insert(
                        TrackId(q.properties.queue_flags),
                        Track::new(&context.device, idx as u32, q.properties.queue_flags),
                    );
                }

                set
            },
        );

        let res = Resources::new(&context.device, surface)?;

        Ok(Rmg {
            res,
            tracks: Tracks(tracks),
            ctx: context,
        })
    }

    pub fn new_image_uninitialized(
        &mut self,
        description: ImgDesc,
        name: Option<&str>,
    ) -> Result<ImageKey, RmgError> {
        //patch usage bits

        if !description.usage.contains(vk::ImageUsageFlags::SAMPLED)
            && !description.usage.contains(vk::ImageUsageFlags::STORAGE)
        {
            return Err(RmgError::from(ResourceError::ImageNoUsageFlags));
        }

        let image = Arc::new(Image::new(
            &self.ctx.device,
            &self.ctx.allocator,
            description,
            MemoryUsage::GpuOnly, //always cpu only, everything else is handled by passes directly
            name,
            None,
        )?);

        Ok(self.res.add_image(image)?)
    }

    pub fn new_buffer_uninitialized(
        &mut self,
        description: BufDesc,
        name: Option<&str>,
    ) -> Result<BufferKey, RmgError> {
        let buffer = Arc::new(Buffer::new(
            &self.ctx.device,
            &self.ctx.allocator,
            description,
            MemoryUsage::GpuOnly,
            name,
            None,
        )?);

        Ok(self.res.add_buffer(buffer)?)
    }

    ///Creates a new (storage)buffer that can hold at max `size` times `T`.
    pub fn new_buffer<T: 'static>(
        &mut self,
        size: usize,
        name: Option<&str>,
    ) -> Result<BufferKey, RmgError> {
        let size = core::mem::size_of::<T>() * size;
        let description = BufDesc {
            size: size.try_into().unwrap(),
            usage: vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::TRANSFER_DST,
            sharing: SharingMode::Exclusive,
        };
        self.new_buffer_uninitialized(description, name)
    }

    pub fn new_sampler(
        &mut self,
        description: &vk::SamplerCreateInfoBuilder<'_>,
    ) -> Result<SamplerKey, RmgError> {
        let sampler = Sampler::new(&self.ctx.device, description)?;

        Ok(self.res.add_sampler(Arc::new(sampler))?)
    }

    pub fn record<'rmg>(&'rmg mut self, window_extent: vk::Extent2D) -> Recorder<'rmg> {
        //tick all tracks to free resources
        for (_k, t) in self.tracks.0.iter_mut() {
            t.tick_frame();
        }
        //tick resource manager as well
        self.res.tick_record(&self.tracks);

        Recorder::new(self, window_extent)
    }

    pub fn delete(&mut self, res: impl Into<AnyResKey>) -> Result<(), ResourceError> {
        self.res.remove_resource(res)
    }

    pub fn resources(&self) -> &Resources {
        &self.res
    }

    pub(crate) fn queue_idx_to_trackid(&self, idx: u32) -> Option<TrackId> {
        for t in self.tracks.0.iter() {
            if t.1.queue_idx == idx {
                return Some(*t.0);
            }
        }
        None
    }

    pub(crate) fn trackid_to_queue_idx(&self, id: TrackId) -> u32 {
        self.tracks.0.get(&id).unwrap().queue_idx
    }
}

impl Drop for Rmg {
    fn drop(&mut self) {
        //make sure all executions have finished, otherwise we could destroy
        // referenced images etc.
        for (_id, t) in self.tracks.0.iter_mut() {
            t.wait_for_inflights()
        }
    }
}
