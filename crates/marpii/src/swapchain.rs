use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use ash::vk::{self, SwapchainCreateInfoKHR};
use oos::OoS;

use crate::{
    allocator::{ManagedAllocation, MemoryUsage, UnmanagedAllocation, UnmanagedAllocator},
    context::Device,
    error::DeviceError,
    resources::{Image, ImgDesc, SharingMode},
    surface::Surface,
    sync::BinarySemaphore,
    MarpiiError,
};

///All info needed to create a swapchain
pub struct RecreateInfo {
    pub format: ash::vk::SurfaceFormatKHR,
    pub present_mode: ash::vk::PresentModeKHR,

    pub image_count: u32,

    pub extent: ash::vk::Extent2D,
    pub array_layers: u32,
    pub usage: ash::vk::ImageUsageFlags,
    pub sharing_mode: SharingMode,
    pub transform: ash::vk::SurfaceTransformFlagsKHR,
    pub composite_alpha: ash::vk::CompositeAlphaFlagsKHR,
    pub is_clipped: bool,
}

impl RecreateInfo {
    pub fn apply_on<'a>(
        &'a self,
        mut create_info: SwapchainCreateInfoKHR<'a>,
    ) -> SwapchainCreateInfoKHR<'a> {
        create_info = create_info
            .min_image_count(self.image_count)
            .image_format(self.format.format)
            .image_color_space(self.format.color_space)
            .image_extent(self.extent)
            .image_array_layers(self.array_layers)
            .image_usage(self.usage)
            .pre_transform(self.transform)
            .composite_alpha(self.composite_alpha)
            .present_mode(self.present_mode)
            .clipped(self.is_clipped);

        match &self.sharing_mode {
            SharingMode::Exclusive => {
                create_info = create_info.image_sharing_mode(ash::vk::SharingMode::EXCLUSIVE)
            }
            SharingMode::Concurrent {
                queue_family_indices,
            } => {
                create_info = create_info
                    .image_sharing_mode(ash::vk::SharingMode::CONCURRENT)
                    .queue_family_indices(queue_family_indices);
            }
        }

        create_info
    }
}

pub struct SwapchainBuilder {
    ///Surface based on which the swapchain will be build.
    pub surface: OoS<Surface>,
    ///Device for which the swapchain will be build.
    pub device: Arc<Device>,
    pub create_info: RecreateInfo,

    pub format_preference: Vec<ash::vk::SurfaceFormatKHR>,
    pub present_mode_preference: Vec<ash::vk::PresentModeKHR>,
    pub extent_preference: Option<vk::Extent2D>,
}

