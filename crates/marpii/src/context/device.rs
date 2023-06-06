use ash::vk::{QueueFlags, TaggedStructure};

use crate::{error::DeviceError, resources::ImgDesc, util::image_usage_to_format_features};

use super::{Queue, QueueBuilder};
use std::{
    os::raw::c_char,
    sync::{Arc, Mutex},
};

///Helper that lets you setup device properties and possibly needed extensions before creating the actual
/// device.
pub struct DeviceBuilder {
    ///Instance based on which the device is creates
    pub instance: Arc<crate::context::Instance>,
    ///The physical device from which this will be an abstraction
    pub physical_device: ash::vk::PhysicalDevice,
    ///Queue family index, and properties of all queues that can be created.
    pub queues: Vec<QueueBuilder>,
    pub features: ash::vk::PhysicalDeviceFeatures,

    ///List of device extensions that are enabled. The pointer is usually optained via `ash::vk::EXTENSION::name().as_ptr()`.
    pub device_extensions: Vec<*const i8>,

    ///p_next elements
    pub p_next: Vec<Box<dyn ash::vk::ExtendsDeviceCreateInfo>>,
}

//TODO / FIXME: at the moment push structures are not checked for availability. However this could be done
//              after building the temporary device create info.
impl DeviceBuilder {
    ///Checks that all device extensions are supported.
    fn check_extensions(&mut self) -> Result<(), DeviceError> {
        let all_supported = unsafe {
            self.instance
                .inner
                .enumerate_device_extension_properties(self.physical_device)
        }
        .unwrap_or(Vec::new());

        let all_supported_names: Vec<String> = all_supported
            .iter()
            .map(|ext| {
                unsafe {
                    std::ffi::CStr::from_ptr(
                        ext.extension_name.as_ptr() as *const std::os::raw::c_char
                    )
                }
                .to_string_lossy()
                .as_ref()
                .to_owned()
            })
            .collect();

        #[cfg(feature = "logging")]
        {
            log::trace!("Supported extensions");
            for ext in all_supported_names.iter() {
                log::trace!("  {}", ext);
            }
        }

        for ext in self.device_extensions.iter() {
            //FIXME: ew this is dirty.
            let ext_as_str = unsafe { std::ffi::CStr::from_ptr(*ext) }
                .to_string_lossy()
                .as_ref()
                .to_owned();

            if !all_supported_names.contains(&ext_as_str) {
                return Err(DeviceError::UnsupportedExtension(ext_as_str));
            }
        }

        Ok(())
    }

    ///Allows changing `self` builder style
    pub fn with(mut self, mut mapping: impl FnMut(&mut DeviceBuilder)) -> Self {
        mapping(&mut self);
        self
    }

    ///Pushes the new extension. The name is usually obtained from the extensions definition like this:
    ///```irgnore
    ///  builder.push_extension(ash::vk::KhrPipelineLibraryFn::name());
    ///```
    ///
    /// # Safety
    /// Pushing the same extensions is currently UB.
    //FIXME: while pushing, already check compatibility and reject either unsupported or already pushed
    //       extensions.
    pub fn with_extensions(mut self, ext_name: &'static std::ffi::CStr) -> Self {
        self.device_extensions.push(ext_name.as_ptr());
        self
    }

    ///Pushes an additional feature into the `p_next` chain.
    pub fn with_feature<T: 'static>(mut self, feature: T) -> Self
    where
        T: ash::vk::ExtendsDeviceCreateInfo,
    {
        self.p_next.push(Box::new(feature));
        self
    }

    pub fn build<'a>(mut self) -> Result<Arc<Device>, DeviceError> {
        //before starting anything, check that the extensions are supported
        self.check_extensions()?;

        let DeviceBuilder {
            instance,
            physical_device,
            queues,
            features,
            device_extensions,
            mut p_next,
        } = self;

        //now unwrap the queue infos into create infos
        let queue_create_infos = queues
            .iter()
            .map(|q| *q.as_create_info())
            .collect::<Vec<_>>();

        //now create the pre-create DeviceCreation info
        //NOTE: acording to the vulkan doc device layers are deprecated. We therfore don't expose
        //anything related to that. However use defined functions on the builder could use this functionality.
        let device_creation_info = ash::vk::DeviceCreateInfo::builder()
            .enabled_extension_names(&device_extensions)
            .enabled_features(&features)
            .queue_create_infos(&queue_create_infos);

        //if there is a p_next queue, build the pointer queue and add it to the builder
        let mut create_info = device_creation_info.build();
        if p_next.len() > 0 {
            //Chain the features together similar to the builders push
            let chain = p_next
                .iter_mut()
                .fold(
                    None,
                    |last: Option<&mut Box<dyn ash::vk::ExtendsDeviceCreateInfo>>, new| {
                        if let Some(last) = last {
                            let mut new_ptr =
                                new.as_mut() as *mut _ as *mut ash::vk::BaseOutStructure;
                            unsafe {
                                (*new_ptr).p_next =
                                    last.as_mut() as *mut _ as *mut ash::vk::BaseOutStructure;
                            }
                            Some(new)
                        } else {
                            Some(new)
                        }
                    },
                )
                .unwrap();
            create_info.p_next = chain.as_ref() as *const _ as *const core::ffi::c_void;
        }

