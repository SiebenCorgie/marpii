use std::{
    ffi::{CStr, CString},
    mem::MaybeUninit,
    ptr::addr_of_mut,
    sync::Arc,
};

use ash::vk::{self, BaseOutStructure, ObjectType, TaggedStructure};
use const_cstr::const_cstr;
use raw_window_handle::HasRawDisplayHandle;

use crate::error::InstanceError;

use super::PhysicalDeviceFilter;
const_cstr! {
    UNKNOWNID = "unknown id";
    NOMSG = "no message";
}

///The external callback print function for debugging
unsafe extern "system" fn vulkan_debug_callback(
    message_severity: ash::vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_types: ash::vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const ash::vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut core::ffi::c_void,
) -> u32 {
    if p_callback_data.is_null() {
        #[cfg(feature = "logging")]
        log::error!("MarpDebugMsg: Got Msg, but no data!");
        return 1;
    }

    //use log if the layer is enabled, otherwise use println
    #[cfg(feature = "logging")]
    {
        let (id, idname) = if !(*p_callback_data).p_message_id_name.is_null() {
            (
                (*p_callback_data).message_id_number,
                std::ffi::CStr::from_ptr((*p_callback_data).p_message_id_name),
            )
        } else {
            (
                (*p_callback_data).message_id_number,
                std::ffi::CStr::from_ptr(UNKNOWNID.as_ptr()),
            )
        };

        let msg = if !(*p_callback_data).p_message.is_null() {
            std::ffi::CStr::from_ptr((*p_callback_data).p_message)
        } else {
            std::ffi::CStr::from_ptr(NOMSG.as_ptr())
        };

        if message_severity.contains(ash::vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
            log::error!("[{}: {:?}]: {:?}", id, idname, msg);
        } else if message_severity.contains(ash::vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
            log::warn!("[{}: {:?}]: {:?}", id, idname, msg);
        } else if message_severity.contains(ash::vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
            log::info!("[{}: {:?}]: {:?}", id, idname, msg);
        } else {
            log::trace!("[{}: {:?}]: {:?}", id, idname, msg);
        }
    }

    #[cfg(not(feature = "logging"))]
    {
        println!(
            "MarpDebugMsg: Level: {:?}, Type: {:?}\n",
            message_severity, message_types
        );
        //If there is a validation layer msg, append
        if !(*p_callback_data).p_message_id_name.is_null() {
            println!(
                "Id[{:?}]: {:?}",
                (*p_callback_data).message_id_number,
                std::ffi::CStr::from_ptr((*p_callback_data).p_message_id_name)
            )
        } else {
            println!("Id[{:?}]", (*p_callback_data).message_id_number);
        }

        //Now display message if there is any
        if !(*p_callback_data).p_message.is_null() {
            println!(
                "Msg: {:?}",
                std::ffi::CStr::from_ptr((*p_callback_data).p_message)
            );
        }
    }
    1
}

///Helper that gets usually initialised by activating validation layers.
/// Allows to use all `VK_EXT_DEBUG_UTILS` functions.
pub struct Debugger {
    pub debug_report_loader: ash::extensions::ext::DebugUtils,
    pub debug_messenger: ash::vk::DebugUtilsMessengerEXT,
}

impl Debugger {
    pub fn name_object(
        &self,
        device: &vk::Device,
        handle: u64,
        ty: ObjectType,
        name: &CStr,
    ) -> Result<(), vk::Result> {
        let info = vk::DebugUtilsObjectNameInfoEXT::builder()
            .object_name(name)
            .object_handle(handle)
            .object_type(ty);
        unsafe {
            self.debug_report_loader
                .set_debug_utils_object_name(*device, &info)
        }
    }
}

///Signales enabled and disabled validation layer features
#[allow(dead_code)]
pub struct ValidationFeatures {
    enabled: Vec<vk::ValidationFeatureEnableEXT>,
    disabled: Vec<vk::ValidationFeatureDisableEXT>,
}

