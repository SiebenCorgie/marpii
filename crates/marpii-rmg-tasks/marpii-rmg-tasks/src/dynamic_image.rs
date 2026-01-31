use marpii::{
    DeviceError, MarpiiError,
    ash::vk,
    resources::{Buffer, ImgDesc},
    util::ImageRegion,
};
use marpii_rmg::{ImageHandle, Rmg, RmgError, Task};
use smallvec::SmallVec;
use std::sync::Arc;

use crate::RmgTaskError;

pub struct DynImgCmd {
    region: ImageRegion,
    buffer: Arc<Buffer>,
}

///Helper task that lets you change a image rapidly between frames.
/// This work by recording a set of copy commands for the image to buffers
/// that are then applied whenever the task is executed.
//TODO - use a ring buffer or something for the buffers.
//     - allow image to image copy
pub struct DynamicImage {
    staging_copies: Vec<DynImgCmd>,
    pub image: ImageHandle,
}

impl DynamicImage {
    pub fn new_from_image(image: ImageHandle) -> Result<Self, RmgTaskError> {
        if !image
            .usage_flags()
            .contains(vk::ImageUsageFlags::TRANSFER_DST)
        {
            return Err(MarpiiError::from(DeviceError::ImageExpectUsageFlag(
                vk::ImageUsageFlags::TRANSFER_DST,
            )))?;
        }
        Ok(DynamicImage {
            staging_copies: Vec::with_capacity(1),
            image,
        })
    }

    pub fn new(rmg: &mut Rmg, mut desc: ImgDesc, name: Option<&str>) -> Result<Self, RmgError> {
        desc.usage |= vk::ImageUsageFlags::TRANSFER_DST;
        let img = rmg.new_image_uninitialized(desc, name)?;
        Ok(Self::new_from_image(img).unwrap())
    }

    ///Schedules write of `bytes` to the `region` of the image. All writes are executed in the order they are submitted to the task.
    ///
    ///
    /// The `region` is clamped to the actual
    /// region of the image, which might mess up your data alignment.
    ///
    ///
    /// If `bytes` is bigger then the image `region` the bytes that are too much are omitted. If `bytes` is too small the
    /// parts of `region` might become undefined.
    pub fn write_bytes(
        &mut self,
        rmg: &mut Rmg,
        region: ImageRegion,
        bytes: &[u8],
    ) -> Result<(), RmgError> {
        let buffer = Buffer::new_staging_for_data(
            &rmg.ctx.device,
            &rmg.ctx.allocator,
            Some("DynamicImageSrcBuffer"),
            bytes,
        )
        .map_err(MarpiiError::from)?;
        self.staging_copies.push(DynImgCmd {
            region,
            buffer: Arc::new(buffer),
        });
        Ok(())
    }
}

impl Task for DynamicImage {
    fn name(&self) -> &'static str {
        "DynamicImage"
    }
    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_image(
                &self.image,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            )
            .unwrap();
        for cp in self.staging_copies.iter() {
            registry.register_asset(cp.buffer.clone());
        }
    }

    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        let image_access = resources.get_image_state(&self.image);

        let staging = core::mem::take(&mut self.staging_copies);

        //Move the src buffers to the correct layout state. Those are (not yet)
        // handled by RMG, so we have to do that by hand here.
        let buffer_barriers: SmallVec<[_; 4]> = staging
            .iter()
            .map(|staging| {
                vk::BufferMemoryBarrier2::default()
                    .buffer(staging.buffer.inner)
                    .offset(0)
                    .size(vk::WHOLE_SIZE)
                    .src_access_mask(vk::AccessFlags2::HOST_WRITE)
                    .src_stage_mask(vk::PipelineStageFlags2::HOST)
                    .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                    .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            })
            .collect();

        unsafe {
            device.inner.cmd_pipeline_barrier2(
                *command_buffer,
                &vk::DependencyInfo::default().buffer_memory_barriers(&buffer_barriers),
            );
        }

        for cp in staging {
            let copy_cmd = vk::BufferImageCopy2::default()
                .buffer_image_height(0)
                .buffer_offset(0)
                .buffer_row_length(0)
                .image_extent(cp.region.extent)
                .image_offset(cp.region.offset)
                .image_subresource(image_access.image.subresource_layers_all());
            unsafe {
                device.inner.cmd_copy_buffer_to_image2(
                    *command_buffer,
                    &vk::CopyBufferToImageInfo2::default()
                        .src_buffer(cp.buffer.inner)
                        .regions(&[copy_cmd])
                        .dst_image(image_access.image.inner)
                        .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL),
                );

                device.inner.cmd_pipeline_barrier2(
                    *command_buffer,
                    &vk::DependencyInfo::default().image_memory_barriers(&[
                        vk::ImageMemoryBarrier2::default()
                            .image(image_access.image.inner)
                            .subresource_range(image_access.image.subresource_all())
                            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                            .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER),
                    ]),
                );
            }
        }
    }
}
