//! Implementation of helper functions that let you facilitate a WGPU context from a MarpII context.

use std::ffi::{CStr, CString};

use marpii::context::InstanceBuilder;
use thiserror::Error;
use wgpu::RequestDeviceError;

#[derive(Error, Debug)]
pub enum MarpiiWgpuError {
    #[error("Adapter is not created for Vulkan backend")]
    AdapterNotVulkan,
    #[error(transparent)]
    RequestDeviceError(#[from] RequestDeviceError),
    #[error(transparent)]
    WgpuInstanceError(#[from] wgpu::hal::InstanceError),
    #[error("Vulkan instance is missing extension: {0:?}")]
    MissingInstanceExtention(CString),
    #[error("Vulkan device extention missing: {0:?}")]
    MissingDeviceExtension(CString),
    #[error(transparent)]
    MarpiiInstanceError(#[from] marpii::InstanceError),
    #[error(transparent)]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),
}

///Configures the `builder` to support a WGPU instance
pub fn instance_builder_for_wgpu(
    mut builder: InstanceBuilder,
) -> Result<InstanceBuilder, MarpiiWgpuError> {
    let desired = wgpu_desired_extensions(&builder.entry, true)?;
    for ext in desired {
        if builder.is_extension_available_cstr(ext) {
            builder = builder.with_extension(ext.to_owned())?;
        }
    }

    Ok(builder)
}

///Returns a list of all instance extensions that *should* be enabled, if available
pub fn wgpu_desired_extensions(
    entry: &marpii::ash::Entry,
    validation_enabled: bool,
) -> Result<Vec<&'static CStr>, MarpiiWgpuError> {
    let instance_version = marpii::context::Instance::api_version();
    let mut flags = wgpu::InstanceFlags::empty();
    if validation_enabled {
        flags |= wgpu::InstanceFlags::VALIDATION;
    }
    wgpu::hal::vulkan::Instance::desired_extensions(entry, instance_version, flags)
        .map_err(|e| e.into())
}

///Tries to build a WGPU Instance from a marpii instance.
pub fn wgpu_instance(
    vulkan_instance: &marpii::context::Instance,
) -> Result<wgpu::Instance, MarpiiWgpuError> {
    let desired_extensions =
        wgpu_desired_extensions(&vulkan_instance.entry, vulkan_instance.validation_enabled())?;

    for ext in &desired_extensions {
        let has_extension = vulkan_instance
            .enabled_extensions()
            .iter()
            .map(|name| name.as_c_str())
            .find(|name| name == ext);
        //NOTE: WGPU does function without *some* of those. But to be save we want _all_.
        if has_extension.is_none() {
            let extname = (**ext).to_owned();
            return Err(MarpiiWgpuError::MissingInstanceExtention(extname));
        }
    }

    let mut flags = wgpu::InstanceFlags::empty();
    if vulkan_instance.validation_enabled() {
        flags |= wgpu::InstanceFlags::VALIDATION;
    }

    //If that went well, create the wgpu-vulkan instance, then wrap that into a wgpu-instance
    let wgpu_vulkan_instance = unsafe {
        wgpu::hal::vulkan::Instance::from_raw(
            vulkan_instance.entry.clone(),
            vulkan_instance.inner.clone(),
            marpii::context::Instance::api_version(),
            //FIXME: If we ever want to support android, this should be set!
            0,
            //Always None, since the callback would be set by the caller
            None,
            //NOTE: We feed back the garbage &'static CStr, since Marpii is not designed to carry
            //      around 'static stuff.
            desired_extensions,
            flags,
            false,
            None,
        )
    }?;

    //now build a actual wgpu instance
    let wgp_instance =
        unsafe { wgpu::Instance::from_hal::<wgpu::core::api::Vulkan>(wgpu_vulkan_instance) };

    Ok(wgp_instance)
}

///Creates a wgpu adapter from a marpii device
pub fn wgpu_device(
    wgpu_instance: &wgpu::Instance,
    device: &marpii::context::Device,
    queue: &marpii::context::Queue,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), MarpiiWgpuError> {
    let features = wgpu::Features::all_native_mask();

    let exposed_adapter = unsafe { wgpu_instance.as_hal::<wgpu::core::api::Vulkan>() }
        .expect("Expected wgpu_instance to be a vulkan instance")
        .expose_adapter(device.physical_device)
        .unwrap();

    let expected_features = exposed_adapter.adapter.required_device_extensions(features);

    for expected in &expected_features {
        if !device.extension_enabled_cstr(*expected) {
            return Err(MarpiiWgpuError::MissingDeviceExtension(
                (*expected).to_owned(),
            ));
        }
    }

    //Try to load the adapter
    let wgpu_vulkan_device = unsafe {
        exposed_adapter.adapter.device_from_raw(
            device.inner.clone(),
            //TODO: what is handle-owned?
            false,
            &expected_features,
            features,
            &wgpu::MemoryHints::Performance,
            queue.family_index,
            //always take first queue of that family for now.
            0,
        )?
    };

    let wrapped_adapter = unsafe { wgpu_instance.create_adapter_from_hal(exposed_adapter) };

    let device_descriptor = wgpu::DeviceDescriptor {
        label: Some("MarpII hosted Vulkan Device"),
        required_features: features,
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
    };

    let (device, queue) = unsafe {
        wrapped_adapter.create_device_from_hal(wgpu_vulkan_device, &device_descriptor, None)
    }?;

    Ok((wrapped_adapter, device, queue))
}