        unsafe { Device::new_from_info(instance, physical_device, &create_info, &queues) }
    }
}

///Thin device abstraction that keeps the underlying instance (and therefore entrypoint) alive.
/// and takes care of device destruction once its dropped.
///
/// # Safety and self creation
/// Since the struct is completely public it is possible to create a device "on your own". In that case you'll have to make sure
/// that the instance is associated with the device and the queues actually exist.
pub struct Device {
    ///The raw ash device
    pub inner: ash::Device,
    pub instance: Arc<crate::context::Instance>,
    pub physical_device: ash::vk::PhysicalDevice,
    pub queues: Vec<Queue>,

    pub enabled_extensions: Vec<String>,

    pub physical_device_properties: ash::vk::PhysicalDeviceProperties,
}

impl Device {
    ///Mini helper function that creates the device from an already created instance and physical device, using
    /// the supplied device and creation infos.
    /// The function assumes that device and queues can be created from the device. No additional checking is done.
    ///
    /// # Safety
    /// The biggest concern when using this function should be that the queue_families of the `queue_builder` actually exist in that way,
    /// and that possibly enabled extensions in the `device_create_info` exist. Otherwise this either panics or fails, depending on the
    /// configured validation.
    pub unsafe fn new_from_info(
        instance: Arc<crate::context::Instance>,
        physical_device: ash::vk::PhysicalDevice,
        device_create_info: &ash::vk::DeviceCreateInfo,
        queue_builder: &[QueueBuilder],
    ) -> Result<Arc<Self>, DeviceError> {
        //finally create the queues and device
        let device = instance
            .inner
            .create_device(physical_device, &device_create_info, None)?;
        //now setup the queues for the infos we prepared before
        let queues = queue_builder
            .into_iter()
            .map(|queue_family| {
                (0..queue_family.priorities.len())
                    .map(|queue_index| Queue {
                        family_index: queue_family.family_index,
                        properties: queue_family.properties,
                        inner: Arc::new(Mutex::new(
                            device.get_device_queue(queue_family.family_index, queue_index as u32),
                        )),
                    })
                    .collect::<Vec<Queue>>()
            })
            .flatten()
            .collect();

        let physical_device_properties = instance
            .inner
            .get_physical_device_properties(physical_device);

        let enabled_extensions = {
            let extension_properties = instance
                .inner
                .enumerate_device_extension_properties(physical_device)?;

            extension_properties
                .iter()
                .map(|ext| {
                    std::ffi::CStr::from_ptr(ext.extension_name.as_ptr() as *const c_char)
                        .to_string_lossy()
                        .as_ref()
                        .to_owned()
                })
                .collect()
        };

        Ok(Arc::new(Device {
            inner: device,
            instance,
            physical_device,
            enabled_extensions,
            queues,
            physical_device_properties,
        }))
    }

    ///Returns true if `extension_name` is enabled. The name can usualy be retrieved like this: `ash::extensions::khr::DynamicRendering::name()`.
    pub fn extension_enabled_cstr(&self, extension_name: &'static std::ffi::CStr) -> bool {
        //FIXME do not convert to String which allocates
        self.enabled_extensions
            .contains(&extension_name.to_string_lossy().to_string())
    }

    pub fn extension_enabled(&self, extension_name: &str) -> bool {
        //FIXME do not convert to String which allocates
        self.enabled_extensions.iter().fold(
            false,
            |flag, this| if flag { flag } else { this == extension_name },
        )
    }

    ///Returns the feature list of the currently used physical device
    pub fn get_physical_device_features(&self) -> ash::vk::PhysicalDeviceFeatures {
        self.instance
            .get_physical_device_features(&self.physical_device)
    }

    ///same as [get_physical_device_features](crate::context::Device::get_physical_device_features) but for PhysicalDeviceFetures2
    pub fn get_physical_device_features2(&self) -> ash::vk::PhysicalDeviceFeatures2 {
        self.instance
            .get_physical_device_features2(&self.physical_device)
    }

    ///Returns the queried E.
    pub fn get_feature<E: ash::vk::ExtendsPhysicalDeviceFeatures2 + TaggedStructure>(&self) -> E {
        self.instance.get_feature(&self.physical_device)
    }

    pub fn get_property<P: ash::vk::ExtendsPhysicalDeviceProperties2 + TaggedStructure>(
        &self,
    ) -> P {
        self.instance.get_property(&self.physical_device)
    }

