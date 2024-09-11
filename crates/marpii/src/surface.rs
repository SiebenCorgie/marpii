use std::sync::Arc;

use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::error::InstanceError;

///using [ash-window](https://crates.io/crates/ash-window) to safely find a surface for a given window
/// handle. Also keeps the instance alive long enough to destroy the created surface in time.
pub struct Surface {
    ///keeps the surface alive
    pub instance: Arc<crate::context::Instance>,
    pub surface: ash::vk::SurfaceKHR,
    pub surface_loader: ash::khr::surface::Instance,
}

impl Surface {
    pub fn new<T>(
        instance: &Arc<crate::context::Instance>,
        window_handle: &T,
    ) -> Result<Self, InstanceError>
    where
        T: HasWindowHandle + HasDisplayHandle,
    {
        let surface = unsafe {
            ash_window::create_surface(
                &instance.entry,
                &instance.inner,
                window_handle.display_handle().unwrap().as_raw(),
                window_handle.window_handle().unwrap().as_raw(),
                None,
            )?
        };
        let surface_loader = ash::khr::surface::Instance::new(&instance.entry, &instance.inner);

        Ok(Surface {
            instance: instance.clone(),
            surface,
            surface_loader,
        })
    }

    pub fn get_capabilities(
        &self,
        physical_device: &ash::vk::PhysicalDevice,
    ) -> Result<ash::vk::SurfaceCapabilitiesKHR, InstanceError> {
        Ok(unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(*physical_device, self.surface)?
        })
    }

    pub fn get_formats(
        &self,
        physical_device: ash::vk::PhysicalDevice,
    ) -> Result<Vec<ash::vk::SurfaceFormatKHR>, InstanceError> {
        Ok(unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(physical_device, self.surface)?
        })
    }

    pub fn get_present_modes(
        &self,
        physical_device: ash::vk::PhysicalDevice,
    ) -> Result<Vec<ash::vk::PresentModeKHR>, InstanceError> {
        Ok(unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(physical_device, self.surface)?
        })
    }

    ///Tries to read the current surface extent. This can fail on some platforms (like Linux+Wayland).
    /// Note that this can be different than the swapchain extent, for instace right after a resize.
    pub fn get_current_extent(&self, physical_device: &vk::PhysicalDevice) -> Option<vk::Extent2D> {
        let extent = self
            .get_capabilities(physical_device)
            .unwrap()
            .current_extent;
        //if on wayland this will be wrong, check and maybe return nothing.
        match extent {
            vk::Extent2D {
                width: 0xFFFFFFFF,
                height: 0xFFFFFFFF,
            }
            | vk::Extent2D {
                width: 0,
                height: 0,
            } => None,
            vk::Extent2D {
                width: 0x4000,
                height: 0x4000,
            } => {
                //FIXME: Not sure why, but on wayland+Intel this size gets reported on startup, which is wrong.
                #[cfg(feature = "logging")]
                log::warn!(
                    "possibly wrong swapchain extent of {:?}, falling back to 512x512",
                    extent
                );

                Some(vk::Extent2D {
                    width: 512,
                    height: 512,
                })
            }
            vk::Extent2D { width, height } => Some(vk::Extent2D { width, height }),
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { self.surface_loader.destroy_surface(self.surface, None) };
    }
}