impl ValidationFeatures {
    pub fn none() -> Self {
        ValidationFeatures {
            enabled: Vec::new(),
            disabled: Vec::new(),
        }
    }

    ///enables only debug printf
    pub fn gpu_printf() -> Self {
        ValidationFeatures {
            enabled: vec![vk::ValidationFeatureEnableEXT::DEBUG_PRINTF],
            disabled: Vec::new(),
        }
    }

    ///Enables all debug features
    pub fn all() -> Self {
        ValidationFeatures {
            enabled: vec![
                vk::ValidationFeatureEnableEXT::GPU_ASSISTED,
                vk::ValidationFeatureEnableEXT::GPU_ASSISTED_RESERVE_BINDING_SLOT,
                vk::ValidationFeatureEnableEXT::BEST_PRACTICES,
                vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION,
            ],
            disabled: Vec::new(),
        }
    }
}

///Instance configuration as well as the source entry point. Usually this struct is created via [Instance::load] or [Instance::linked]
pub struct InstanceBuilder {
    pub entry: ash::Entry,
    pub validation_layers: Option<ValidationFeatures>,
    pub enabled_layers: Vec<CString>,
    pub enabled_extensions: Vec<CString>,
}

impl InstanceBuilder {
    ///Builds the instance from the current information.
    ///if `validation_layers` is enabled and no `debugger` is supplied a default debugger will be used.
    pub fn build(mut self) -> Result<Arc<Instance>, InstanceError> {
        //check if validation is enabled, in that case push the validation layers
        let has_val_layers = self.validation_layers.is_some();
        if has_val_layers {
            self = self.with_layer(CString::new("VK_LAYER_KHRONOS_validation").unwrap())?;
            self = self.with_extension(ash::extensions::ext::DebugUtils::name().to_owned())?;
        }

        let InstanceBuilder {
            entry,
            validation_layers: _,
            enabled_layers,
            enabled_extensions,
        } = self;
        /*
                let validation_features = if let Some(f) = validation_layers{
                    f
                }else{
                    ValidationFeatures::none()
                };
        */
        let app_desc = ash::vk::ApplicationInfo::builder().api_version(ash::vk::make_api_version(
            0,
            Instance::API_VERSION_MAJOR,
            Instance::API_VERSION_MINOR,
            Instance::API_VERSION_PATCH,
        ));

        //at this point, if we are logging, write out instance creation data
        #[cfg(feature = "logging")]
        {
            log::info!("Instance creation:");
            let apiversion = app_desc.api_version;
            log::info!(
                "  Vulkan version: {}.{}.{}.{}",
                ash::vk::api_version_major(apiversion),
                ash::vk::api_version_minor(apiversion),
                ash::vk::api_version_patch(apiversion),
                ash::vk::api_version_variant(apiversion)
            );
            log::info!("  Layers:");
            for l in &enabled_layers {
                log::info!("    {:?}", l);
            }
            log::info!("  Extensions:");
            for e in &enabled_extensions {
                log::info!("    {:?}", e);
            }
        }

        let enabled_extensions = enabled_extensions
            .iter()
            .map(|ext| ext.as_ptr())
            .collect::<Vec<_>>();

        let enabled_layers = enabled_layers
            .iter()
            .map(|ext| ext.as_ptr())
            .collect::<Vec<_>>();

        let create_info = ash::vk::InstanceCreateInfo::builder()
            .application_info(&app_desc)
            .enabled_extension_names(&enabled_extensions)
            .enabled_layer_names(&enabled_layers);
        /*
        let mut valext = vk::ValidationFeaturesEXT::builder()
            .enabled_validation_features(&validation_features.enabled)
            .disabled_validation_features(&validation_features.disabled);

        if has_val_layers{
            create_info = create_info.push_next(&mut valext);
            //now create the instance based on the provided create info
        }
        */
        let instance = unsafe { entry.create_instance(&create_info, None)? };

        //if validation is enabled, either unwrap the debugger, of create the new one
        let debugger = if has_val_layers {
            let (debug_report_loader, debug_messenger) = {
                //create the reporter
                let debug_report_loader = ash::extensions::ext::DebugUtils::new(&entry, &instance);

                //Now based on the Debug features create a debug callback
                let debug_info = ash::vk::DebugUtilsMessengerCreateInfoEXT::builder()
                    .message_severity(
                        ash::vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                            | ash::vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                            | ash::vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
                    )
                    .message_type(
                        ash::vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                            | ash::vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                            | ash::vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
                    )
                    .pfn_user_callback(Some(vulkan_debug_callback));

                let messenger =
                    unsafe { debug_report_loader.create_debug_utils_messenger(&debug_info, None)? };
                (debug_report_loader, messenger)
            };

            let debugger = Debugger {
                debug_messenger,
                debug_report_loader,
            };

            Some(debugger)
        } else {
            None
        };

        Ok(Arc::new(Instance {
            debugger,
            entry,
            inner: instance,
        }))
    }

