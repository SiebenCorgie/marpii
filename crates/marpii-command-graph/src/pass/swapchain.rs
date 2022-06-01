use std::sync::Arc;

use marpii::{
    ash::vk::{self, Extent2D},
    context::Device,
    surface::Surface,
    swapchain::{Swapchain, SwapchainImage},
    sync::Semaphore,
};
use marpii_commands::Recorder;

use crate::{ImageState, StImage};

use super::{AssumedState, Pass};

///Handles whole swapchain operation by waiting for the swapchain's availablity, and sheduling the present
/// operation.
///
/// The next to be used swapchain image is `next_image`. You can use the image for instance as attachment to another
/// render pass, of as target for a blit operation.
///
/// Note that the `next_image` changes on a per frame basis. You therefore might have to change the target of your blit operation per frame.
/// or you use the `next_image_index` function to find the actual image index, which can be used to identify the image on the
/// swapchain.
///
///Also note that the swapchain will always be presented on one of the graphics enabled queues. If there are none present is not done at all.
pub struct SwapchainPresent {
    ///SwapchainImage with state.
    next_st_image: StImage,
    next_image: Option<SwapchainImage>,
    assumed: AssumedState,
    swapchain: Swapchain,
    device: Arc<Device>,
}

impl SwapchainPresent {
    pub fn new(device: &Arc<Device>, surface: Arc<Surface>) -> Result<Self, anyhow::Error> {
        //NOTE: we add the standard usages atm.
        //TODO: Let user decide parts of the swapchain creation?
        let mut swapchain = Swapchain::builder(device, &surface)?
            .with(|b| {
                b.create_info.usage =
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST
            })
            .build()?;

        //acquire first image and setup st image
        let next = swapchain.acquire_next_image()?;
        let st_next = StImage::shared(
            next.image.clone(),
            device
                .first_queue_for_attribute(true, false, false)
                .unwrap()
                .family_index,
            vk::AccessFlags::empty(),
            vk::ImageLayout::UNDEFINED,
        );

        let assumed = AssumedState::Image {
            image: st_next.clone(),
            state: ImageState {
                access_mask: vk::AccessFlags::empty(),
                layout: vk::ImageLayout::PRESENT_SRC_KHR,
            },
        };

        Ok(SwapchainPresent {
            next_image: Some(next),
            next_st_image: st_next,
            assumed,
            swapchain,
            device: device.clone(),
        })
    }

    ///Tries to read the current surface extent. This can fail on some platforms (like Linux+Wayland).
    pub fn current_extent(&self) -> Option<Extent2D> {
        let extent = self
            .swapchain
            .surface
            .get_capabilities(self.device.physical_device)
            .unwrap()
            .current_extent;
        //if on wayland this will be wrong, check and maybe return nothing.
        match extent {
            Extent2D {
                width: 0xFFFFFFFF,
                height: 0xFFFFFFFF,
            }
            | Extent2D {
                width: 0,
                height: 0,
            } => None,
            Extent2D { width, height } => Some(Extent2D { width, height }),
        }
    }

    ///Returns the current extent of the swapchain images
    pub fn image_extent(&self) -> Extent2D {
        let mut extent = self.swapchain.images[0].extent_2d();

        //FIXME: Not sure why, but on wayland+Intel this size gets reported on startup, which is wrong.
        if extent
            == (Extent2D {
                width: 0x4_000,
                height: 0x4_000,
            })
        {
            #[cfg(feature = "logging")]
            log::warn!(
                "possibly wrong swapchain extent of {:?}, falling back to 512x512",
                extent
            );

            extent = Extent2D {
                width: 512,
                height: 512,
            };
        }

        extent
    }

    ///Resizes the swapchain and resources.
    pub fn resize(&mut self, extent: Extent2D) {
        if let Err(e) = self.swapchain.recreate(extent) {
            #[cfg(feature = "logging")]
            log::error!(
                "Failed to get recreate swapchain: {:?}. Silently failing...",
                e
            );
        }

        //now acquire next to remove any old references
        self.acquire_next();
    }

    pub fn swapchain(&self) -> &Swapchain {
        &self.swapchain
    }

