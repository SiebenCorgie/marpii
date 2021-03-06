use std::sync::Arc;

use const_cstr::const_cstr;

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
    if p_callback_data == core::ptr::null() {
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

pub struct Debugger {
    pub debug_report_loader: ash::extensions::ext::DebugUtils,
    pub debug_messenger: ash::vk::DebugUtilsMessengerEXT,
}

///Instance configuration as well as the source entry point. Usually this struct is created via [Instance::load] or [Instance::linked]
pub struct InstanceBuilder {
    pub entry: ash::Entry,
    pub validation_layers: bool,
    pub enabled_layers: Vec<std::ffi::CString>,
    pub enabled_extensions: Vec<std::ffi::CString>,
}

impl InstanceBuilder {
    ///Builds the instance from the current information.
    ///if `validation_layers` is enabled and no `debugger` is supplied a default debugger will be used.
    pub fn build(mut self) -> Result<Arc<Instance>, anyhow::Error> {
        //check if validation is enabled, in that case push the validation layers
        if self.validation_layers {
            self =
                self.with_layer(std::ffi::CString::new("VK_LAYER_KHRONOS_validation").unwrap())?;
            self = self.with_extension(ash::extensions::ext::DebugUtils::name().to_owned())?;
        }

        let InstanceBuilder {
            entry,
            validation_layers,
            enabled_layers,
            enabled_extensions,
        } = self;

        let app_desc =
            ash::vk::ApplicationInfo::builder().api_version(ash::vk::make_api_version(0, 1, 2, 0));

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

        //now create the instance based on the provided create info
        let instance = unsafe { entry.create_instance(&create_info, None)? };

        //if validation is enabled, either unwrap the debugger, of create the new one
        let debugger = if validation_layers {
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
    pub fn with_extension(mut self, name: std::ffi::CString) -> Result<Self, anyhow::Error> {
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
    pub fn with_layer(mut self, name: std::ffi::CString) -> Result<Self, anyhow::Error> {
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
    pub fn for_surface(
        mut self,
        handle: &dyn raw_window_handle::HasRawWindowHandle,
    ) -> Result<Self, anyhow::Error> {
        let required_extensions = ash_window::enumerate_required_extensions(handle)?;
        for r in required_extensions {
            self = self.with_extension(r.to_owned())?;
        }

        Ok(self)
    }

    ///enables validation layers and implicitly sets a debugger that prints either via [println](println), or via the log crate if the `logging` feature is enabled.
    pub fn enable_validation(mut self) -> Self {
        self.validation_layers = true;
        self
    }
}

///marpii instance. Wrapps the entry point as well as the created instance into one object.
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
    ///Creates instance loaded by using [Entry::load](ash::Entry::load)
    pub fn load() -> Result<InstanceBuilder, anyhow::Error> {
        let entry = unsafe { ash::Entry::load()? };

        Ok(InstanceBuilder {
            entry,
            enabled_extensions: Vec::new(),
            enabled_layers: Vec::new(),
            validation_layers: false,
        })
    }

    ///Creates instance loaded by using [Entry::linked](ash::Entry::linked)
    pub fn linked() -> Result<InstanceBuilder, anyhow::Error> {
        let entry = ash::Entry::linked();

        Ok(InstanceBuilder {
            entry,
            enabled_extensions: Vec::new(),
            enabled_layers: Vec::new(),
            validation_layers: false,
        })
    }
}

pub trait GetDeviceFilter {
    fn create_physical_device_filter(&self) -> Result<PhysicalDeviceFilter, anyhow::Error>;
}

impl GetDeviceFilter for Arc<Instance> {
    fn create_physical_device_filter(&self) -> Result<PhysicalDeviceFilter, anyhow::Error> {
        let devices = unsafe { self.inner.enumerate_physical_devices()? };
        Ok(PhysicalDeviceFilter::new(&self.inner, devices))
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            //Destroy the messenger before destroying the instance.
            if let Some(drl) = &self.debugger {
                //destroies the messenge if it was loaded
                drl.debug_report_loader
                    .destroy_debug_utils_messenger(drl.debug_messenger, None);
            }
            self.inner.destroy_instance(None);
        }
    }
}
