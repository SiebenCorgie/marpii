use crate::{BufferState, ImageState, StBuffer, StImage};
use marpii::{
    allocator::{Allocator, MemoryUsage},
    ash::vk::{self, BufferImageCopy},
    context::Device,
    resources::{BufDesc, Buffer, Image, ImgDesc, SharingMode},
};
use std::sync::{Arc, Mutex};

use super::{buffer_upload::UploadPassError, AssumedState, Pass};

///Simple pass that creates a image from data that is then uploaded whenever the pass is submitted
/// to a graph.
///
/// Note that until the first submission the image is invalid. After the first submission the image will be valid.
/// Any additional submissions will not change the image.
///
/// The pass should be used for convenience if a single image is uploaded. For multiple images use [UploadBufferChunk](https://siebencorgie.rs/todo)
/// For a dynamically changing buffer use [DynamicImagePass](crate::pass::DynamicImagePass).
pub struct ImageUploadPass {
    ///Image reference. Note that the image is undefined until first submission
    pub image: StImage,
    assumed_states: [AssumedState; 2],
    //Staging buffer used for upload once. Is cleared afterwards
    staging: Option<StBuffer>,
}

impl ImageUploadPass {
    ///Creates the pass that uploads `image_data` to the resulting image. Note that the MemoryUsage will always be GpuOnly.
    ///
    pub fn new<A: Allocator + Send + Sync + 'static>(
        device: &Arc<Device>,
        allocator: &Arc<Mutex<A>>,
        data: &[u8],
        mut img_desc: ImgDesc,
        name: Option<&str>,
        create_flags: Option<vk::ImageCreateFlags>,
    ) -> Result<Self, UploadPassError> {
        //FIXME: check if we need to overallocate, depending on T's alignment...
        let size = core::mem::size_of::<u8>() * data.len();

        //Add transfer_dst to target image
        img_desc.usage |= vk::ImageUsageFlags::TRANSFER_DST;

        let target_image = StImage::unitialized(Image::new(
            device,
            allocator,
            img_desc,
            MemoryUsage::GpuOnly,
            name,
            create_flags,
        )?);

        let mut staging_buffer = Buffer::new(
            device,
            allocator,
            BufDesc {
                sharing: SharingMode::Exclusive,
                size: size as u64,
                usage: vk::BufferUsageFlags::TRANSFER_SRC,
            },
            MemoryUsage::CpuToGpu,
            None,
            None,
        )?;

        //write data for upload and flush
        staging_buffer.write(0, data)?;
        staging_buffer.flush_range();

        let staging_buffer = StBuffer::unitialized(staging_buffer);

        Ok(ImageUploadPass {
            image: target_image.clone(),
            assumed_states: [
                AssumedState::Image {
                    image: target_image,
                    state: ImageState {
                        access_mask: vk::AccessFlags::TRANSFER_WRITE,
                        layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    },
                },
                AssumedState::Buffer {
                    buffer: staging_buffer.clone(),
                    state: BufferState {
                        access_mask: vk::AccessFlags::TRANSFER_READ,
                    },
                },
            ],
            staging: Some(staging_buffer),
        })
    }

    ///Returns true if the upload has been scheduled. Note that this does not necessarly mean that the buffer is valid.
    /// This is only the case after the scheduled upload has finished executing.
    pub fn is_uploaded(&self) -> bool {
        self.staging.is_none()
    }
}

impl Pass for ImageUploadPass {
    fn assumed_states(&self) -> &[super::AssumedState] {
        if self.staging.is_some() {
            &self.assumed_states
        } else {
            &[] //in the case of an uploaded buffer, nothing happens
        }
    }

    fn record(
        &mut self,
        command_buffer: &mut marpii_commands::Recorder,
    ) -> Result<(), anyhow::Error> {
        if let Some(staging) = self.staging.take() {
            #[cfg(feature = "logging")]
            log::info!("Scheduling image upload!");

            let dst_image = self.image.clone();
            let image_extent = dst_image.image().extent_3d();
            let image_subresource = dst_image.image().subresource_layers_all();
            command_buffer.record(move |device, cmd| unsafe {
                device.cmd_copy_buffer_to_image(
                    *cmd,
                    staging.buffer().inner,
                    dst_image.image().inner,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[BufferImageCopy {
                        buffer_offset: 0,
                        buffer_row_length: 0,
                        buffer_image_height: 0, //always copying tightly packed.
                        image_extent,
                        image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                        image_subresource,
                    }],
                );
            });
        }

        Ok(())
    }

    fn requirements(&self) -> &'static [super::SubPassRequirement] {
        &[super::SubPassRequirement::TransferBit]
    }
}
