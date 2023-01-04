use marpii::{ash::vk, util::ImageRegion};
use marpii_rmg::{ImageHandle, Task};

///Blits `N` a regions of one image to another. Always blits all subresource layers.
pub struct ImageBlit<const N: usize> {
    pub blits: [(ImageRegion, ImageRegion); N],
    pub src: ImageHandle,
    pub dst: ImageHandle,
}

impl ImageBlit<0> {
    pub fn new(src: ImageHandle, dst: ImageHandle) -> Self {
        ImageBlit {
            blits: [],
            src,
            dst,
        }
    }
}

impl<const N: usize> ImageBlit<N> {
    ///Overwrites `self` to use the given (src, dst) blit operation pairs.
    pub fn with_blits<const M: usize>(
        self,
        blit_regions: [(ImageRegion, ImageRegion); M],
    ) -> ImageBlit<M> {
        ImageBlit {
            blits: blit_regions,
            src: self.src,
            dst: self.dst,
        }
    }
}

impl<const N: usize> Task for ImageBlit<N> {
    fn name(&self) -> &'static str {
        "Image Blit"
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_image(
                &self.src,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            )
            .unwrap();
        registry
            .request_image(
                &self.dst,
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            )
            .unwrap();
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &marpii::ash::vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        let src_image = resources.get_image_state(&self.src);
        let dst_image = resources.get_image_state(&self.dst);

        let mut regions = [vk::ImageBlit2::default(); N];
        let src_subresource = src_image.image.subresource_layers_all();
        let dst_subresource = dst_image.image.subresource_layers_all();
        for (idx, blit) in self.blits.iter_mut().enumerate() {
            blit.0.clamp_to(&blit.1);
            blit.1.clamp_to(&blit.0);

            regions[idx] = vk::ImageBlit2::builder()
                .src_offsets(blit.0.to_blit_offsets())
                .dst_offsets(blit.1.to_blit_offsets())
                .src_subresource(src_subresource)
                .dst_subresource(dst_subresource)
                .build();
        }

        let blit_image_info = vk::BlitImageInfo2::builder()
            .src_image(src_image.image.inner)
            .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .dst_image(dst_image.image.inner)
            .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .filter(vk::Filter::LINEAR)
            .regions(&regions);

        unsafe {
            device
                .inner
                .cmd_blit_image2(*command_buffer, &blit_image_info)
        }
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::GRAPHICS | vk::QueueFlags::TRANSFER
    }
}
