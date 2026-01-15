use marpii::{
    ash::vk::{
        self, PhysicalDeviceAccelerationStructurePropertiesKHR, PhysicalDeviceLimits,
        PhysicalDeviceRayTracingPipelinePropertiesKHR, PhysicalDeviceShaderAtomicFloat2FeaturesEXT,
        PhysicalDeviceShaderAtomicFloatFeaturesEXT, PhysicalDeviceShaderAtomicInt64Features,
        PhysicalDeviceShaderImageAtomicInt64FeaturesEXT, PhysicalDeviceVulkan11Properties,
        PhysicalDeviceVulkan12Properties, PhysicalDeviceVulkan13Properties,
    },
    context::Instance,
};

///Collects the atomic-operation capabilities of the device.
#[derive(Debug, Default)]
pub struct AtomicsSupport {
    pub atomic_float: PhysicalDeviceShaderAtomicFloatFeaturesEXT<'static>,
    pub atomic_float2: PhysicalDeviceShaderAtomicFloat2FeaturesEXT<'static>,
    pub atomic_int: PhysicalDeviceShaderAtomicInt64Features<'static>,
    pub atomic_image: PhysicalDeviceShaderImageAtomicInt64FeaturesEXT<'static>,
}

impl AtomicsSupport {
    ///Returns true if any ShaderAtomicFloatFeaturesEXT is supported, i.e. the extension can/should be enabled
    pub fn any_atomic_float(&self) -> bool {
        self.atomic_float.shader_buffer_float32_atomic_add != 0
            || self.atomic_float.shader_buffer_float32_atomics != 0
            || self.atomic_float.shader_buffer_float64_atomic_add != 0
            || self.atomic_float.shader_buffer_float64_atomics != 0
            || self.atomic_float.shader_image_float32_atomic_add != 0
            || self.atomic_float.shader_image_float32_atomics != 0
            || self.atomic_float.shader_shared_float32_atomic_add != 0
            || self.atomic_float.shader_shared_float32_atomics != 0
            || self.atomic_float.shader_shared_float64_atomic_add != 0
            || self.atomic_float.shader_shared_float64_atomics != 0
            || self.atomic_float.sparse_image_float32_atomic_add != 0
            || self.atomic_float.sparse_image_float32_atomics != 0
    }

    pub fn any_atomic_float2(&self) -> bool {
        self.atomic_float2.shader_buffer_float16_atomic_add != 0
            || self.atomic_float2.shader_buffer_float16_atomic_min_max != 0
            || self.atomic_float2.shader_buffer_float16_atomics != 0
            || self.atomic_float2.shader_buffer_float32_atomic_min_max != 0
            || self.atomic_float2.shader_buffer_float64_atomic_min_max != 0
            || self.atomic_float2.shader_image_float32_atomic_min_max != 0
            || self.atomic_float2.shader_shared_float16_atomic_add != 0
            || self.atomic_float2.shader_shared_float16_atomic_min_max != 0
            || self.atomic_float2.shader_shared_float32_atomic_min_max != 0
            || self.atomic_float2.shader_shared_float64_atomic_min_max != 0
            || self.atomic_float2.sparse_image_float32_atomic_min_max != 0
    }

    pub fn any_atomic_int(&self) -> bool {
        self.atomic_int.shader_buffer_int64_atomics != 0
            || self.atomic_int.shader_shared_int64_atomics != 0
    }

    pub fn any_atomic_image(&self) -> bool {
        self.atomic_image.shader_image_int64_atomics != 0
            || self.atomic_image.sparse_image_int64_atomics != 0
    }
}

///Collects a prefetched set of _often-used_ device properties.
///
/// # Safety
///
/// Don't deref the `p_next` pointers in any of the fields, since those are guaranteed to be invalid.
///(I.e. only use the properties defined in that struct, don't walk the chain.)
#[derive(Debug, Default)]
pub struct PhysicalDeviceLimitsExtended {
    ///Limits as defined by vulkan
    pub limits: PhysicalDeviceLimits,

    pub acceleration_structure: PhysicalDeviceAccelerationStructurePropertiesKHR<'static>,

    pub raytracing_pipeline: PhysicalDeviceRayTracingPipelinePropertiesKHR<'static>,

    pub atomics_support: AtomicsSupport,

    pub vk11: PhysicalDeviceVulkan11Properties<'static>,
    pub vk12: PhysicalDeviceVulkan12Properties<'static>,
    pub vk13: PhysicalDeviceVulkan13Properties<'static>,
}