impl SwapchainBuilder {
    pub fn build(self) -> Result<Swapchain, DeviceError> {
        if self.create_info.extent.width == 0 || self.create_info.extent.height == 0 {
            return Err(DeviceError::InvalidSwapchainSize(self.create_info.extent));
        }

        let sharing_mode = self.create_info.sharing_mode.clone();

        let create_info = self.as_swapchain_create_info();
        let swapchain_loader =
            ash::khr::swapchain::Device::new(&self.device.instance.inner, &self.device.inner);
        let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None)? };

        //at this point we got the swapchain. The swapchain is managing its images so we have to create the images without an allocator attachment.
        let images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? }
            .into_iter()
            .map(|swimage| {
                Arc::new(Image {
                    allocation: Box::new(ManagedAllocation {
                        allocation: Some(UnmanagedAllocation {
                            hidden: PhantomData,
                        }),
                        allocator: Arc::new(Mutex::new(UnmanagedAllocator)),
                        device: self.device.clone(),
                    }),
                    desc: ImgDesc {
                        extent: ash::vk::Extent3D {
                            width: create_info.image_extent.width,
                            height: create_info.image_extent.height,
                            depth: 1,
                        },
                        format: create_info.image_format,
                        img_type: crate::resources::ImageType::Tex2d,
                        mip_levels: 1,
                        samples: ash::vk::SampleCountFlags::TYPE_1,
                        sharing_mode: sharing_mode.clone(),
                        tiling: ash::vk::ImageTiling::OPTIMAL,
                        usage: self.create_info.usage,
                        ..Default::default()
                    },
                    usage: MemoryUsage::GpuOnly, //FIXME: maybe incorrect... might depend on the implementation?
                    inner: swimage,
                    device: self.device.clone(),
                    do_not_destroy: true,
                })
            })
            .collect::<Vec<_>>();

        //create semaphore buffers and setup the roundtrip state for the semaphore buffers
        let acquire_semaphore = (0..images.len())
            .map(|_| Arc::new(BinarySemaphore::new(&self.device).unwrap()))
            .collect();
        let render_finished_semaphore = (0..images.len())
            .map(|_| Arc::new(BinarySemaphore::new(&self.device).unwrap()))
            .collect();

        //Update recreate info with the stuff we currently use.

        let format = self.get_first_supported_format();
        let present_mode = self.get_first_supported_present_mode();
        let mut recreate_info = self.create_info;
        recreate_info.format = format;
        recreate_info.present_mode = present_mode;

        Ok(Swapchain {
            surface: self.surface,
            images,
            device: self.device,
            acquire_semaphore,
            render_finished_semaphore,
            next_semaphore: 0,
            loader: swapchain_loader,
            swapchain,
            recreate_info,
        })
    }

    pub fn get_first_supported_format(&self) -> ash::vk::SurfaceFormatKHR {
        let mut supported = self
            .surface
            .get_formats(self.device.physical_device)
            .unwrap();
        for prefered in self.format_preference.iter() {
            if supported.contains(prefered) {
                return *prefered;
            }
        }

        //if we came till here non of the supported formats where available, therefore just take the first one
        supported.remove(0)
    }

    pub fn get_first_supported_present_mode(&self) -> ash::vk::PresentModeKHR {
        let mut supported = self
            .surface
            .get_present_modes(self.device.physical_device)
            .unwrap();
        for prefered in self.present_mode_preference.iter() {
            if supported.contains(prefered) {
                return *prefered;
            }
        }

        //if we came till here non of the supported formats where available, therefore just take the first one
        supported.remove(0)
    }

    pub fn get_supported_image_extent(&self) -> ash::vk::Extent2D {
        let supported = self
            .surface
            .get_capabilities(&self.device.physical_device)
            .unwrap();
        let ext = ash::vk::Extent2D {
            width: supported
                .min_image_extent
                .width
                .max(self.create_info.extent.width)
                .min(supported.max_image_extent.width),
            height: supported
                .min_image_extent
                .height
                .max(self.create_info.extent.height)
                .min(supported.max_image_extent.height),
        };

        if ext.width == u32::MAX || ext.height == u32::MAX {
            #[cfg(feature = "logging")]
            log::warn!("Swapchain extent is u32::MAX on one axis. Should be reduced to the window's size. Extent: {:?}", ext);
        }

        ext
    }

    ///Transforms self into a swapchain create info. Note that the validity of each element is checked agains the capabilities. Therefore, for instance
    /// if no supported format is found in the list of prefered formats, the first supported is chosen.
    pub fn as_swapchain_create_info<'a>(&'a self) -> SwapchainCreateInfoKHR<'a> {
        let format = self.get_first_supported_format();

        let mut builder = SwapchainCreateInfoKHR::default()
            .surface(self.surface.surface)
            .min_image_count(self.create_info.image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(
                self.extent_preference
                    .unwrap_or(self.get_supported_image_extent()),
            )
            .image_array_layers(self.create_info.array_layers)
            .image_usage(self.create_info.usage)
            .pre_transform(self.create_info.transform)
            .composite_alpha(self.create_info.composite_alpha)
            .present_mode(self.get_first_supported_present_mode())
            .clipped(self.create_info.is_clipped);

        match &self.create_info.sharing_mode {
            SharingMode::Exclusive => {
                builder = builder.image_sharing_mode(ash::vk::SharingMode::EXCLUSIVE)
            }
            SharingMode::Concurrent {
                queue_family_indices,
            } => {
                builder = builder
                    .image_sharing_mode(ash::vk::SharingMode::CONCURRENT)
                    .queue_family_indices(queue_family_indices)
            }
        }

        builder
    }

    ///Tries to order present mode preferences to use FIFO_RELAXED or FIFO
    pub fn with_vsync(mut self) -> Self {
        if let Ok(at) = self
            .present_mode_preference
            .binary_search(&ash::vk::PresentModeKHR::FIFO_RELAXED)
        {
            let e = self.present_mode_preference.remove(at);
            self.present_mode_preference.insert(0, e);
        }

        self
    }

    ///Tries to prefere immediate presentation.
    pub fn with_immediate_present(mut self) -> Self {
        if let Ok(at) = self
            .present_mode_preference
            .binary_search(&ash::vk::PresentModeKHR::IMMEDIATE)
        {
            let e = self.present_mode_preference.remove(at);
            self.present_mode_preference.insert(0, e);
        }

        self
    }

    pub fn with_extent(mut self, extent: vk::Extent2D) -> Self {
        self.extent_preference = Some(extent);
        self
    }

    ///enables you to chain multiple assignments to a constructed builder. For instance
    ///
    ///```ignore
    /// builder.with(|b| b.usage = ash::vk::ImageUsageFlags::COLOR_ATTACHMENT)
    ///    .with(|b| b.format = ...)
    ///    .build(..)
    ///```
    pub fn with<FILTER>(mut self, mut filter: FILTER) -> Self
    where
        FILTER: FnMut(&mut Self) + 'static,
    {
        filter(&mut self);
        self
    }
}

