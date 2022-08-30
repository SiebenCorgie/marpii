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

use crate::{allocator::Allocator, surface::Surface};

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
    pub fn new_headless(use_validation: bool) -> Result<Self, anyhow::Error> {
        let mut instance_builder = Instance::linked()?;
        if use_validation {
            instance_builder = instance_builder.enable_validation();
        }
        let instance = instance_builder.build()?;

        let mut device_candidates = instance
            .create_physical_device_filter()?
            .filter_queue_flags(ash::vk::QueueFlags::GRAPHICS)
            .release();

        if device_candidates.len() == 0 {
            anyhow::bail!("Could not find suitable physical device!");
        }

        //NOTE: By default we setup extensions in a way that we can load rust shaders.
        let vulkan_memory_model = ash::vk::PhysicalDeviceVulkan12Features::builder()
            .shader_int8(true)
            .vulkan_memory_model(true);

        let device = device_candidates
            .remove(0)
            .into_device_builder(instance.clone())?
            .push_extensions(ash::vk::KhrVulkanMemoryModelFn::name())
            .with(|b| b.features.shader_int16 = 1)
            .with_additional_feature(vulkan_memory_model)
            .build()?;

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
            })?;

        allocator.report_memory_leaks(log::Level::Info);

        Ok(Ctx {
            allocator: Arc::new(Mutex::new(allocator)),
            device,
            instance,
        })
    }

    ///Creates simple context that has only one graphics queue. If provided creates the instance in a way that
    ///a surface for the provided window handle could be created.
    pub fn default_with_surface(
        window_handle: &dyn raw_window_handle::HasRawWindowHandle,
        use_validation: bool,
    ) -> Result<(Self, Arc<crate::surface::Surface>), anyhow::Error> {
        let mut instance_builder = Instance::linked()?;
        instance_builder = instance_builder.for_surface(window_handle)?;

        //when creating the default context we do not enable anything else, therfore
        //instance creation should be fine and we can "create"
        if use_validation {
            instance_builder = instance_builder.enable_validation();
        }
        let instance = instance_builder.build()?;

        //create the surface, so we can check for compatible devices in the filter.
        let surface = Arc::new(crate::surface::Surface::new(&instance, window_handle)?);

        let mut device_candidates = instance
            .create_physical_device_filter()?
            .filter_queue_flags(ash::vk::QueueFlags::GRAPHICS)
            .filter_presentable(&surface.surface_loader, &surface.surface)
            .release();

        if device_candidates.len() == 0 {
            anyhow::bail!("Could not find suitable physical device!");
        }

        //NOTE: By default we setup extensions in a way that we can load rust shaders.
        let vulkan_memory_model = ash::vk::PhysicalDeviceVulkan12Features::builder()
            .shader_int8(true)
            .runtime_descriptor_array(true)
            .vulkan_memory_model(true);
        //NOTE: used for dynamic rendering based pipelines which are preffered over renderpass based graphics queues.
        let dynamic_rendering =
            ash::vk::PhysicalDeviceDynamicRenderingFeatures::builder().dynamic_rendering(true);

        let device = device_candidates
            .remove(0)
            .into_device_builder(instance.clone())?
            .push_extensions(ash::extensions::khr::Swapchain::name())
            .push_extensions(ash::vk::KhrVulkanMemoryModelFn::name())
            .push_extensions(ash::extensions::khr::DynamicRendering::name())
            .with(|b| b.features.shader_int16 = 1)
            .with_additional_feature(vulkan_memory_model)
            .with_additional_feature(dynamic_rendering)
            .build()?;

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
            })?;

        Ok((
            Ctx {
                allocator: Arc::new(Mutex::new(allocator)),
                device,
                instance,
            },
            surface,
        ))
    }

    ///Creates the *best* context possible.
    ///
    /// Each queue family that exists is crated with the at max 16 queues (if possible).
    ///
    /// To control the device creation process, use the `on_device_builder` closure. Usefull as specially to register extentions etc.
    pub fn custom_context(
        window_handle: Option<&dyn raw_window_handle::HasRawWindowHandle>,
        use_validation: bool,
        on_device_builder: impl FnOnce(DeviceBuilder) -> DeviceBuilder,
    ) -> Result<(Self, Option<Surface>), anyhow::Error> {
        let mut instance_builder = Instance::linked()?;
        if let Some(window_handle) = window_handle {
            instance_builder = instance_builder.for_surface(window_handle)?;
        }

        //when creating the default context we do not enable anything else, therfore
        //instance creation should be fine and we can "create"
        if use_validation {
            instance_builder = instance_builder.enable_validation();
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
            anyhow::bail!("Could not find suitable physical device!");
        }

        //Find the *best* we try to get a dicrete one, of not then we use the integrated
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
            })?;

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
