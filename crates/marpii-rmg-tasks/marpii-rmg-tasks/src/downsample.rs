use std::sync::Arc;

use crate::TaskError;
use marpii::{
    ash::vk,
    resources::{ComputePipeline, ImageType, PushConstant, ShaderModule},
    MarpiiError, OoS,
};
use marpii_rmg::{ImageHandle, Rmg, Task};
use marpii_rmg_task_shared::{DownsamplePush, ResourceHandle};
use thiserror::Error;

const DOWNSAMPLE_SHADER: &'static [u8] = include_bytes!("../resources/downsample.spv");

#[derive(Error, Debug)]
pub enum DownsampleError {
    #[error("Image needs support for simultaneous SHADER_READ and SHADER_WRITE, but had following flags set: {0:?}.")]
    ImageFlags(vk::ImageUsageFlags),
    #[error("Maximum per axis size is 4096, but was {0:?}")]
    Extent(vk::Extent3D),
    #[error("Downsampler only supports 2d images, no arrays or cubemaps, but was {0:?}")]
    ImgType(ImageType),
}

//Inner, single blit operation.
struct MipBlit {
    image: ImageHandle,
    src_level: u32,
    dst_level: u32,
}

/* TODO: I actually wanted to do the fancy "downsample in one go" trick from AMD. However, this needs that the image is bound as an array,
 *       or at least each mip level as a singel ImageHandel. Which is shite. Instead
 *       We implement a meta pass for now that build seperate views and uses image blit instead.
///Single pass downsample task inspired by AMD's [FidelityFx downsampler](https://github.com/GPUOpen-Effects/FidelityFX-SPD).
/// Can downsample up to 4096²px textures.
pub struct Downsample {
    image: ImageHandle,

    pipeline: Arc<ComputePipeline>,
    push: PushConstant<DownsamplePush>,
}

impl Downsample {
    pub const MAX_EXTENT: u32 = 4096;

    pub fn new(rmg: &mut Rmg, image: ImageHandle) -> Result<Self, TaskError<DownsampleError>> {
        //Sort out most runtime errors
        if !image
            .image_desc()
            .usage
            .contains(vk::ImageUsageFlags::STORAGE)
        {
            return Err(TaskError::Task(DownsampleError::ImageFlags(
                image.image_desc().usage,
            )));
        }

        if image.extent_3d().width > Self::MAX_EXTENT
            || image.extent_3d().height > Self::MAX_EXTENT
            || image.extent_3d().depth > Self::MAX_EXTENT
        {
            return Err(TaskError::Task(DownsampleError::Extent(image.extent_3d())));
        }

        if
        //image.image_desc().img_type == ImageType::Tex1d
        image.image_desc().img_type != ImageType::Tex2d
        //|| image.image_desc().img_type == ImageType::Tex3d
        {
            return Err(TaskError::Task(DownsampleError::ImgType(
                image.image_desc().img_type,
            )));
        }

        let push = PushConstant::new(
            DownsamplePush {
                img: ResourceHandle::INVALID,
                pad0: [ResourceHandle::INVALID; 3],
                mip_level: image.image_desc().mip_levels,
                pad1: [0; 3],
            },
            vk::ShaderStageFlags::COMPUTE,
        );

        let shader_module = ShaderModule::new_from_bytes(&rmg.ctx.device, DOWNSAMPLE_SHADER)
            .map_err(|e| MarpiiError::from(e))?;

        let shader_stage = shader_module.into_shader_stage(vk::ShaderStageFlags::COMPUTE, "main");
        //No additional descriptors for us
        let layout = rmg.resources.bindless_layout();
        let pipeline = Arc::new(
            ComputePipeline::new(
                &rmg.ctx.device,
                &shader_stage,
                None,
                OoS::new_shared(layout),
            )
            .map_err(|e| MarpiiError::from(e))?,
        );

        Ok(Downsample {
            image,
            push,
            pipeline,
        })
    }
}

impl Task for Downsample {
    fn name(&self) -> &'static str {
        "Downsample"
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }

    fn pre_record(
        &mut self,
        resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        self.push.get_content_mut().img = resources.resource_handle_or_bind(&self.image)?;
        self.push.get_content_mut().mip_level = self.image.image_desc().mip_levels;
        Ok(())
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_image(
                &self.image,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
            )
            .unwrap();
        registry.register_asset(self.pipeline.clone());
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        _resources: &marpii_rmg::Resources,
    ) {
        //group size is 8x8x8
        let ext = self.image.extent_2d();
        let dispatch = [(ext.width / 8).max(1), (ext.height / 8).max(1)];
        //calculate dispatch.
        //bind commandbuffer, setup push constant and execute
        unsafe {
            device.inner.cmd_bind_pipeline(
                *command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline.pipeline,
            );
            device.inner.cmd_push_constants(
                *command_buffer,
                self.pipeline.layout.layout,
                vk::ShaderStageFlags::ALL,
                0,
                self.push.content_as_bytes(),
            );

            device
                .inner
                .cmd_dispatch(*command_buffer, dispatch[0], dispatch[1], 1);
        }
    }
}
*/