#[derive(Default)]
pub struct Config {
    ///Whether ray-tracing support
    pub rt_support: bool,

    ///Whether the `unified_image_layouts` extension is present.
    pub unified_image_layout_support: bool,

    ///limits defined by variouse used extension
    pub limit: PhysicalDeviceLimitsExtended,
}

impl Config {
    ///Initializes the [Config] for a given physical device
    pub fn new_for_device(instance: &Instance, physical_device: &vk::PhysicalDevice) -> Self {
        let mut conf = Config::default();
        conf.load_limits(instance, physical_device);
        conf.check_enable_rt_support(instance, physical_device);
        conf.check_enable_unified_image_layout(instance, physical_device);
        conf.check_atomics(instance, physical_device);
        conf
    }

    pub(crate) fn load_limits(
        &mut self,
        instance: &Instance,
        physical_device: &vk::PhysicalDevice,
    ) {
        self.limit.limits = unsafe {
            instance
                .inner
                .get_physical_device_properties(*physical_device)
                .limits
        };

        if self.rt_support {
            self.limit.acceleration_structure = instance
                .get_property::<PhysicalDeviceAccelerationStructurePropertiesKHR<
                '_,
            >>(physical_device);

            self.limit.raytracing_pipeline = instance
                .get_property::<PhysicalDeviceRayTracingPipelinePropertiesKHR<'_>>(physical_device);
        }

        self.limit.vk11 =
            instance.get_property::<PhysicalDeviceVulkan11Properties<'_>>(physical_device);

        self.limit.vk12 =
            instance.get_property::<PhysicalDeviceVulkan12Properties<'_>>(physical_device);

        self.limit.vk13 =
            instance.get_property::<PhysicalDeviceVulkan13Properties<'_>>(physical_device);
    }

    ///Checks that all of:
    ///
    /// - `khr::acceleration_structure`
    /// - `khr::ray_tracing_pipeline`
    /// - `khr::ray_query`
    /// - `khr::pipeline_library`
    /// - `khr::deferred_host_operations`
    ///
    /// are supported. If so, enables the features
    pub(crate) fn check_enable_rt_support(
        &mut self,
        instance: &Instance,
        physical_device: &vk::PhysicalDevice,
    ) {
        let f_acceleration = instance
            .get_feature::<marpii::ash::vk::PhysicalDeviceAccelerationStructureFeaturesKHR<'_>>(
            physical_device,
        );

        //NOTE: we need bot in our framework
        if f_acceleration.acceleration_structure != 1
            || f_acceleration.descriptor_binding_acceleration_structure_update_after_bind != 1
        {
            self.rt_support = false;
            return;
        }

        let f_ray_pipes = instance
            .get_feature::<marpii::ash::vk::PhysicalDeviceRayTracingPipelineFeaturesKHR<'_>>(
                physical_device,
            );

        if f_ray_pipes.ray_tracing_pipeline != 1 {
            self.rt_support = false;
            return;
        }
        let f_ray_query = instance
            .get_feature::<marpii::ash::vk::PhysicalDeviceRayQueryFeaturesKHR<'_>>(physical_device);

        if f_ray_query.ray_query != 1 {
            self.rt_support = false;
            return;
        }

        let f_pipelib = instance
            .get_feature::<marpii::ash::vk::PhysicalDeviceGraphicsPipelineLibraryFeaturesEXT<'_>>(
                physical_device,
            );

        if f_pipelib.graphics_pipeline_library != 1 {
            self.rt_support = false;
        }
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn check_enable_unified_image_layout(
        &mut self,
        _instance: &Instance,
        _physical_device: &vk::PhysicalDevice,
    ) {
        log::warn!("Checking for unified-image-layout-khr not implemented");
    }

    pub(crate) fn check_atomics(
        &mut self,
        instance: &Instance,
        physical_device: &vk::PhysicalDevice,
    ) {
        let atomic_float =
            instance.get_feature::<PhysicalDeviceShaderAtomicFloatFeaturesEXT<'_>>(physical_device);
        let atomic_float2 = instance
            .get_feature::<PhysicalDeviceShaderAtomicFloat2FeaturesEXT<'_>>(physical_device);
        let atomic_int =
            instance.get_feature::<PhysicalDeviceShaderAtomicInt64Features<'_>>(physical_device);
        let atomic_image = instance
            .get_feature::<PhysicalDeviceShaderImageAtomicInt64FeaturesEXT<'_>>(physical_device);

        self.limit.atomics_support = AtomicsSupport {
            atomic_float,
            atomic_float2,
            atomic_int,
            atomic_image,
        }
    }
}
