//! ## Context
//!
//! When working with Vulkan the [Device](ash::Device) is entry point for most of the operations.
//! It therefore is needed in most structures and functions calls that somehow transform state related to Vulkan.
//! The device is created from an [Instance](ash::Instance) which represents a runtime instance of Vulkan.
//!
//! Additionally to the device one or multiple [queues](ash::vk::Queue) might be created. They can be understood as
//! a kind of "thread". Basically they are used for scheduling work on the GPU. Multiple queue types exists that can
//! do different types of work.
//!
//!
//! When working with buffers (and images) another structure, the allocator is relevant.
//! It takes care of tracking where and which memory is in-use on the GPU etc.
//!
//! Since those four structures closely work together we define an abstraction called [Ctx](context::Ctx), or "Context".
//!
//! The `Instance` and `Device` are always created by ash, the allocator however can be defined by the
//! application. Have a look at the [allocator](crate::allocator) module for its definition and default implementation.
//!
//! # Examples
//! 🚧 Todo: show several examples on how to create an instance, device, queue or context, from least verbose to most verbose. 🚧
//!
use std::{
    cmp::Ordering,
    sync::{Arc, Mutex},
};

mod instance;
use ash::vk;
pub use instance::{GetDeviceFilter, Instance, InstanceBuilder};

mod device;
pub use device::{Device, DeviceBuilder};

mod queue;
pub use queue::{Queue, QueueBuilder};

mod physical_device;
pub use physical_device::{PhyDeviceProperties, PhysicalDeviceFilter};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::{allocator::Allocator, error::DeviceError, surface::Surface, MarpiiError};

use self::instance::ValidationFeatures;

///Context related errors. Can occur either while creating the context, or when using one of the high level
/// functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextError {}

///MarpII's Vulkan context. Can either be constructed by hand, or via helper functions.
#[derive(Clone)]
pub struct Ctx<A: Allocator + Send> {
    ///Allocator instance used for all buffer and image allocation in this context.
    pub allocator: Arc<Mutex<A>>,
    ///Vulkan device including assosiated queues.
    //TODO expose queues directly?
    pub device: Arc<Device>,
    ///The initial vulkan instance used for the context.
    pub instance: Arc<crate::context::Instance>,
    //TODO include swapchain? Or make another highlevel context for that?
}

impl<A: Allocator + Send> Ctx<A> {
    ///Creates the context from its elements.
    ///
    /// # Safety
    /// Assumes that the allocator was created for the device, which is in turn created for the instance.
    pub fn new(allocator: A, device: Arc<Device>, instance: Arc<crate::context::Instance>) -> Self {
        Ctx {
            allocator: Arc::new(Mutex::new(allocator)),
            device,
            instance,
        }
    }
}

#[cfg(feature = "default_allocator")]
impl Ctx<gpu_allocator::vulkan::Allocator> {
    ///Creates a new context that does not check for any surface availability.
    pub fn new_default_headless(use_validation: bool) -> Result<Self, MarpiiError> {
        let mut instance_builder = Instance::linked()?;
        if use_validation {
            instance_builder = instance_builder.enable_validation(ValidationFeatures::all());
        }
        let instance = instance_builder.build()?;
        Self::new_default_from_instance(instance, None)
    }

    ///Creates simple context that has only one graphics queue. If provided creates the instance in a way that
    ///a surface for the provided window handle could be created.
    pub fn default_with_surface<T>(
        window_handle: &T,
        use_validation: bool,
    ) -> Result<(Self, Arc<crate::surface::Surface>), MarpiiError>
    where
        T: HasRawDisplayHandle + HasRawWindowHandle,
    {
        let mut instance_builder = Instance::linked()?;
        instance_builder = instance_builder.for_surface(window_handle)?;

        //when creating the default context we do not enable anything else, therfore
        //instance creation should be fine and we can "create"
        if use_validation {
            instance_builder = instance_builder.enable_validation(ValidationFeatures::all());
            //instance_builder = instance_builder.with_layer(CString::new("VK_VALIDATION_FEATURE_ENABLE_GPU_ASSISTED_EXT").unwrap())?;
        }
        let instance = instance_builder.build()?;

        //create the surface, so we can check for compatible devices in the filter.
        let surface = Arc::new(crate::surface::Surface::new(&instance, window_handle)?);

        let ctx = Self::new_default_from_instance(instance, Some(&surface))?;

        Ok((ctx, surface))
    }

