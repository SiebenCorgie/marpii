use crate::error::DeviceError;

use super::{DeviceBuilder, QueueBuilder};
use std::sync::Arc;

///Collection off all properties for this physical device. Can be used to easily create a [DeviceBuilder](DeviceBuilder).
/// Is usually acquired from a [PhysicaldeviceFilter](PhysicalDeviceFilter), or by using `new`.
pub struct PhyDeviceProperties {
    pub phydev: ash::vk::PhysicalDevice,
    pub properties: ash::vk::PhysicalDeviceProperties,
    pub queue_properties: Vec<(usize, ash::vk::QueueFamilyProperties)>,
}

impl PhyDeviceProperties {
    ///Creates Self from just a physical device definition. Fills in `queue_properties` with all available properties.
    pub fn new(instance: &ash::Instance, physical_device: ash::vk::PhysicalDevice) -> Self {
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let queues =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

        PhyDeviceProperties {
            phydev: physical_device,
            properties,
            queue_properties: queues.into_iter().enumerate().collect(),
        }
    }

    ///creates a device builder for this physical device and its current properties
    pub fn into_device_builder(
        self,
        instance: Arc<crate::context::Instance>,
    ) -> Result<DeviceBuilder, DeviceError> {
        Ok(DeviceBuilder {
            instance,
            physical_device: self.phydev,
            queues: self
                .queue_properties
                .into_iter()
                .map(|(idx, properties)| QueueBuilder {
                    family_index: idx as u32,
                    properties,
                    priorities: vec![1.0], //per default create one queue
                })
                .collect(),
            device_extensions: Vec::new(),
            features: ash::vk::PhysicalDeviceFeatures::default(),
            p_next: Vec::new(),
        })
    }
}

///Filter that lets you select a sub-set of all physical devices.
/// use [ash::Instance::enumerate_physical_devices](ash::Instance::enumerate_physical_devices) to get a list of all devices
/// and [PhysicalDeviceFilter::new](PhysicalDeviceFilter::new) to create this filter.
pub struct PhysicalDeviceFilter {
    ///All available devices.
    pub pdevices: Vec<PhyDeviceProperties>,
}

impl PhysicalDeviceFilter {
    pub fn new(instance: &ash::Instance, phydevices: Vec<ash::vk::PhysicalDevice>) -> Self {
        PhysicalDeviceFilter {
            pdevices: phydevices
                .into_iter()
                .map(|phy| {
                    let properties = unsafe { instance.get_physical_device_properties(phy) };
                    let queues =
                        unsafe { instance.get_physical_device_queue_family_properties(phy) };

                    PhyDeviceProperties {
                        phydev: phy,
                        properties,
                        queue_properties: queues.into_iter().enumerate().collect(),
                    }
                })
                .collect(),
        }
    }

    ///removes all devices that do not contain the device type bits.
    pub fn filter_type(mut self, dev_type: ash::vk::PhysicalDeviceType) -> Self {
        self.pdevices = self
            .pdevices
            .into_iter()
            .filter(|dev| dev.properties.device_type == dev_type)
            .collect();

        self
    }

    ///removes all queues that do not contain a queue with the given flags
    pub fn filter_queue_flags(mut self, flags: ash::vk::QueueFlags) -> Self {
        self.pdevices = self
            .pdevices
            .into_iter()
            .filter(|dev| {
                let mut has = false;
                for (_idx, f) in dev.queue_properties.iter() {
                    #[cfg(feature = "logging")]
                    log::info!("Checking {:?} for {:?}", f.queue_flags, flags);
                    if f.queue_flags.contains(flags) {
                        has = true;
                        break;
                    }
                }

                has
            })
            .collect();

        self
    }

    ///Custom filter on the cached properties
    pub fn filter<F>(mut self, filter: F) -> Self
    where
        F: FnMut(&PhyDeviceProperties) -> bool,
    {
        self.pdevices = self.pdevices.into_iter().filter(filter).collect();
        self
    }

    ///Removes all devices and queues that can not present on the supplied surface
    pub fn filter_presentable(
        mut self,
        surface_loader: &ash::khr::surface::Instance,
        surface: &ash::vk::SurfaceKHR,
    ) -> Self {
        self.pdevices = self.pdevices.into_iter().filter_map(|mut pdev|{
            //check each queue if it is presentable, if not filter out queue
            pdev.queue_properties = pdev.queue_properties.into_iter().filter(|(qidx, _queue)| {
                if let Ok(res) = unsafe{surface_loader.get_physical_device_surface_support(pdev.phydev, *qidx as u32, *surface)}{
                    res
                }else{
                    #[cfg(feature="logging")]
                    log::warn!("Failed to query surface capability on queue family {} of physical device: {:?}", qidx, pdev.properties.device_name);
                    false
                }
            }).collect();
            //Check if any family is left, otherwise remove device completely
            if pdev.queue_properties.len() > 0{
                Some(pdev)
            }else{
                None
            }
        }).collect();
        self
    }

    ///Releases the current filtered physical devices and queues. You can use [into_device_builder](PhyDeviceProperties::into_device_builder) to start and create an abstract device for these.
    pub fn release(self) -> Vec<PhyDeviceProperties> {
        self.pdevices
    }
}