///Wrapper around the swapchains `image` that keeps track of needed primitives.
pub struct SwapchainImage {
    ///The actual image, managed by the swapchain implementation (the reason for the "UnmanagedAllocator").
    pub image: Arc<crate::resources::Image>,
    ///Index identifying the image when presenting
    pub index: u32,
    ///Semaphore used for image acquire operatons. Will usually be used as a dependency for swapchain accessing
    /// operations, or the swapchain present operation.
    ///
    ///
    /// Note that this is **NOT** a TimelineSemaphore. Therefor giving it any value when submitting to a queue won't have
    /// any effect.
    pub sem_acquire: Arc<BinarySemaphore>,
    ///Semaphore that is signalled when this image is ready for present. Should be signalled by the commandbuffer
    /// that is writing to the image.
    ///
    ///
    /// Note that this is **NOT** a TimelineSemaphore. Therefor giving it any value when submitting to a queue won't have
    /// any effect.
    pub sem_present: Arc<BinarySemaphore>,
}

pub struct Swapchain {
    pub loader: ash::khr::swapchain::Device,
    pub swapchain: ash::vk::SwapchainKHR,

    pub device: Arc<Device>,

    ///assosiated surface. Needed to keep the surface alive until the swapchain is dropped.
    pub surface: OoS<Surface>,

    ///all images of the swapchain.
    ///
    /// # Safety
    ///
    /// Don't reorder those fields if you are using the [acquire_next_image](Swapchain::acquire_next_image) function.
    /// It depends on the correct ordering (and size) of this field.
    pub images: Vec<Arc<crate::resources::Image>>,
    ///acquire semaphores
    //NOTE: They are hidden, since those are fully managed by this struct. Changing anything would break assumptions.
    // Not that those are binary semaphores, since this is defined for the present function.
    acquire_semaphore: Vec<Arc<BinarySemaphore>>,
    render_finished_semaphore: Vec<Arc<BinarySemaphore>>,
    ///present_finished semaphores
    next_semaphore: usize,
    recreate_info: RecreateInfo,
}

impl Swapchain {
    ///Creates a new swapchain builder where all fields are set either with defaults, or data obtained from surface capabilities.
    ///Most notably the format preferences are filled with all available formats, same with the present modes, and the extent is set to the current maximum.
    ///
    /// # Note on Wayland
    /// It can happen that the surfaces "supported" extend is `u32::MAX` on all axis. In that case you'll have to manually set the the correct extent.
    pub fn builder(
        device: &Arc<Device>,
        surface: OoS<Surface>,
    ) -> Result<SwapchainBuilder, MarpiiError> {
        let formats = surface.get_formats(device.physical_device)?;
        let capabilities = surface.get_capabilities(&device.physical_device)?;
        let present_modes = surface.get_present_modes(device.physical_device)?;

        let format = formats[0];
        let present_mode = present_modes[0];

        Ok(SwapchainBuilder {
            surface,
            device: device.clone(),
            format_preference: formats,
            present_mode_preference: present_modes,
            extent_preference: None,
            create_info: RecreateInfo {
                format, //Get later set
                present_mode,
                image_count: capabilities
                    .max_image_count
                    .min(3)
                    .max(capabilities.min_image_count), //default to trippel-buffering if possible
                extent: capabilities.current_extent,
                array_layers: 1,
                usage: capabilities.supported_usage_flags,
                sharing_mode: SharingMode::Exclusive,
                transform: if capabilities
                    .supported_transforms
                    .contains(ash::vk::SurfaceTransformFlagsKHR::IDENTITY)
                {
                    ash::vk::SurfaceTransformFlagsKHR::IDENTITY
                } else {
                    capabilities.current_transform
                },
                composite_alpha: ash::vk::CompositeAlphaFlagsKHR::OPAQUE,
                is_clipped: false,
            },
        })
    }

    ///Retrieves the next image that should be written to. Note that all required information (acquire semaphore and)
    /// a semaphore to be signalled when finished presenting is included in that image.
    pub fn acquire_next_image(&mut self) -> Result<SwapchainImage, DeviceError> {
        //find right semaphores
        let acquire_semaphore = self.acquire_semaphore[self.next_semaphore].clone();
        let present_semaphore = self.render_finished_semaphore[self.next_semaphore].clone();
        self.next_semaphore = (self.next_semaphore + 1) % self.acquire_semaphore.len();

        let (index, is_suboptimal) = unsafe {
            self.loader.acquire_next_image(
                self.swapchain,
                core::u64::MAX,
                acquire_semaphore.inner,
                ash::vk::Fence::null(),
            )?
        };

        if is_suboptimal {
            #[cfg(feature = "logging")]
            log::warn!("Acquired image is suboptimal!");
        }

        let image = self.images[index as usize].clone();

        //Setup image description with correct semaphores and image pointer.
        Ok(SwapchainImage {
            image,
            index,
            sem_acquire: acquire_semaphore,
            sem_present: present_semaphore,
        })
    }

