use marpii::{ash::vk, util::ImageRegion};
use marpii_commands::Recorder;

use crate::{ImageState, StImage};

use super::{AssumedState, Pass, SubPassRequirement};

///Blits an image (or a region of an image) to another image (or a region there of). If you want to blit one whole image to another whole image, consider using the simple [ImageBlit](super::ImageBlit) pass.
pub struct BlitToRegion {
    src: StImage,
    src_region: ImageRegion,
    dst: StImage,
    dst_region: ImageRegion,

    assumed_state: [AssumedState; 2],
}

impl BlitToRegion {
    ///Blits the given region of the source image to the given region of the destination image.
    pub fn new(
        src: StImage,
        src_region: ImageRegion,
        dst: StImage,
        dst_region: ImageRegion,
    ) -> Self {
        let assumed_state = [
            AssumedState::Image {
                image: src.clone(),
                state: ImageState {
                    access_mask: vk::AccessFlags::TRANSFER_READ,
                    layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                },
            },
            AssumedState::Image {
                image: dst.clone(),
                state: ImageState {
                    access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                },
            },
        ];

        BlitToRegion {
            assumed_state,
            dst,
            dst_region,
            src_region,
            src,
        }
    }

    ///Blits the whole image to the given region of the destination image.
    pub fn image_to_region(src: StImage, dst: StImage, dst_region: ImageRegion) -> Self {
        let src_region = ImageRegion {
            offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            extent: src.image().extent_3d(),
        };
        Self::new(src, src_region, dst, dst_region)
    }

    pub fn region_to_image(src: StImage, src_region: ImageRegion, dst: StImage) -> Self {
        let dst_region = ImageRegion {
            offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            extent: dst.image().extent_3d(),
        };

        Self::new(src, src_region, dst, dst_region)
    }
}

impl Pass for BlitToRegion {
    fn assumed_states(&self) -> &[AssumedState] {
        &self.assumed_state
    }

    ///The actual recording step. Gets provided with access to the actual resources through the
    /// `ResourceManager` as well as the `command_buffer` recorder that is currently in use.
    fn record(&mut self, command_buffer: &mut Recorder) -> Result<(), anyhow::Error> {
        let src_img = self.src.image().clone();
        let dst_img = self.dst.image().clone();

        //TODO: actually create the min subresource ranges. But need a test case for that

        let src_subresource = src_img.subresource_layers_all();
        let dst_subresource = dst_img.subresource_layers_all();

        let src_offsets = self.src_region.to_blit_offsets();
        let dst_offsets = self.dst_region.to_blit_offsets();
        command_buffer.record({
            move |dev, cmd| unsafe {
                dev.cmd_blit_image(
                    *cmd,
                    src_img.inner,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    dst_img.inner,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::ImageBlit {
                        //Note we are using blit mainly for format transfer
                        src_offsets,
                        dst_offsets,
                        src_subresource,
                        dst_subresource,
                        ..Default::default()
                    }],
                    vk::Filter::LINEAR,
                );
            }
        });

        Ok(())
    }

    fn requirements(&self) -> &'static [SubPassRequirement] {
        &[
            SubPassRequirement::TransferBit,
            SubPassRequirement::GraphicsBit,
        ]
    }
}
