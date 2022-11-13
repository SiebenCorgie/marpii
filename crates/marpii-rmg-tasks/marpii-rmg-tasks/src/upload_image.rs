use marpii::{
    ash::vk,
    resources::{Buffer, ImgDesc},
};
use marpii_rmg::{BufferHandle, ImageHandle, ResourceRegistry, Resources, Rmg, RmgError, Task};
use std::sync::Arc;

///Transfer pass that copies data to an image on the GPU.
/// perfect if you need to initialise textures for instance.
pub struct UploadImage {
    pub image: ImageHandle,
    upload: BufferHandle<u8>,
}

impl UploadImage {
    //TODO: add tasks constructors, for instance automatic "load from file"?

    ///Creates the upload task. Note that data is interpreted as whatever `target`'s format is.
    /// If this is wrong you will get artefacts. Use a format convertion before (on CPU), or a chained GPU based
    /// convertion task otherwise.
    pub fn new_with_image<'dta>(
        rmg: &mut Rmg,
        data: &'dta [u8],
        mut desc: ImgDesc,
    ) -> Result<Self, RmgError> {
        let staging = Buffer::new_staging_for_data(
            &rmg.ctx.device,
            &rmg.ctx.allocator,
            Some("StagingBuffer"),
            data,
        )?;

        staging.flush_range().map_err(|e| {
            #[cfg(feature = "logging")]
            log::error!("Flushing upload image failed: {}", e);
            RmgError::Any(anyhow::anyhow!("Flushing upload image failed"))
        })?;

        if !desc.usage.contains(vk::ImageUsageFlags::TRANSFER_DST) {
            #[cfg(feature = "logging")]
            log::warn!("Upload image had TRANSEFER_DST not set, adding to usage...");
            desc.usage |= vk::ImageUsageFlags::TRANSFER_DST;
        }

        let staging = rmg.import_buffer(Arc::new(staging), None, None)?;
        let image = rmg.new_image_uninitialized(desc, None)?;

        Ok(UploadImage {
            image,
            upload: staging,
        })
    }
}

impl Task for UploadImage {
    fn name(&self) -> &'static str {
        "UploadImage"
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
    fn register(&self, registry: &mut ResourceRegistry) {
        registry.request_image(&self.image);
        registry.request_buffer(&self.upload);
    }
    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        let buffer = resources.get_buffer_state(&self.upload);
        let img = resources.get_image_state(&self.image);

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
