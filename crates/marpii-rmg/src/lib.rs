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
//! It handles the context creation as well as resource creation and binding. The user (you) primarily interacts in the form of [Task](Task)s. They can be scheduled
//! in an execution Graph using a [Recorder](Recorder). The tasks implementation is up to you and has full access to all resources and the Vulkan context.
//!
//! Apart from the task all execution related tracking of resources and executions is done by RMG.
//!
//! The architecture looks like this:
//!
//! ```ignore
//!
//! |-----------------|
//! |  Application    | <- User defined
//! |-----------------|
//! |  RMG runtime    | <- RMG
//! |--|              |
//! |  |   Recording  | <- RMG
//! |  |---|          |
//! |  |   |  Task    | <- User defined
//! |  |   |  Task    |
//! |  |   |  ...     |
//! |  |   |          |
//! |  |   execution  | <- RMG
//! |  |              |
//! |  |-Vulkan       | <- RMG(MarpII)
//! |-----------------|
//! |   Hardware      |
//! |-----------------|
//! ```
//!
//!
//! ## Using resources
//! Since RMG handles all resources direct access is only possible from within a [Task](Task). To still reference resources (and defining data flow between tasks)
//! ResourceHandles are used. They behave as if they where the resources. This means if all handles to a resource are dropped, the resource itself is dropped.
//!
//! ## Performance, blocking and multithreading
//!
//! RMG occasionally spawns threads for tasks like garbage collection. Operations like the recording and execution can block for some time (as specially if a swapchain
//! present operation is involved).
//!
//! Therefore RMG should usually run parallel to other (gameplay/application) code.
//!
//! # Example
//!
//! ## Copying a texture to the GPU
//!
//! ```rust, ignore
//!
//! let ev = winit::event_loop::EventLoop::new();
//! let window = winit::window::Window::new(&ev).unwrap();
//!
//! let (context, surface) = Ctx::default_with_surface(&window, true)?;
//!
//! let mut rmg = Rmg::new(context, &surface)?;
//! //The texture data we want to upload, usually loaded from a file
//! let texture_data = [0u8; 1024];
//!
//! //creating an GPU local image
//! let img = rmg.new_image_uninitialized(
//!     ImgDesc::storage_image_2d(
//!         image_data.width(),
//!         image_data.height(),
//!         vk::Format::R32G32B32A32_SFLOAT,
//!     ),
//!     None,
//! )?;
//!
//! //Creating the upload task
//! let mut image_init = UploadImage::new(img, &texture_data);
//!
//! //And executing it
//! rmg.record(window_extent(&window))
//!     .add_task(&mut image_init)
//!     .unwrap()
//!     .execute()?;
//!
//! //If you use `img` anywhere after this you can be sure that the upload has finished
//! //before any access happens
//! ```
//!
//!
mod resources;
pub use resources::{
    handle::{BufferHandle, ImageHandle, SamplerHandle},
    res_states::{ResBuffer, ResImage, ResSampler},
    ResourceError, Resources,
};
mod recorder;
pub use recorder::{
    task::{ResourceRegistry, Task},
    RecordError, Recorder,
};

pub(crate) mod track;

mod rmg;
pub use rmg::{CtxRmg, Rmg, RmgError};
