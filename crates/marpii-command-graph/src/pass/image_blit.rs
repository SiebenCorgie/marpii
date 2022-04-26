use marpii::{ash::vk, util::extent_to_offset};
use marpii_commands::Recorder;

use super::{AssumedState, Pass, SubPassRequirement};
use crate::{state::StImage, ImageState};

///Simple subpass that blits one image to another.
///
/// In case of non matching dimensions (1d to 2d, 2d to 3d or arrays), the minimum of both is used.
/// For instance, when bliting a 1d image to a 2d image, only the first row is blit.
/// when blitting a 2-element array to a 4-element array, only the first two are blit.
pub struct ImageBlit {
    src: StImage,
    dst: StImage,

    assume: [AssumedState; 2],
}

impl ImageBlit {
    pub fn new(src: StImage, dst: StImage) -> Self {
        let assume = [
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

        ImageBlit { src, dst, assume }
    }

    pub fn set_images(&mut self, src: StImage, dst: StImage) {
        self.src = src;
        self.dst = dst;
        self.assume = [
            AssumedState::Image {
                image: self.src.clone(),
                state: ImageState {
                    access_mask: vk::AccessFlags::TRANSFER_READ,
                    layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                },
            },
            AssumedState::Image {
                image: self.dst.clone(),
                state: ImageState {
                    access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                },
            },
        ]
    }

    pub fn src_image(&self) -> &StImage {
        &self.src
    }

    pub fn dst_image(&self) -> &StImage {
        &self.dst
    }
}

impl Pass for ImageBlit {
    fn assumed_states(&self) -> &[AssumedState] {
        &self.assume
    }

    ///The actual recording step. Gets provided with access to the actual resources through the
    /// `ResourceManager` as well as the `command_buffer` recorder that is currently in use.
    fn record(&mut self, command_buffer: &mut Recorder) -> Result<(), anyhow::Error> {
        let src_img = self.src.image().clone();
        let dst_img = self.dst.image().clone();

        let src_ext = extent_to_offset(src_img.extent_3d(), true);
        let dst_ext = extent_to_offset(dst_img.extent_3d(), true);
        //TODO: actually create the min subresource ranges. But need a test case for that

        let src_subresource = src_img.subresource_layers_all();
        let dst_subresource = dst_img.subresource_layers_all();

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
                        src_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, src_ext],
                        dst_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, dst_ext],
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
        &[SubPassRequirement::TransferBit]
    }
}
