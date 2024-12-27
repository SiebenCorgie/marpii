//! Implementation of helper functions that let you facilitate a WGPU context from a MarpII context.

///Convention utilities used to exchange Wgpu and Vulkan (Ash) data.
pub mod conv;
use std::{
    ffi::{CStr, CString},
    sync::Arc,
};

use marpii::{
    allocator::Allocator,
    ash::vk::{self, ImageCreateFlags, SampleCountFlags},
    context::{Ctx, InstanceBuilder},
    resources::ImgDesc,
};
use thiserror::Error;
use wgpu::{RequestDeviceError, TextureDimension};

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
    #[error("Vulkan context had no graphics and compute capable queue")]
    NoGraphicQueue,
    #[error("Can not convert {0:?} to valid wgpu-hal texture")]
    UnsupportedHalTexture(marpii::resources::ImageType),
    #[error("Could not map Vulkan format {0:?} to Wgpu TextureFormat")]
    VkFormatError(marpii::ash::vk::Format),
    #[error("Could not map Vulkan image usage flag {0:?} to Wgpu TextureUsages")]
    VkImgUsageError(marpii::ash::vk::ImageUsageFlags),
    #[error("Could not map WGPU TextureUsage {0:?} to vulkan ImageUsageFlags")]
    WgpuImageUsage(wgpu::TextureUsages),
    #[error("Sample count {0} not supported by Vulkan")]
    UnsupportedSampleCount(u32),
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

    let enabled_extensions = desired_extensions
        .into_iter()
        .filter_map(|ext| {
            let has_extension = vulkan_instance
                .enabled_extensions()
                .iter()
                .map(|name| name.as_c_str())
                .find(|name| *name == ext);
            if has_extension.is_some() {
                Some(ext)
            } else {
                None
            }
        })
        .collect();

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
            enabled_extensions,
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

    println!("All DeviceExtensios:\n{:#?}", device.enabled_extensions);

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

        println!("{:?} is enabled!", expected);
    }

    //Try to load the adapter
    let wgpu_vulkan_device = unsafe {
        exposed_adapter.adapter.device_from_raw(
            device.inner.clone(),
            None,
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

///WGPU equivalent to [Ctx](marpii::context::Ctx). Makes sure that the [Ctx](marpii::context::Ctx) outlives
///the Wgpu components.
pub struct WgpuCtx {
    wgpu_instance: wgpu::Instance,
    wgpu_adapter: wgpu::Adapter,
    wgpu_device: wgpu::Device,
    wgpu_queue: wgpu::Queue,

    //NOTE: we don't clone the allocator, since wgpu uses its own allocator.
    //      This also makes sure, that whenever wgpu is dropped, everything is de-allocated before the
    //      MarpII structures are dropped.
    #[allow(dead_code)]
    marpii_instance: Arc<marpii::context::Instance>,
    #[allow(dead_code)]
    marpii_device: Arc<marpii::context::Device>,
    queue_family_index: u32,
}

impl WgpuCtx {
    pub fn new<A: Allocator + Send>(marpii_context: &Ctx<A>) -> Result<Self, MarpiiWgpuError> {
        let wgpu_instance = wgpu_instance(&marpii_context.instance)?;
        let graphics_queue = marpii_context
            .device
            .first_queue_for_attribute(true, true, false)
            .ok_or(MarpiiWgpuError::NoGraphicQueue)?;
        let queue_family_index = graphics_queue.family_index;
        let (wgpu_adapter, wgpu_device, wgpu_queue) =
            wgpu_device(&wgpu_instance, &marpii_context.device, &graphics_queue)?;

        Ok(WgpuCtx {
            wgpu_instance,
            wgpu_adapter,
            wgpu_device,
            wgpu_queue,

            marpii_instance: marpii_context.instance.clone(),
            marpii_device: marpii_context.device.clone(),
            queue_family_index,
        })
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.wgpu_adapter
    }
    pub fn instance(&self) -> &wgpu::Instance {
        &self.wgpu_instance
    }
    pub fn device(&self) -> &wgpu::Device {
        &self.wgpu_device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.wgpu_queue
    }

    pub fn vulkan_queue_family_index(&self) -> u32 {
        self.queue_family_index
    }

    ///tries to convert the `texture` to an marpii image.
    ///
    /// # Safety
    /// Image should not be dropped, and livetime must be ensured by the caller. `texture` must life at least as long as the resulting image is in use
    pub unsafe fn texture_as_vk_image(
        &self,
        texture: &wgpu::Texture,
    ) -> Result<marpii::resources::Image, MarpiiWgpuError> {
        texture.as_hal::<wgpu::hal::vulkan::Api, _, _>(|tex| {
            if let Some(tex) = tex {
                let image = marpii::resources::Image {
                    desc: ImgDesc {
                        img_type: match texture.dimension() {
                            TextureDimension::D1 => marpii::resources::ImageType::Tex1d,
                            TextureDimension::D2 => marpii::resources::ImageType::Tex2d,
                            TextureDimension::D3 => marpii::resources::ImageType::Tex3d,
                        },
                        format: conv::map_wgpu_to_vk_texture_format(texture.format()),
                        extent: marpii::ash::vk::Extent3D {
                            width: texture.width(),
                            height: texture.height(),
                            depth: texture.depth_or_array_layers(),
                        },
                        mip_levels: texture.mip_level_count(),
                        samples: match texture.sample_count() {
                            1 => SampleCountFlags::TYPE_1,
                            2 => SampleCountFlags::TYPE_2,
                            4 => SampleCountFlags::TYPE_4,
                            8 => SampleCountFlags::TYPE_8,
                            16 => SampleCountFlags::TYPE_16,
                            _ => {
                                return Err(MarpiiWgpuError::UnsupportedSampleCount(
                                    texture.sample_count(),
                                ))
                            }
                        },
                        //TODO: is that right?
                        tiling: vk::ImageTiling::OPTIMAL,
                        usage: conv::map_wgpu_to_vk_image_usage(texture.usage()),
                        sharing_mode: marpii::resources::SharingMode::Exclusive,
                        create_flags: ImageCreateFlags::empty(),
                    },
                    inner: tex.raw_handle(),
                    allocation: Box::new(unsafe { marpii::allocator::UnmanagedAllocation::new() }),
                    usage: marpii::allocator::MemoryUsage::Unknown,
                    device: self.marpii_device.clone(),
                    //do not destroy, since it is handeled by wgpu
                    do_not_destroy: true,
                };
                Ok(image)
            } else {
                Err(MarpiiWgpuError::AdapterNotVulkan)
            }
        })
    }
    /*
    pub fn texture_as_hal(
        &self,
        texture: marpii::resources::Image,
    ) -> Result<wgpu::Texture, MarpiiWgpuError> {
        let desc = wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: texture.extent_3d().width,
                height: texture.extent_3d().height,
                depth_or_array_layers: texture.extent_3d().depth,
            },
            mip_level_count: texture.desc.mip_levels,
            sample_count: texture.desc.samples.as_raw(),
            dimension: match texture.desc.img_type {
                marpii::resources::ImageType::Tex1d => wgpu::TextureDimension::D1,
                marpii::resources::ImageType::Tex2d => wgpu::TextureDimension::D2,
                marpii::resources::ImageType::Tex3d => wgpu::TextureDimension::D3,
                marpii::resources::ImageType::Cube => wgpu::TextureDimension::D3,
                other => return Err(MarpiiWgpuError::UnsupportedHalTexture(other)),
            },
            format: conv::map_vk_to_wgpu_texture_format(texture.desc.format)
                .ok_or(MarpiiWgpuError::VkFormatError(texture.desc.format))?,
            usage: conv::map_vk_to_wgpu_texture_usage(texture.desc.usage)
                .ok_or(MarpiiWgpuError::VkImgUsageError(texture.desc.usage))?,
            view_formats: &[],
        };
        let handle = unsafe {
            self.device()
                .create_texture_from_hal::<wgpu::hal::vulkan::Api>(texture.inner, &desc)
        };

        Ok(handle)
    }
    */
}

impl Drop for WgpuCtx {
    fn drop(&mut self) {
        unsafe { self.marpii_device.inner.device_wait_idle() }.unwrap();
    }
}
