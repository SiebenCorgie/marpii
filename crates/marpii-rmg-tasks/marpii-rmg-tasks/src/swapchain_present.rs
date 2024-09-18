use marpii::{
    ash::vk,
    resources::ImgDesc,
    surface::Surface,
    swapchain::{Swapchain, SwapchainImage},
    MarpiiError, OoS,
};
use marpii_rmg::{ImageHandle, RecordError, ResourceError, Rmg, Task};
use std::sync::Arc;

use crate::RmgTaskError;

enum PresentOp {
    None,
    Scheduled(ImageHandle),
    Paired {
        src_image: ImageHandle,
        sw_image: SwapchainImage,
    },
    InFilght(SwapchainImage),
}

impl PresentOp {
    fn take(&mut self) -> PresentOp {
        let mut ret = PresentOp::None;
        std::mem::swap(self, &mut ret);
        ret
    }
}

///Task that handles a swapchain as well as present operation. Lets you blit any image to the swapchain.
pub struct SwapchainPresent {
    swapchain: Swapchain,
    last_known_extent: vk::Extent2D,
    ///the image that is going to be presented next.
    //TODO: Do we want to implement more fancy double buffering? Usually done by the driver tho.
    next: PresentOp,
}

impl SwapchainPresent {
    pub fn new(rmg: &mut Rmg, surface: OoS<Surface>) -> Result<Self, RmgTaskError> {
        //Check for the creation extent.
        let create_extent = surface
            .get_current_extent(&rmg.ctx.device.physical_device)
            .unwrap_or({
                #[cfg(feature = "logging")]
                log::error!("Could not get initial swapchain extent, falling back to 800x600");
                vk::Extent2D {
                    width: 800,
                    height: 600,
                }
            });

        let swapchain = Swapchain::builder(&rmg.ctx.device, surface)?
            .with(move |b| {
                //try to use the highest bit format format
                let mut best_format = b.format_preference.remove(0);
                for next_format in b.format_preference.iter() {
                    if marpii::util::byte_per_pixel(next_format.format).unwrap_or(8)
                        > marpii::util::byte_per_pixel(best_format.format).unwrap_or(8)
                    {
                        best_format = next_format.clone();
                    }
                }
                b.format_preference = vec![best_format];
                //Flag for color attachment and transfer dst, which are mostly used to interact with the image
                b.create_info.usage =
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST;

                b.extent_preference = Some(create_extent)
            })
            .build()
            .map_err(|e| MarpiiError::from(e))?;

        Ok(SwapchainPresent {
            swapchain,
            last_known_extent: vk::Extent2D {
                width: 1,
                height: 1,
            },
            next: PresentOp::None,
        })
    }

    ///Pushes `image` to be presented whenever the task is scheduled next.
    /// Might overwrite any inflight or already pushed frames that are waiting for execution.
    ///
    /// `extent` should be the framebuffer extent or something smaller. Either retrieve from
    /// [Self::extent] or from your windowing
    /// library.
    pub fn push_image(&mut self, image: ImageHandle, extent: vk::Extent2D) {
        self.last_known_extent = extent;
        self.next = PresentOp::Scheduled(image);
    }

    ///Returns the current surface extent, or, if that can't be acquired, nothing.
    /// The latter can happen on some platforms. In that case it is easiest to check the window provider.
    pub fn extent(&self) -> Option<vk::Extent2D> {
        self.swapchain
            .surface
            .get_current_extent(&self.swapchain.device.physical_device)
    }

    ///Returns the swapchain image's format.
    pub fn format(&self) -> vk::Format {
        self.swapchain.images[0].desc.format
    }

    ///Returns the description all current swapchain images are created with.
    pub fn image_desc(&self) -> &ImgDesc {
        &self.swapchain.images[0].desc
    }

    fn recreate(&mut self, surface_extent: vk::Extent2D) -> Result<(), RmgTaskError> {
        self.swapchain
            .recreate(surface_extent)
            .map_err(|e| MarpiiError::from(e))?;
        self.last_known_extent = vk::Extent2D {
            width: self.swapchain.images[0].desc.extent.width,
            height: self.swapchain.images[0].desc.extent.height,
        };

        Ok(())
    }

    fn next_image(&mut self) -> Result<SwapchainImage, ResourceError> {
        let surface_extent = self.extent().unwrap_or(self.last_known_extent);
        if self.swapchain.images[0].extent_2d() != surface_extent {
            #[cfg(feature = "logging")]
            log::info!("Recreating swapchain with extent {:?}!", surface_extent);

            self.recreate(surface_extent)
                .map_err(|_e| ResourceError::SwapchainError)?;
        }

        if let Ok(img) = self.swapchain.acquire_next_image() {
            Ok(img)
        } else {
            Err(ResourceError::SwapchainError)
        }
    }