    pub fn get_device_properties(&self) -> ash::vk::PhysicalDeviceProperties2 {
        let mut properties = ash::vk::PhysicalDeviceProperties2::default();
        unsafe {
            self.instance
                .inner
                .get_physical_device_properties2(self.physical_device, &mut properties)
        };
        properties
    }

    ///Returns the first queue for the given family, if there is any.
    pub fn get_first_queue_for_family(&self, family: u32) -> Option<&Queue> {
        self.queues.iter().find(|q| q.family_index == family)
    }

    ///Returns the first queue that has all attributes flaged as true
    pub fn first_queue_for_attribute(
        &self,
        graphics: bool,
        compute: bool,
        transfer: bool,
    ) -> Option<&Queue> {
        self.queues.iter().find(|q| {
            let mut is = true;
            if graphics && !q.properties.queue_flags.contains(QueueFlags::GRAPHICS) {
                is = false;
            }
            if compute && !q.properties.queue_flags.contains(QueueFlags::COMPUTE) {
                is = false;
            }
            if transfer && !q.properties.queue_flags.contains(QueueFlags::TRANSFER) {
                is = false;
            }
            is
        })
    }

    pub fn is_format_supported(
        &self,
        usage: ash::vk::ImageUsageFlags,
        tiling: ash::vk::ImageTiling,
        format: ash::vk::Format,
    ) -> bool {
        let format_properties = unsafe {
            self.instance
                .inner
                .get_physical_device_format_properties(self.physical_device, format)
        };

        //now check if the tiling mode and usage are given by the properties
        if tiling == ash::vk::ImageTiling::LINEAR {
            format_properties
                .linear_tiling_features
                .contains(image_usage_to_format_features(usage))
        } else {
            format_properties
                .optimal_tiling_features
                .contains(image_usage_to_format_features(usage))
        }
    }

    ///Selects the first format of the provided formats that can be used with `usage` on `self`.
    pub fn select_format(
        &self,
        usage: ash::vk::ImageUsageFlags,
        tiling: ash::vk::ImageTiling,
        formats: &[ash::vk::Format],
    ) -> Option<ash::vk::Format> {
        formats.iter().find_map(|f| {
            if self.is_format_supported(usage, tiling, *f) {
                Some(*f)
            } else {
                None
            }
        })
    }

    ///Returns the image format properties for the given image description (`desc`), assuming the image was/is created with `create_flags`.
    pub fn image_format_properties(
        &self,
        desc: &ImgDesc,
        create_flags: ash::vk::ImageCreateFlags,
    ) -> Result<ash::vk::ImageFormatProperties2, ash::vk::Result> {
        let mut properties = ash::vk::ImageFormatProperties2::default();
        unsafe {
            self.instance
                .inner
                .get_physical_device_image_format_properties2(
                    self.physical_device,
                    &ash::vk::PhysicalDeviceImageFormatInfo2 {
                        flags: create_flags,
                        format: desc.format,
                        tiling: desc.tiling,
                        ty: desc.img_type.into(),
                        usage: desc.usage,
                        ..Default::default()
                    },
                    &mut properties,
                )?;
        };

        Ok(properties)
    }

    ///Returns true if the format is usable with the intended usage. Can be used in a `Filter` iterator to select
    /// usable formats at runtime
    pub fn get_image_format_properties(
        &self,
        format: ash::vk::Format,
        ty: ash::vk::ImageType,
        tiling: ash::vk::ImageTiling,
        usage: ash::vk::ImageUsageFlags,
        crate_flags: ash::vk::ImageCreateFlags,
    ) -> Result<ash::vk::ImageFormatProperties, DeviceError> {
        match unsafe {
            self.instance
                .inner
                .get_physical_device_image_format_properties(
                    self.physical_device,
                    format,
                    ty,
                    tiling,
                    usage,
                    crate_flags,
                )
        } {
            Err(e) => {
                #[cfg(feature = "logging")]
                log::error!("Failed to get image format properties: {e}");
                return Err(DeviceError::GetFormatProperties { format, error: e });
            }
            Ok(o) => Ok(o),
        }
    }

    ///Moves `offset` to next lower multiple of DeviceLimits::nonCoherentAtomSize.
    pub fn offset_to_next_lower_coherent_atom_size(&self, offset: u64) -> u64 {
        let atom_size = self
            .physical_device_properties
            .limits
            .non_coherent_atom_size;
        offset - (offset % atom_size)
    }

    ///Moves `offset` to next higher multiple of DeviceLimits::nonCoherentAtomSize.
    pub fn offset_to_next_higher_coherent_atom_size(&self, offset: u64) -> u64 {
        let atom_size = self
            .physical_device_properties
            .limits
            .non_coherent_atom_size;
        //NOTE second mod needed to not +atom_size if offset%atom_size==0
        offset + ((atom_size - (offset % atom_size)) % atom_size)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { self.inner.destroy_device(None) };
    }
}
