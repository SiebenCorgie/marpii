use std::ffi::CStr;

use ash::vk::{self, Handle};

///Helper that gets usually initialised by activating validation layers.
/// Allows to use all `VK_EXT_DEBUG_UTILS` functions.
pub struct Debugger {
    pub debug_instance: ash::ext::debug_utils::Instance,
    pub debug_report_loader: ash::ext::debug_utils::Device,
    pub debug_messenger: ash::vk::DebugUtilsMessengerEXT,
}

impl Debugger {
    pub fn name_object<H: Handle>(&self, handle: H, name: &CStr) -> Result<(), vk::Result> {
        let info = vk::DebugUtilsObjectNameInfoEXT::default()
            .object_name(name)
            .object_handle(handle);
        unsafe { self.debug_report_loader.set_debug_utils_object_name(&info) }
    }
}

impl Drop for Debugger {
    fn drop(&mut self) {
        //destroys the messenger if it was loaded
        unsafe {
            self.debug_instance
                .destroy_debug_utils_messenger(self.debug_messenger, None)
        };
    }
}