    ///Recreates the swapchain with the same settings it was created from.
    //FIXME: This is not safe if the recreation failed. In that case Swapchain is partialy "new"
    //       Should not overwrite self's fields until recreation succeeded.
    pub fn recreate(&mut self, extent: ash::vk::Extent2D) -> Result<(), DeviceError> {
        let device = self.images[0].device.clone();

        //overwrite extent
        self.recreate_info.extent = extent;

        //Setup recreate info
        let mut recreateinfo = ash::vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface.surface)
            .old_swapchain(self.swapchain)
            .image_extent(extent);

        recreateinfo = self.recreate_info.apply_on(recreateinfo);

        //create new swapchain and change self's sc
        let new_sc = unsafe { self.loader.create_swapchain(&recreateinfo, None)? };
        //destroy old chain
        unsafe { self.loader.destroy_swapchain(self.swapchain, None) };

        //overwrite swapchain ptr
        self.swapchain = new_sc;

        //Now overwrite inner swapchain images with new ones. The old ones should be dropped once
        //they have no references anymore.
        self.images = unsafe { self.loader.get_swapchain_images(self.swapchain)? }
            .into_iter()
            .map(|img| {
                Arc::new(Image {
                    allocation: Box::new(ManagedAllocation {
                        allocation: Some(UnmanagedAllocation {
                            hidden: PhantomData,
                        }),
                        allocator: Arc::new(Mutex::new(UnmanagedAllocator)),
                        device: device.clone(),
                    }),
                    desc: ImgDesc {
                        extent: ash::vk::Extent3D {
                            width: self.recreate_info.extent.width,
                            height: self.recreate_info.extent.height,
                            depth: 1,
                        },
                        format: self.recreate_info.format.format,
                        img_type: crate::resources::ImageType::Tex2d,
                        mip_levels: 1,
                        samples: ash::vk::SampleCountFlags::TYPE_1,
                        sharing_mode: self.recreate_info.sharing_mode.clone(),
                        tiling: ash::vk::ImageTiling::OPTIMAL,
                        usage: self.recreate_info.usage,
                        ..Default::default()
                    },
                    inner: img,
                    usage: MemoryUsage::GpuOnly,
                    device: device.clone(),
                    do_not_destroy: true,
                })
            })
            .collect();

        //add semaphores if needed
        while self.acquire_semaphore.len() < self.images.len() {
            self.acquire_semaphore
                .push(Arc::new(BinarySemaphore::new(&device)?));
        }

        while self.render_finished_semaphore.len() < self.images.len() {
            self.render_finished_semaphore
                .push(Arc::new(BinarySemaphore::new(&device)?));
        }

        #[cfg(feature = "logging")]
        log::info!("Recreating swapchain for {:?}", extent);

        Ok(())
    }

    ///Will enqueue a present command for `image`. It will wait for `image.sem_present`. An error is returned
    /// if the swapchain failed to present the image for some reason. Ususally this means that either the surface size
    /// has changed, or that the window's surface is lost.
    pub fn present_image(
        &self,
        image: SwapchainImage,
        queue: &ash::vk::Queue,
    ) -> ash::prelude::VkResult<()> {
        let present_info = ash::vk::PresentInfoKHR::default()
            .swapchains(core::slice::from_ref(&self.swapchain))
            .image_indices(core::slice::from_ref(&image.index))
            .wait_semaphores(core::slice::from_ref(&image.sem_present.inner));

        match unsafe { self.loader.queue_present(*queue, &present_info) } {
            Ok(b) => {
                if b {
                    #[cfg(feature = "logging")]
                    log::warn!("Suboptimal image on present. returning error");
                    Err(ash::vk::Result::SUBOPTIMAL_KHR)
                } else {
                    Ok(()) //all is right
                }
            }
            Err(e) => {
                #[cfg(feature = "logging")]
                log::error!("Error while presenting image: {}", e);
                Err(e)
            }
        }
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            self.loader.destroy_swapchain(self.swapchain, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_impl_all;

    #[test]
    fn impl_send_sync() {
        assert_impl_all!(Arc<Swapchain>: Send, Sync);
    }
}
