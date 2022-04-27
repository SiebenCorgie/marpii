use super::{Queue, QueueBuilder};
use std::sync::Arc;

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
//              after building the temproray device create info.
impl DeviceBuilder {
    ///Checks that all device extensions are supported.
    fn check_extensions(&mut self) -> Result<(), anyhow::Error> {
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
            log::info!("Supported extensions");
            for ext in all_supported_names.iter() {
                log::info!("  {}", ext);
            }
        }

        for ext in self.device_extensions.iter() {
            //FIXME: ew this is dirty.
            let ext_as_str = unsafe { std::ffi::CStr::from_ptr(*ext) }
                .to_string_lossy()
                .as_ref()
                .to_owned();

            if !all_supported_names.contains(&ext_as_str) {
                anyhow::bail!("Extensions {:?} was not supported", ext_as_str);
            }
        }

        Ok(())
    }

    ///Allows changing `self` builder style
    pub fn with(mut self, mut mapping: impl FnMut(&mut DeviceBuilder)) -> Self {
        mapping(&mut self);
        self
    }

    ///Pushes the new extension. The name is usually optained from the extensions definition like this:
    ///```irgnore
    ///  builder.push_extension(ash::vk::KhrPipelineLibraryFn::name());
    ///```
    ///
    /// # Safety
    /// Pushing the same extensions is currently UB.
    //FIXME: while pushing, already check compatibility and reject either unsupported or already pushed
    //       extensions.
    pub fn push_extensions(mut self, ext_name: &'static std::ffi::CStr) -> Self {
        self.device_extensions.push(ext_name.as_ptr());
        self
    }

    ///Pushes an additional feature into the `p_next` chain
    pub fn with_additional_feature<T: 'static>(mut self, feature: T) -> Self
    where
        T: ash::vk::ExtendsDeviceCreateInfo,
    {
        self.p_next.push(Box::new(feature));
        self
    }

    pub fn build<'a>(mut self) -> Result<Arc<Device>, anyhow::Error> {
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
            //Chain the featuers together similar to the builders push
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

///Thin device abstraction that keeps the underlying instance (and therfore entrypoint) alive.
/// and takes care of device destruction once its dropped.
///
/// # Safety and self creation
/// Since the struct is compleatly public it is possible to create a device "on your own". In that case you'll have to make sure
/// that the instance is assosiated with the device and the queues actually exist.
pub struct Device {
    ///The raw ash device
    pub inner: ash::Device,
    pub instance: Arc<crate::context::Instance>,
    pub physical_device: ash::vk::PhysicalDevice,
    pub queues: Vec<Queue>,
}

impl Device {
    ///Mini helper function that creates the device from an already created instance and physical device, using
    /// the supplied device and creation infos.
    /// The function assumes that device and queues can be created from the device. No additional checking is done.
    ///
    /// # Safety
    /// The biggest concern when using this function should be that the queue_families of the `queue_builder` actully exist in that way,
    /// and that possibly enabled extensions in the `deviec_create_info` exist. Otherwise this either panics or fails, depending on the
    /// configured validation.
    pub unsafe fn new_from_info(
        instance: Arc<crate::context::Instance>,
        physical_device: ash::vk::PhysicalDevice,
        device_create_info: &ash::vk::DeviceCreateInfo,
        queue_builder: &[QueueBuilder],
    ) -> Result<Arc<Self>, anyhow::Error> {
        //finally create the queues and devic
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
                        inner: device
                            .get_device_queue(queue_family.family_index, queue_index as u32),
                    })
                    .collect::<Vec<Queue>>()
            })
            .flatten()
            .collect();

        Ok(Arc::new(Device {
            inner: device,
            instance,
            physical_device,
            queues,
        }))
    }

    ///Returns the first queue for the given family, if there is any.
    pub fn get_first_queue_for_family(&self, family: u32) -> Option<&Queue> {
        self.queues.iter().find(|q| q.family_index == family)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { self.inner.destroy_device(None) };
    }
}