    ///Creates a default context from a given instance. This is also the base creation code for
    /// [Self::default_with_surface] and [Self::new_default_headless].
    ///
    /// It enables multiple default features and extension that make this context work with
    /// marpii-rmg.
    pub fn new_default_from_instance(
        instance: Arc<Instance>,
        surfaces: Option<&Surface>,
    ) -> Result<Self, MarpiiError> {
        let mut device_candidates = instance
            .create_physical_device_filter()?
            .filter_queue_flags(ash::vk::QueueFlags::GRAPHICS);
        //If we have a surface, filter for that
        if let Some(surface) = surfaces {
            device_candidates =
                device_candidates.filter_presentable(&surface.surface_loader, &surface.surface);
        }

        let mut device_candidates = device_candidates.release();

        if device_candidates.len() == 0 {
            return Err(DeviceError::NoPhysicalDevice)?;
        }

        //NOTE: By default we setup extensions in a way that we can load rust shaders.
        let features12 = ash::vk::PhysicalDeviceVulkan12Features::builder()
            .shader_int8(true)
            .runtime_descriptor_array(true)
            .timeline_semaphore(true)
            .descriptor_indexing(true)
            .descriptor_binding_sampled_image_update_after_bind(true)
            .descriptor_binding_storage_image_update_after_bind(true)
            .descriptor_binding_storage_buffer_update_after_bind(true)
            .descriptor_binding_partially_bound(true)
            .descriptor_binding_variable_descriptor_count(true)
            .shader_storage_buffer_array_non_uniform_indexing(true)
            .shader_storage_image_array_non_uniform_indexing(true)
            .shader_sampled_image_array_non_uniform_indexing(true)
            .vulkan_memory_model(true);

        let features13 = ash::vk::PhysicalDeviceVulkan13Features::builder()
            .maintenance4(true)
            .dynamic_rendering(true)
            .synchronization2(true);

        //Acceleration structure support
        /*
        let accel_structure = ash::vk::PhysicalDeviceAccelerationStructureFeaturesKHR::builder()
        .acceleration_structure(true)
        .descriptor_binding_acceleration_structure_update_after_bind(true);
         */
        let mut device_builder = device_candidates
            .remove(0)
            .into_device_builder(instance.clone())?
            .with_extensions(ash::vk::KhrVulkanMemoryModelFn::name())
            .with_extensions(ash::extensions::khr::DynamicRendering::name())
            .with(|b| {
                b.features.shader_int16 = 1;
                b.features.shader_storage_buffer_array_dynamic_indexing = 1;
                b.features.shader_storage_image_array_dynamic_indexing = 1;
                b.features.shader_uniform_buffer_array_dynamic_indexing = 1;
                b.features.shader_sampled_image_array_dynamic_indexing = 1;
                b.features.robust_buffer_access = 1;
            })
            .with_feature(features12)
            .with_feature(features13);
        //.with_additional_feature(accel_structure)

        // only add swapchain extension if we got a surface
        if surfaces.is_some() {
            device_builder =
                device_builder.with_extensions(ash::extensions::khr::Swapchain::name());
        }
        let device = device_builder.build()?;

        //create allocator for device
        let allocator =
            gpu_allocator::vulkan::Allocator::new(&gpu_allocator::vulkan::AllocatorCreateDesc {
                buffer_device_address: false,
                debug_settings: gpu_allocator::AllocatorDebugSettings {
                    log_leaks_on_shutdown: true,
                    ..Default::default()
                },
                device: device.inner.clone(),
                instance: instance.inner.clone(),
                physical_device: device.physical_device,
            })
            .map_err(|e| DeviceError::GpuAllocatorError(Box::new(e)))?;

        Ok(Ctx {
            allocator: Arc::new(Mutex::new(allocator)),
            device,
            instance,
        })
    }

