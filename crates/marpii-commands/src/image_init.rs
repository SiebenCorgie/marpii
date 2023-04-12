use marpii::{
    allocator::{Allocator, MemoryUsage},
    ash::vk::{self, BufferImageCopy},
    context::{Device, Queue},
    resources::{Buffer, CommandBufferAllocator, CommandPool, Image, ImgDesc, SharingMode},
    CommandBufferError, MarpiiError, OoS,
};
use std::sync::{Arc, Mutex};

use crate::ManagedCommands;

///Creates a Gpu exclusive image from `data`. Assumes that `data` is in the same format as described in `image_description`.
///
///Returns when the image has finished uploading.
/// Since this can potentially be a long operation you can either use a dedicated
/// uploading pass in a graph if the upload should be scheduled better, or use something like [poll-promise](https://crates.io/crates/poll-promise) to do the upload on another thread.
pub fn image_from_data<A: Allocator + Send + Sync + 'static>(
    device: &Arc<Device>,
    allocator: &Arc<Mutex<A>>,
    upload_queue: &Queue,
    mut description: ImgDesc,
    name: Option<&str>,
    data: &[u8],
) -> Result<Image, MarpiiError> {
    //Upload works by initing a buffer with `data`, then executing a copy command buffer.

    //make sure image usage transfer DST is actiavted
    description.usage |= vk::ImageUsageFlags::TRANSFER_DST;

    let image_extent = description.extent;
    let staging_buffer =
        Buffer::new_staging_for_data(device, allocator, Some("ImageStagingBuffer"), data)?;
    //init image
    let image = Image::new(device, allocator, description, MemoryUsage::GpuOnly, name)?;
    let image_subresource = image.subresource_layers_all();
    //now schedule CB that uploads the image
    let command_pool = OoS::new(CommandPool::new(
        device,
        upload_queue.family_index,
        vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
    )?);
    let command_buffer = command_pool.allocate_buffer(vk::CommandBufferLevel::PRIMARY)?;
    //Now launch command buffer that uploads the data
    let mut cb = ManagedCommands::new(device, command_buffer)?;
    let mut recorder = cb.start_recording()?;

    //NOTE: Lifetime ok since we wait at the end of the function and return the image

    #[cfg(feature = "logging")]
    log::info!("Copying image data to image with desc: {:#?}", image.desc);

    let image_hdl = image.inner;
    let subresource_range = image.subresource_all();
    let queue_family = upload_queue.family_index;
    recorder.record(move |device, cmd| {
        unsafe {
            //init layout to receive copy
            device.cmd_pipeline_barrier(
                *cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier {
                    image: image_hdl,
                    src_access_mask: vk::AccessFlags::empty(),
                    dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    src_queue_family_index: queue_family,
                    dst_queue_family_index: queue_family,
                    old_layout: vk::ImageLayout::UNDEFINED,
                    new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    subresource_range,
                    ..Default::default()
                }],
            );

            device.cmd_copy_buffer_to_image(
                *cmd,
                staging_buffer.inner,
                image_hdl,
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
        }
    });

    recorder.finish_recording()?;

    cb.submit(device, upload_queue, &[], &[])?;
    cb.wait().map_err(|e| CommandBufferError::from(e))?;

    Ok(image)
}

///Loads an image from `image`. `image` in this case is an somehow by the `image` crate initialized image. This can happen when using
/// external loaders for instance. And yes, *images* is confusing here, but typesignatures should help.
///
///
///The images format, extent etc. are decided from the files properties. But you can
/// read those afterwards from the file directly. Note that each *integer* format is used as `UNORM` which is
/// not always correct. If you want to control the format/loading process your self, consider using [image_from_data](image_from_data)
///
///
/// If you need a certain format, consider using a Blit operation after loading.
#[cfg(feature = "image_loading")]
pub fn image_from_image<A: Allocator + Send + Sync + 'static>(
    device: &Arc<Device>,
    allocator: &Arc<Mutex<A>>,
    upload_queue: &Queue,
    usage: vk::ImageUsageFlags,
    img: image::DynamicImage,
) -> Result<Image, MarpiiError> {
    use image::GenericImageView;
    use marpii::resources::ImageType;

    let (width, height) = img.dimensions();

    //TODO decide for a format

    let format = match &img {
        image::DynamicImage::ImageLuma8(_) => vk::Format::R8_UNORM,
        image::DynamicImage::ImageLumaA8(_) => vk::Format::R8G8_UNORM,
        image::DynamicImage::ImageRgb8(_) => vk::Format::R8G8B8_UNORM,
        image::DynamicImage::ImageRgba8(_) => vk::Format::R8G8B8A8_UNORM,
        image::DynamicImage::ImageLuma16(_) => vk::Format::R16_UNORM,
        image::DynamicImage::ImageLumaA16(_) => vk::Format::R16G16_UNORM,
        image::DynamicImage::ImageRgb16(_) => vk::Format::R16G16B16_UNORM,
        image::DynamicImage::ImageRgba16(_) => vk::Format::R16G16B16A16_UNORM,
        image::DynamicImage::ImageRgb32F(_) => vk::Format::R32G32B32_SFLOAT,
        image::DynamicImage::ImageRgba32F(_) => vk::Format::R32G32B32A32_SFLOAT,
        _ => {
            return Err(MarpiiError::Other(String::from(
                "Could not translate image format to vulkan format",
            )))
        }
    };

    let desc = ImgDesc {
        extent: vk::Extent3D {
            width,
            height,
            depth: 1,
        },
        format,
        img_type: ImageType::Tex2d,
        mip_levels: 1,
        samples: vk::SampleCountFlags::TYPE_1,
        sharing_mode: SharingMode::Exclusive,
        tiling: vk::ImageTiling::LINEAR,
        usage,
        ..Default::default()
    };

    image_from_data(device, allocator, upload_queue, desc, None, img.as_bytes())
}

///Loads an image from `file`.
///The images format, extent etc. are decided from the files properties. But you can
/// read those afterwards from the file directly. Note that each *integer* format is used as `UNORM` which is
/// not always correct. If you want to control the format/loading process your self, consider using [image_from_data](image_from_data)
///
/// If you need a certain format, consider using a Blit operation after loading.
#[cfg(feature = "image_loading")]
pub fn image_from_file<A: Allocator + Send + Sync + 'static>(
    device: &Arc<Device>,
    allocator: &Arc<Mutex<A>>,
    upload_queue: &Queue,
    usage: vk::ImageUsageFlags,
    file: impl AsRef<std::path::Path>,
) -> Result<Image, MarpiiError> {
    let img = image::open(file)
        .map_err(|e| MarpiiError::Other(format!("Failed to open image: {}", e)))?;
    image_from_image(device, allocator, upload_queue, usage, img)
}
