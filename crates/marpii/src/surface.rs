use std::sync::Arc;

///using [ash-window](https://crates.io/crates/ash-window) to safely find a surface for a given window
/// handle. Also keeps the instance alive lon enough to destroy the created surface in time.
pub struct Surface {
    ///keeps the surface alive
    pub instance: Arc<crate::context::Instance>,
    pub surface: ash::vk::SurfaceKHR,
    pub surface_loader: ash::extensions::khr::Surface,
}

impl Surface {
    pub fn new(
        instance: &Arc<crate::context::Instance>,
        window_handle: &dyn raw_window_handle::HasRawWindowHandle,
    ) -> Result<Self, anyhow::Error> {
        let surface = unsafe {
            ash_window::create_surface(&instance.entry, &instance.inner, window_handle, None)?
        };
        let surface_loader = ash::extensions::khr::Surface::new(&instance.entry, &instance.inner);

        Ok(Surface {
            instance: instance.clone(),
            surface,
            surface_loader,
        })
    }

    pub fn get_capabilities(
        &self,
        physical_device: ash::vk::PhysicalDevice,
    ) -> Result<ash::vk::SurfaceCapabilitiesKHR, anyhow::Error> {
        Ok(unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(physical_device, self.surface)?
        })
    }

    pub fn get_formats(
        &self,
        physical_device: ash::vk::PhysicalDevice,
    ) -> Result<Vec<ash::vk::SurfaceFormatKHR>, anyhow::Error> {
        Ok(unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(physical_device, self.surface)?
        })
    }

    pub fn get_present_modes(
        &self,
        physical_device: ash::vk::PhysicalDevice,
    ) -> Result<Vec<ash::vk::PresentModeKHR>, anyhow::Error> {
        Ok(unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(physical_device, self.surface)?
        })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { self.surface_loader.destroy_surface(self.surface, None) };
    }
}
