use marpii::{
    ash::vk,
    resources::{Buffer, BufferMapError, Image, ImgDesc},
    MarpiiError, OoS,
};
use marpii_rmg::{BufferHandle, ImageHandle, ResourceRegistry, Resources, Rmg, RmgError, Task};
use std::sync::Arc;

use crate::RmgTaskError;

///Describes a buffer range for the data of a mip map.
pub struct MipOffset {
    ///The mip level of the texture this is writing to
    pub mip_level: u32,
    ///Offset into the source buffer at which this mip-map data starts.
    pub offset: u64,
    ///extent of the mip map
    pub extent: vk::Extent3D,
    ///How many layers this mip has. Usually 1, in case of a cube map 6.
    pub layer_count: u32,
}

///Transfer pass that copies data to an image on the GPU.
/// perfect if you need to initialise textures for instance.
/// Note that this only works reliable for 2D and 3D images.
/// Does not work for cubemaps!
pub struct UploadImage {
    ///The GPU-Local image, which will contain `new`'s `data` after this pass was submitted.
    pub image: ImageHandle,
    pub mip_maps: Option<Vec<MipOffset>>,
    upload: BufferHandle<u8>,
}

impl UploadImage {
    //TODO: add tasks constructors, for instance automatic "load from file"?

    pub fn new_from_image<'dta>(
        rmg: &mut Rmg,
        data: &'dta [u8],
        image: impl Into<OoS<Image>>,
    ) -> Result<Self, RmgTaskError> {
        let image = image.into();
        let staging = Buffer::new_staging_for_data(
            &rmg.ctx.device,
            &rmg.ctx.allocator,
            Some("StagingBuffer"),
            data,
        )
        .map_err(|e| MarpiiError::from(e))?;

        staging.flush_range().map_err(|e| {
            #[cfg(feature = "logging")]
            log::error!("Flushing upload image failed: {}", e);
            MarpiiError::from(BufferMapError::FailedToFlush)
        })?;
        let staging = rmg
            .import_buffer(Arc::new(staging), None, None)
            .map_err(|e| RmgError::from(e))?;

        //register image in rmg
        let image = rmg
            .resources
            .add_image(image)
            .map_err(|e| RmgTaskError::RmgError(RmgError::ResourceError(e)))?;

        Ok(UploadImage {
            image,
            mip_maps: None,
            upload: staging,
        })
    }

    ///Creates the upload task. Note that data is interpreted as whatever `desc`'s format is.
    /// If this is wrong you will get artefacts. Use a format convertion before (on CPU), or a chained GPU based
    /// convertion task otherwise.
    pub fn new<'dta>(
        rmg: &mut Rmg,
        data: &'dta [u8],
        mut desc: ImgDesc,
    ) -> Result<Self, RmgTaskError> {
        if !desc.usage.contains(vk::ImageUsageFlags::TRANSFER_DST) {
            #[cfg(feature = "logging")]
            log::warn!("Upload image had TRANSEFER_DST not set, adding to usage...");
            desc.usage |= vk::ImageUsageFlags::TRANSFER_DST;
        }

        let image = Image::new(
            &rmg.ctx.device,
            &rmg.ctx.allocator,
            desc,
            marpii::allocator::MemoryUsage::GpuOnly,
            None,
        )
        .map_err(|e| RmgTaskError::Marpii(MarpiiError::DeviceError(e)))?;
        Self::new_from_image(rmg, data, image)
    }

    ///Adds the given copy operation which
    pub fn with_mip_maps(mut self, mip_maps: Vec<MipOffset>) -> Self {
        self.mip_maps = Some(mip_maps);
        self
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
        registry
            .request_image(
                &self.image,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            )
            .unwrap();
        registry
            .request_buffer(
                &self.upload,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
            )
            .unwrap();
    }
    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &Resources,
    ) {
        let buffer = resources.get_buffer_state(&self.upload);
        let img = resources.get_image_state(&self.image);

        let mut copies =
            Vec::with_capacity(1 + self.mip_maps.as_ref().map(|maps| maps.len()).unwrap_or(0));

        //This is the first copy, it will just copy 0..image_memory_size to the first mip.
        copies.push(
            vk::BufferImageCopy2::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_extent(img.image.desc.extent)
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_subresource(img.image.subresource_layers_all()),
        );

        let mut offset = 0;

        //If we have mip copies, copy those as well.
        if let Some(mips) = &self.mip_maps {
            for mip in mips {
                let mut subres = img.image.subresource_layers_all();
                subres.mip_level = mip.mip_level;
                subres.base_array_layer = 0;
                subres.layer_count = mip.layer_count;
                copies.push(
                    vk::BufferImageCopy2::default()
                        .buffer_offset(offset)
                        .buffer_row_length(0)
                        .buffer_image_height(0)
                        .image_extent(mip.extent)
                        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                        .image_subresource(subres),
                );
                offset += mip.offset * u64::from(mip.layer_count);
            }
        }

        //Finally execute copies for all defined regions
        unsafe {
            device.inner.cmd_copy_buffer_to_image2(
                *command_buffer,
                &vk::CopyBufferToImageInfo2::default()
                    .src_buffer(buffer.buffer.inner)
                    .dst_image(img.image.inner)
                    .regions(&copies)
                    .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL),
            );
        }
    }
}
