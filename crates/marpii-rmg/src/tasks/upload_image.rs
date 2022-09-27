use crate::{AnyResKey, BufferKey, CtxRmg, ImageKey, RecordError, Task};
use marpii::{ash::vk, resources::Buffer};
use std::sync::Arc;

///Transfer pass that copies data to an image on the GPU.
/// perfect if you need to initialise textures for instance.
pub struct UploadImage<'dta> {
    target: ImageKey,
    src: &'dta [u8],
    host_image: Option<BufferKey>,
}

impl<'dta> UploadImage<'dta> {
    ///Creates the upload task. Note that data is interpreted as whatever `target`'s format is.
    /// If this is wrong you will get artefacts. Use a format convertion before (on CPU), or a chained GPU based
    /// convertion task otherwise.
    pub fn new(target: ImageKey, data: &'dta [u8]) -> Self {
        Self {
            target,
            src: data,
            host_image: None,
        }
    }
}

impl<'dta> Task for UploadImage<'dta> {
    fn name(&self) -> &'static str {
        "UploadImage"
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
    fn pre_record(
        &mut self,
        resources: &mut crate::Resources,
        ctx: &CtxRmg,
    ) -> Result<(), RecordError> {
        //create host image
        // TODO: Document that this is not free and should be done as early as possible

        let desc = resources
            .images
            .get(self.target)
            .unwrap()
            .image
            .desc
            .clone();
        if !desc.usage.contains(vk::ImageUsageFlags::TRANSFER_DST) {
            #[cfg(feature = "logging")]
            log::error!("Image used as upload target does not have TRANSFER_DST flag set!");
            return Err(RecordError::Any(anyhow::anyhow!(
                "Upload Image has no transfer bit set"
            )));
        }

        let host_image = Buffer::new_staging_for_data(
            &ctx.device,
            &ctx.allocator,
            Some("StagingBuffer"),
            self.src,
        )?;

        //Add buffer to resource manager
        self.host_image = Some(resources.add_buffer(Arc::new(host_image))?);

        Ok(())
    }
    fn register(&self, registry: &mut crate::ResourceRegistry) {
        registry.request_image(self.target);
        registry.request_buffer(self.host_image.unwrap());
    }
    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &crate::Resources,
    ) {
        if let Some(bufkey) = self.host_image {
            let buffer = resources
                .buffer
                .get(bufkey)
                .ok_or(RecordError::NoSuchResource(AnyResKey::Buffer(bufkey)))
                .unwrap();
            let img = resources
                .images
                .get(self.target)
                .ok_or(RecordError::NoSuchResource(AnyResKey::Image(self.target)))
                .unwrap();

            //copy over by moving to right layout, issue copy and moving back to _old_ layout

            unsafe {
                device.inner.cmd_pipeline_barrier2(
                    *command_buffer,
                    &vk::DependencyInfo::builder()
                        .buffer_memory_barriers(&[*vk::BufferMemoryBarrier2::builder()
                            .buffer(buffer.buffer.inner)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)])
                        .image_memory_barriers(&[*vk::ImageMemoryBarrier2::builder()
                            .image(img.image.inner)
                            .subresource_range(img.image.subresource_all())
                            .old_layout(vk::ImageLayout::UNDEFINED)
                            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)]),
                );

                device.inner.cmd_copy_buffer_to_image2(
                    *command_buffer,
                    &vk::CopyBufferToImageInfo2::builder()
                        .src_buffer(buffer.buffer.inner)
                        .dst_image(img.image.inner)
                        .regions(&[*vk::BufferImageCopy2::builder()
                            .buffer_offset(0)
                            .buffer_row_length(0)
                            .buffer_image_height(0)
                            .image_extent(img.image.desc.extent)
                            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                            .image_subresource(img.image.subresource_layers_all())])
                        .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL),
                );

                device.inner.cmd_pipeline_barrier2(
                    *command_buffer,
                    &vk::DependencyInfo::builder().image_memory_barriers(&[
                        *vk::ImageMemoryBarrier2::builder()
                            .image(img.image.inner)
                            .subresource_range(img.image.subresource_all())
                            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                            .new_layout(img.layout),
                    ]),
                );
            }
        }
    }

    fn post_execution(&mut self, resources: &mut crate::Resources, _ctx: &CtxRmg) -> Result<(), RecordError> {
        //mark for removal
        if let Some(buf) = self.host_image.take() {
            resources.remove_resource(buf)?;
        }

        Ok(())
    }
}