    fn present_image(&mut self, image: SwapchainImage) {
        let queue = self
            .swapchain
            .device
            .first_queue_for_attribute(true, false, false)
            .unwrap(); //FIXME use track instead
        #[allow(unused_variables)]
        if let Err(e) = self.swapchain.present_image(image, &queue.inner()) {
            #[cfg(feature = "logging")]
            log::error!("present failed with: {}, recreating swapchain", e);
        }
    }
}

impl Task for SwapchainPresent {
    fn name(&self) -> &'static str {
        "SwapchainPresent"
    }

    fn pre_record(
        &mut self,
        _resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        //fetches a new swapchain image if we have a blit image scheduled
        if let PresentOp::Scheduled(to_blit) = self.next.take() {
            let img = self.next_image()?;
            self.next = PresentOp::Paired {
                src_image: to_blit,
                sw_image: img,
            };
        } else {
            #[cfg(feature = "logging")]
            log::warn!("Swapchain queued, but no present image set.");
        }

        Ok(())
    }

    fn post_execution(
        &mut self,
        _resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), RecordError> {
        //schedule image for present if there is any
        if let PresentOp::InFilght(swimg) = self.next.take() {
            self.present_image(swimg);
            //reset state
            self.next = PresentOp::None;
        } else {
            #[cfg(feature = "logging")]
            log::warn!("Swapchain queued, but no image infilght after execution.");
        }

        Ok(())
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        match &self.next {
            PresentOp::InFilght(_) => {
                #[cfg(feature = "logging")]
                log::warn!("Got inflight swapchain image at register phase...");
            }
            PresentOp::Paired {
                src_image,
                sw_image,
            } => {
                registry
                    .request_image(
                        src_image,
                        vk::PipelineStageFlags2::TRANSFER,
                        vk::AccessFlags2::TRANSFER_READ,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    )
                    .unwrap();
                registry.register_binary_signal_semaphore(sw_image.sem_present.clone());
                registry.register_binary_wait_semaphore(sw_image.sem_acquire.clone());
                registry.register_asset(sw_image.image.clone());
            }
            PresentOp::Scheduled(_) => {
                #[cfg(feature = "logging")]
                log::warn!("Got scheduled image at register phase. Do nothing and present next.");
            }
            PresentOp::None => {}
        }
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS
    }

    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        if let PresentOp::Paired {
            src_image,
            sw_image,
        } = self.next.take()
        {
            //NOTE: This is a little dirty, but okay since only like that for the swapchain.
            //      We take care of the swapchain image our self. We always transition from "undefined",
            //      since we don't care for the old value.
            //
            //      We transition to blit-receive, schedule the receive, than transition to "present".

            //to transfer-recv
            unsafe {
                device.inner.cmd_pipeline_barrier2(
                    *command_buffer,
                    &vk::DependencyInfo::default().image_memory_barriers(&[
                        //swapchain image transition. Don't keep data
                        vk::ImageMemoryBarrier2::default()
                            .image(sw_image.image.inner)
                            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            //.src_access_mask(vk::AccessFlags2::NONE)
                            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                            .subresource_range(sw_image.image.subresource_all())
                            .old_layout(vk::ImageLayout::UNDEFINED)
                            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL),
                    ]),
                );
            }

            //copy over
            let img = resources.get_image_state(&src_image);
            let src_region = img.image.image_region();
            let dst_region = sw_image.image.image_region();

            let filter = if src_image.image_desc().is_depth_stencil() {
                vk::Filter::NEAREST
            } else {
                vk::Filter::LINEAR
            };

            unsafe {
                device.inner.cmd_blit_image2(
                    *command_buffer,
                    &vk::BlitImageInfo2::default()
                        .src_image(img.image.inner)
                        .dst_image(sw_image.image.inner)
                        .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .filter(filter)
                        .regions(&[vk::ImageBlit2::default()
                            .src_subresource(img.image.subresource_layers_all())
                            .dst_subresource(sw_image.image.subresource_layers_all())
                            .src_offsets(src_region.to_blit_offsets())
                            .dst_offsets(dst_region.to_blit_offsets())]),
                );
            }

            //Move swapchain image to present
            unsafe {
                device.inner.cmd_pipeline_barrier2(
                    *command_buffer,
                    &vk::DependencyInfo::default().image_memory_barriers(&[
                        //swapchain image
                        vk::ImageMemoryBarrier2::default()
                            .image(sw_image.image.inner)
                            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                            .dst_access_mask(vk::AccessFlags2::empty())
                            .subresource_range(sw_image.image.subresource_all())
                            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR),
                    ]),
                );
            }

            //change state again
            self.next = PresentOp::InFilght(sw_image);
        } else {
            #[cfg(feature = "logging")]
            log::warn!("Present scheduled, but no swapchain image present on record.");
        }
    }
}