    ///adds an extensions with the given name, if it was not added yet.
    pub fn with_extension(mut self, name: CString) -> Result<Self, InstanceError> {
        for e in &self.enabled_extensions {
            #[cfg(feature = "logging")]
            log::warn!("Tried to enable extension twice: {:?}", name);

            if e == &name {
                return Ok(self); //was enabled already
            }
        }

        //is not present, add and return
        self.enabled_extensions.push(name);

        Ok(self)
    }

    ///adds an layer with the given name to the list of layers
    pub fn with_layer(mut self, name: CString) -> Result<Self, InstanceError> {
        for l in &self.enabled_layers {
            #[cfg(feature = "logging")]
            log::warn!("Tried to enable layer twice: {:?}", name);

            if l == &name {
                return Ok(self); //was enabled already
            }
        }

        //is not present, add and return
        self.enabled_layers.push(name);

        Ok(self)
    }

    ///Enables all extensions that are needed for the surface behind `handle` to work.
    pub fn for_surface(mut self, handle: &dyn HasRawDisplayHandle) -> Result<Self, InstanceError> {
        let required_extensions =
            ash_window::enumerate_required_extensions(handle.raw_display_handle())?;
        for r in required_extensions {
            let st = unsafe { CStr::from_ptr(*r).to_owned() };
            self = self.with_extension(st)?;
        }

        Ok(self)
    }

    ///enables validation layers and implicitly sets a debugger that prints either via [println](println), or via the log crate if the `logging` feature is enabled.
    pub fn enable_validation(mut self, features: ValidationFeatures) -> Self {
        self.validation_layers = Some(features);
        self
    }
}

///marpii instance. Wraps the entry point as well as the created instance into one object.
///
/// # Safety
///
/// This struct is un-clonable for a reason. It implements [Drop] which takes care of destroying the vulkan instance, as well as the debug
/// messenger if it was loaded.
pub struct Instance {
    pub entry: ash::Entry,
    pub inner: ash::Instance,
    pub debugger: Option<Debugger>, //hidden detail. If loaded, has the debugger that gets called when printing in the validation layers.
}

impl Instance {
    ///The major version of Vulkan loaded.
    pub const API_VERSION_MAJOR: u32 = 1;
    ///The minor version of Vulkan loaded.
    pub const API_VERSION_MINOR: u32 = 3;
    ///The patch version of Vulkan loaded.
    pub const API_VERSION_PATCH: u32 = 0;

    ///Creates instance loaded by using [Entry::load](ash::Entry::load)
    pub fn load() -> Result<InstanceBuilder, InstanceError> {
        let entry = unsafe { ash::Entry::load()? };

        Ok(InstanceBuilder {
            entry,
            enabled_extensions: Vec::new(),
            enabled_layers: Vec::new(),
            validation_layers: None,
        })
    }

    ///Creates instance loaded by using [Entry::linked](ash::Entry::linked)
    pub fn linked() -> Result<InstanceBuilder, InstanceError> {
        let entry = ash::Entry::linked();

        Ok(InstanceBuilder {
            entry,
            enabled_extensions: Vec::new(),
            enabled_layers: Vec::new(),
            validation_layers: None,
        })
    }