    ///Creates the *best* context possible.
    ///
    /// Each queue family that exists is crated with the at max 16 queues (if possible).
    ///
    /// To control the device creation process, use the `on_device_builder` closure. Usefull as specially to register extentions etc.
    pub fn custom_context<T>(
        window_handle: Option<&T>,
        use_validation: bool,
        on_device_builder: impl FnOnce(DeviceBuilder) -> DeviceBuilder,
    ) -> Result<(Self, Option<Surface>), MarpiiError>
    where
        T: HasRawDisplayHandle + HasRawWindowHandle,
    {
        let mut instance_builder = Instance::linked()?;
        if let Some(window_handle) = window_handle {
            instance_builder = instance_builder.for_surface(window_handle)?;
        }

        //when creating the default context we do not enable anything else, therfore
        //instance creation should be fine and we can "create"
        if use_validation {
            instance_builder = instance_builder.enable_validation(ValidationFeatures::all());
        }
        let instance = instance_builder.build()?;

        //create the surface, so we can check for compatible devices in the filter.
        let surface = if let Some(handle) = window_handle {
            Some(Surface::new(&instance, handle)?)
        } else {
            None
        };

        let mut physical_device_filter = instance.create_physical_device_filter()?;

        //If creating for surface we need a queue that can do graphics stuff
        if let Some(surface) = &surface {
            physical_device_filter = physical_device_filter
                .filter_queue_flags(ash::vk::QueueFlags::GRAPHICS)
                .filter_presentable(&surface.surface_loader, &surface.surface)
        }

        let mut device_candidates = physical_device_filter.release();

        if device_candidates.len() == 0 {
            return Err(DeviceError::NoPhysicalDevice)?;
        }

        //Find the *best* we try to get a discrete one, of not then we use the integrated
        device_candidates.sort_by(|a, b| {
            match (a.properties.device_type, b.properties.device_type) {
                (vk::PhysicalDeviceType::DISCRETE_GPU, vk::PhysicalDeviceType::DISCRETE_GPU) => {
                    Ordering::Equal
                }
                (vk::PhysicalDeviceType::DISCRETE_GPU, _) => Ordering::Greater,
                (vk::PhysicalDeviceType::INTEGRATED_GPU, vk::PhysicalDeviceType::DISCRETE_GPU) => {
                    Ordering::Less
                }
                _ => Ordering::Less,
            }
        });

        #[cfg(feature = "logging")]
        {
            log::info!("Device candidates (in order):");
            for dev in device_candidates.iter() {
                log::info!("    Device: {:#?}", dev.properties);
            }
        }

        let mut device_builder = device_candidates
            .remove(0)
            .into_device_builder(instance.clone())?;

        device_builder = on_device_builder(device_builder);

        let device = device_builder.build()?;

        //create allocator for device
        let allocator =
            gpu_allocator::vulkan::Allocator::new(&gpu_allocator::vulkan::AllocatorCreateDesc {
                buffer_device_address: false,
                debug_settings: gpu_allocator::AllocatorDebugSettings {
                    log_leaks_on_shutdown: true,
                    ..Default::default()
                },
                device: device.inner.clone(),
                instance: instance.inner.clone(),
                physical_device: device.physical_device,
            })
            .map_err(|e| DeviceError::GpuAllocatorError(Box::new(e)))?;

        Ok((
            Ctx {
                allocator: Arc::new(Mutex::new(allocator)),
                device,
                instance,
            },
            surface,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_impl_all;

    #[test]
    fn impl_send_sync() {
        assert_impl_all!(Ctx<gpu_allocator::vulkan::Allocator>: Send, Sync);
        assert_impl_all!(Device: Send, Sync);
    }
}
