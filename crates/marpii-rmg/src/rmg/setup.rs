use marpii::{
    ash::vk::{self},
    context::DeviceBuilder,
    surface::Surface,
    OoS,
};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::{Config, Rmg, RmgError};

impl Rmg {
    ///Handles the creation and initialization of a _as-powerful-as-possible_ RMG.
    pub fn init_for_window<W: HasWindowHandle + HasDisplayHandle>(
        window: &W,
    ) -> Result<(Self, OoS<Surface>), RmgError> {
        let (rmg, surface) = Self::init(Some(window), |db, _| db)?;
        Ok((
            rmg,
            surface.ok_or(RmgError::ResourceError(
                crate::ResourceError::SurfaceCreation,
            ))?,
        ))
    }

    ///The inner initialization routine which might setup the device for a window.
    ///
    /// use `on_builder` to setup additional extensions needed by your application. If you do so,
    /// make sure they are actually supported first, and probably panic, if some _needed_ is not supported.
    /// Otherwise you might want to conditionally enable and save that information in some context info.
    fn init<W: HasWindowHandle + HasDisplayHandle>(
        window: Option<&W>,
        mut on_builder: impl FnMut(DeviceBuilder, &Config) -> DeviceBuilder,
    ) -> Result<(Self, Option<OoS<Surface>>), RmgError> {
        let use_validation = std::env::var("RMG_VALIDATE").is_ok();

        if use_validation {
            log::info!("Using validation layers");
        }

        let (ctx, surface) =
            marpii::context::Ctx::custom_context(window, use_validation, |mut db| {
                let config = Config::new_for_device(&db.instance, &db.physical_device);
                db = db
                    .with(|inner| {
                        inner.features = inner
                            .features
                            //All the dynamic indexing features
                            .shader_sampled_image_array_dynamic_indexing(true)
                            .shader_storage_image_array_dynamic_indexing(true)
                            .shader_storage_buffer_array_dynamic_indexing(true)
                            .shader_uniform_buffer_array_dynamic_indexing(true)
                            //double and half
                            .shader_int16(true)
                            .shader_int64(true)
                            .shader_float64(true)
                            //Robust access
                            .robust_buffer_access(true);
                    })
                    .with_feature(
                        vk::PhysicalDeviceVulkan12Features::default()
                            //Backbone of working with buffers
                            .buffer_device_address(true)
                            //used to be rust-gpu compatible
                            .vulkan_memory_model(true)
                            //Again for rust-gpu
                            .shader_int8(true)
                            //The way we synchronise _everything_
                            .timeline_semaphore(true)
                            //Descriptor indexing
                            .runtime_descriptor_array(true)
                            .descriptor_indexing(true)
                            .shader_sampled_image_array_non_uniform_indexing(true)
                            .shader_storage_image_array_non_uniform_indexing(true)
                            .shader_storage_buffer_array_non_uniform_indexing(true)
                            .shader_uniform_buffer_array_non_uniform_indexing(true)
                            //Desriptor updating
                            .descriptor_binding_partially_bound(true)
                            .descriptor_binding_sampled_image_update_after_bind(true)
                            .descriptor_binding_storage_image_update_after_bind(true)
                            .descriptor_binding_storage_buffer_update_after_bind(true)
                            //Enabel int64 atomics if supported
                            .shader_buffer_int64_atomics(
                                config.limit.atomics_support.any_atomic_int(),
                            )
                            .descriptor_binding_variable_descriptor_count(true),
                    )
                    .with_feature(
                        vk::PhysicalDeviceVulkan13Features::default()
                            .maintenance4(true)
                            .dynamic_rendering(true)
                            .synchronization2(true),
                    )
                    //Activate maintainance 1 & 3
                    .with_extensions(marpii::ash::khr::maintenance1::NAME)
                    .with_extensions(marpii::ash::khr::maintenance3::NAME);

                //If rt is active, add everything related to it
                db = if config.rt_support {
                    log::info!("Using Raytracing extensions");
                    db.with_extensions(marpii::ash::khr::acceleration_structure::NAME)
                        .with_extensions(marpii::ash::khr::ray_tracing_pipeline::NAME)
                        .with_extensions(marpii::ash::khr::ray_query::NAME)
                        .with_extensions(marpii::ash::khr::pipeline_library::NAME)
                        .with_extensions(marpii::ash::khr::deferred_host_operations::NAME)
                } else {
                    db
                };

                //if unified-layout is active, enable it
                db = if config.unified_image_layout_support {
                    log::warn!("UnifiedImageLayoutKHR not yet in ash...");
                    //db = db.with_extensions(marpii::ash::khr::unified_image_layout::NAME);
                    db
                } else {
                    log::warn!(
                        "UnifiedImageLayout not supported, might result in degarded performance!"
                    );

                    db
                };

                //if atomics supported, add them as well
                db = if config.limit.atomics_support.any_atomic_float() {
                    log::info!("Enable AtomicFloat");
                    db.with_extensions(marpii::ash::ext::shader_atomic_float::NAME)
                        .with_feature(config.limit.atomics_support.atomic_float)
                } else {
                    db
                };
                db = if config.limit.atomics_support.any_atomic_float2() {
                    log::info!("Enable AtomicFloat2 support");
                    db.with_extensions(marpii::ash::ext::shader_atomic_float2::NAME)
                        .with_feature(config.limit.atomics_support.atomic_float2)
                } else {
                    db
                };
                db = if config.limit.atomics_support.any_atomic_image() {
                    log::info!("Enable AtomicImage support ");
                    db.with_extensions(marpii::ash::ext::shader_image_atomic_int64::NAME)
                        .with_feature(config.limit.atomics_support.atomic_image)
                } else {
                    db
                };

                db = on_builder(db, &config);

                db
            })?;

        Ok((Rmg::new(ctx)?, surface))
    }
}
