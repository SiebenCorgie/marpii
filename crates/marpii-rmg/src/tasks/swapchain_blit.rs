use crate::{CtxRmg, ImageKey, RecordError, Task};
use marpii::{ash::vk, swapchain::SwapchainImage};

struct Blit {
    src_image: ImageKey,
    sw_image: Option<SwapchainImage>,
}

///Task that blits the source image `src_image` to the next swapchain image.
pub struct SwapchainBlit {
    next_blit: Option<Blit>,
}

impl SwapchainBlit {
    pub fn new() -> Self {
        SwapchainBlit { next_blit: None }
    }

    pub fn next_blit(&mut self, img: ImageKey) {
        self.next_blit = Some(Blit {
            src_image: img,
            sw_image: None,
        })
    }
}

impl Task for SwapchainBlit {
    fn pre_record(
        &mut self,
        resources: &mut crate::Resources,
        _ctx: &CtxRmg,
    ) -> Result<(), RecordError> {
        if let Some(blit) = &mut self.next_blit {
            blit.sw_image = Some(resources.get_next_swapchain_image().unwrap());

            println!("src_img_hdl = {}", resources.get_resource_handle(blit.src_image)?.index());
        }

        Ok(())
    }

    fn post_execution(&mut self, resources: &mut crate::Resources, _ctx: &CtxRmg) -> Result<(), RecordError> {
        if let Some(mut blit) = self.next_blit.take() {
            if let Some(swimage) = blit.sw_image.take() {
                resources.present_image(swimage);
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "SwapchainBlit"
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS | vk::QueueFlags::TRANSFER
    }
    fn register(&self, registry: &mut crate::ResourceRegistry) {
        if let Some(Blit {
            src_image,
            sw_image: Some(swimage),
        }) = &self.next_blit
        {
            println!("Blitting {:?} to swapchain", src_image);
            registry.request_image(*src_image);
            registry.register_foreign_semaphore(swimage.sem_present.clone())
        } else {
            #[cfg(feature = "logging")]
            if self.next_blit.is_some() {
                log::warn!(
                    "There is a blit operation chained, but no swapchain image was optained!"
                );
            }
        }
    }
    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &crate::Resources,
    ) {
        if let Some(Blit {
            src_image,
            sw_image: Some(swimage),
        }) = &self.next_blit
        {
            //init our swapchain image to transfer-able, and move the src image to transfer
            let (before_access, before_layout, img) = {
                let img_access = resources.images.get(*src_image).unwrap();

                (img_access.mask, img_access.layout, img_access.image.clone())
            };

            unsafe {
                device.inner.cmd_pipeline_barrier2(
                    *command_buffer,
                    &vk::DependencyInfo::builder().image_memory_barriers(&[
                        //src image
                        *vk::ImageMemoryBarrier2::builder()
                            .image(img.inner)
                            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .src_access_mask(before_access)
                            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                            .subresource_range(img.subresource_all())
                            .old_layout(before_layout)
                            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
                        //swapchain image
                        *vk::ImageMemoryBarrier2::builder()
                            .image(swimage.image.inner)
                            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            //.src_access_mask(vk::AccessFlags2::NONE)
                            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                            .subresource_range(swimage.image.subresource_all())
                            .old_layout(vk::ImageLayout::UNDEFINED)
                            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL),
                    ]),
                );
            }

            //copy over
            let src_region = img.image_region();
            let dst_region = swimage.image.image_region();

            unsafe {
                device.inner.cmd_blit_image2(
                    *command_buffer,
                    &vk::BlitImageInfo2::builder()
                        .src_image(img.inner)
                        .dst_image(swimage.image.inner)
                        .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .filter(vk::Filter::LINEAR)
                        .regions(&[*vk::ImageBlit2::builder()
                            .src_subresource(img.subresource_layers_all())
                            .dst_subresource(swimage.image.subresource_layers_all())
                            .src_offsets(src_region.to_blit_offsets())
                            .dst_offsets(dst_region.to_blit_offsets())]),
                );
            }

            //Move swapchain image to present, and src image back to "before" state
            unsafe {
                device.inner.cmd_pipeline_barrier2(
                    *command_buffer,
                    &vk::DependencyInfo::builder().image_memory_barriers(&[
                        //src image
                        *vk::ImageMemoryBarrier2::builder()
                            .image(img.inner)
                            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .src_access_mask(vk::AccessFlags2::TRANSFER_READ)
                            .dst_access_mask(before_access)
                            .subresource_range(img.subresource_all())
                            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                            .new_layout(before_layout),
                        //swapchain image
                        *vk::ImageMemoryBarrier2::builder()
                            .image(swimage.image.inner)
                            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                            .dst_access_mask(vk::AccessFlags2::empty())
                            .subresource_range(swimage.image.subresource_all())
                            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR),
                    ]),
                );
            }
        }
    }
}