    pub fn get_debugger(&self) -> Option<&Debugger> {
        self.debugger.as_ref()
    }

    ///Returns the feature list of the currently used physical device
    pub fn get_physical_device_features(
        &self,
        physical_device: &ash::vk::PhysicalDevice,
    ) -> ash::vk::PhysicalDeviceFeatures {
        unsafe { self.inner.get_physical_device_features(*physical_device) }
    }

    ///same as [get_physical_device_features](crate::context::Device::get_physical_device_features) but for PhysicalDeviceFetures2
    pub fn get_physical_device_features2(
        &self,
        physical_device: &ash::vk::PhysicalDevice,
    ) -> ash::vk::PhysicalDeviceFeatures2 {
        let mut features = ash::vk::PhysicalDeviceFeatures2::default();
        unsafe {
            self.inner
                .get_physical_device_features2(*physical_device, &mut features)
        };
        features
    }

    ///Returns the queried E.
    pub fn get_feature<E: ash::vk::ExtendsPhysicalDeviceFeatures2 + TaggedStructure>(
        &self,
        physical_device: &ash::vk::PhysicalDevice,
    ) -> E {
        //What we do to get E is that we try to upcast each element of the p_next chain of out feature list to E.

        //Create uninited E. This makes sure we reserved enough space for E.
        // We use zerode since this are *always* structs with 32bit per field, except for snext.
        // This somewhat sanitzes the values if the driver does not set the
        // zero values correctly.
        let mut q: MaybeUninit<E> = std::mem::MaybeUninit::zeroed();
        //cast to base struct to set stype. This lets the vulkan getter figure out what we want.
        let qptr = q.as_mut_ptr();
        unsafe {
            addr_of_mut!((*(qptr as *mut BaseOutStructure)).s_type).write(E::STRUCTURE_TYPE);
        }
        //push into chain
        let mut features2 =
            vk::PhysicalDeviceFeatures2::builder().push_next(unsafe { &mut *q.as_mut_ptr() });

        //issue query
        unsafe {
            self.inner
                .get_physical_device_features2(*physical_device, &mut features2);
        }
        //at this point we can assume q to be init.
        let query = unsafe { q.assume_init() };
        query
    }

    pub fn get_property<P: vk::ExtendsPhysicalDeviceProperties2 + TaggedStructure>(
        &self,
        physical_device: &ash::vk::PhysicalDevice,
    ) -> P {
        //Similar to how we get the feature above
        let mut q: MaybeUninit<P> = std::mem::MaybeUninit::zeroed();
        //cast to base struct to set stype. This lets the vulkan getter figure out what we want.
        let qptr = q.as_mut_ptr();
        unsafe {
            addr_of_mut!((*(qptr as *mut BaseOutStructure)).s_type).write(P::STRUCTURE_TYPE);
        }
        //push into chain
        let mut properties2 =
            vk::PhysicalDeviceProperties2::builder().push_next(unsafe { &mut *q.as_mut_ptr() });

        //issue query
        unsafe {
            self.inner
                .get_physical_device_properties2(*physical_device, &mut properties2);
        }
        //at this point we can assume q to be init.
        let query = unsafe { q.assume_init() };
        query
    }
}

pub trait GetDeviceFilter {
    fn create_physical_device_filter(&self) -> Result<PhysicalDeviceFilter, InstanceError>;
}

impl GetDeviceFilter for Arc<Instance> {
    fn create_physical_device_filter(&self) -> Result<PhysicalDeviceFilter, InstanceError> {
        let devices = unsafe { self.inner.enumerate_physical_devices()? };
        Ok(PhysicalDeviceFilter::new(&self.inner, devices))
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            //Destroy the messenger before destroying the instance.
            if let Some(drl) = &self.debugger {
                //destroys the messenger if it was loaded
                drl.debug_report_loader
                    .destroy_debug_utils_messenger(drl.debug_messenger, None);
            }
            self.inner.destroy_instance(None);
        }
    }
}