    fn acquire_next(&mut self) {
        //acquire first image and setup st image
        let next = match self.swapchain.acquire_next_image() {
            Ok(img) => img,
            Err(e) => {
                #[cfg(feature = "logging")]
                log::error!(
                    "Failed to get next swapchain image: {:?}, try recreate...",
                    e
                );
                //Try recreation, otherwise panic
                self.resize(self.current_extent().unwrap_or(Extent2D {
                    width: 1,
                    height: 1,
                }));

                self.swapchain.acquire_next_image().unwrap()
            }
        };
        let st_next = StImage::shared(
            next.image.clone(),
            self.device
                .first_queue_for_attribute(true, false, false)
                .unwrap()
                .family_index,
            vk::AccessFlags::empty(),
            vk::ImageLayout::UNDEFINED,
        );

        //Overwrite
        self.next_image = Some(next);
        self.next_st_image = st_next;
        self.assumed = AssumedState::Image {
            image: self.next_st_image.clone(),
            state: ImageState {
                access_mask: vk::AccessFlags::empty(),
                layout: vk::ImageLayout::PRESENT_SRC_KHR,
            },
        };
    }

    ///Returns the next to be written swapchain image.
    pub fn next_image(&self) -> &StImage {
        &self.next_st_image
    }

    ///Returns the index of the next present image.
    pub fn next_index(&self) -> usize {
        self.next_image.as_ref().map(|img| img.index).unwrap_or(0) as usize
    }
}
impl Pass for SwapchainPresent {
    fn assumed_states(&self) -> &[AssumedState] {
        core::slice::from_ref(&self.assumed)
    }
    fn record(&mut self, _command_buffer: &mut Recorder) -> Result<(), anyhow::Error> {
        //Record is handled by assuming that the image is already in present state. In that case we can just
        //submit the present on the swapchain and then swap the images to the next one.

        Ok(())
    }
    fn waits_for_external(&self) -> &[Arc<Semaphore>] {
        if let Some(img) = &self.next_image {
            core::slice::from_ref(&img.sem_acquire)
        } else {
            &[]
        }
    }
    fn signals_external(&self) -> &[Arc<Semaphore>] {
        if let Some(img) = &self.next_image {
            core::slice::from_ref(&img.sem_present)
        } else {
            &[]
        }
    }

    fn post_action(&mut self) {
        //TODO: There is technically a soundness problem. A pass could for instance blit to the swapchain
        //before the swapchain finished (as declared by Self::wait_for_external).
        // However, in practice there are two things preventing.
        //1. This is usaly on the same segment in the graph, therefore the wait semaphore
        //   will be handled before the blit pass
        //2. Usually a renderer has to wait for all its frame-dependend resources anyways. Those however become available
        //   *after* the swapchain has finished present. Therefore this is okay as well.

        if let (Some(present_queue), Some(swimage)) = (
            self.device.first_queue_for_attribute(true, false, false),
            self.next_image.take(),
        ) {
            if let Err(e) = self
                .swapchain
                .present_image(swimage, &present_queue.inner())
            {
                #[cfg(feature = "logging")]
                log::error!("Failed to present: {}", e);
            }
        } else {
            #[cfg(feature = "logging")]
            log::error!("Failed to find graphics queue for present");
        }

        //finally swap to next image
        self.acquire_next();
    }
}

///Handles handles transition of the `image` into a presentable state and signals the `semaphore`.
pub struct SwapchainPrepare {
    #[allow(dead_code)] //TODO might use later ...
    image: StImage,
    assume: [AssumedState; 1],
    signals: Arc<Semaphore>,
}

impl SwapchainPrepare {
    pub fn new(image: StImage, signals: Arc<Semaphore>) -> Self {
        SwapchainPrepare {
            image: image.clone(),
            assume: [AssumedState::Image {
                image,
                state: ImageState {
                    access_mask: vk::AccessFlags::empty(),
                    layout: vk::ImageLayout::PRESENT_SRC_KHR,
                },
            }],
            signals,
        }
    }
}

impl Pass for SwapchainPrepare {
    fn assumed_states(&self) -> &[AssumedState] {
        &self.assume
    }

    ///The actual recording step. Gets provided with access to the actual resources through the
    /// `ResourceManager` as well as the `command_buffer` recorder that is currently in use.
    fn record(&mut self, _command_buffer: &mut Recorder) -> Result<(), anyhow::Error> {
        Ok(()) //doesn't do anything.
    }

    fn signals_external(&self) -> &[Arc<Semaphore>] {
        core::slice::from_ref(&self.signals)
    }
}
