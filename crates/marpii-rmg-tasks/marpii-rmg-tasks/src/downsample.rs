use crate::{ImageBlit, TaskError};
use marpii::{ash::vk, resources::ImageType};
use marpii_rmg::{recorder::task::MetaTask, ImageHandle, Rmg, Task};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DownsampleError {
    #[error("Image needs support for simultaneous SHADER_READ and SHADER_WRITE, but had following flags set: {0:?}.")]
    ImageFlags(vk::ImageUsageFlags),
    #[error("Maximum per axis size is 4096, but was {0:?}")]
    Extent(vk::Extent3D),
    #[error("Downsampler only supports 2d images, no arrays or cubemaps, but was {0:?}")]
    ImgType(ImageType),
}

struct MipCopy {
    mip: ImageHandle,
    dst: ImageHandle,
    dst_mip: u32,
}

impl Task for MipCopy {
    fn name(&self) -> &'static str {
        "MipCopy"
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::TRANSFER
    }
    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_image(
                &self.mip,
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
        command_buffer: &vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        let mut dst_subresource = self.dst.image_desc().subresource_layers_all();
        dst_subresource.mip_level = self.dst_mip;
        let copy = vk::ImageCopy2::default()
            .src_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .dst_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .src_subresource(self.mip.image_desc().subresource_layers_all())
            .dst_subresource(dst_subresource)
            .extent(self.mip.extent_3d());
        let regions = [*copy];
        let copy_image = vk::CopyImageInfo2::default()
            .regions(&regions)
            .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_image(resources.get_image_state(&self.mip).image.inner)
            .dst_image(resources.get_image_state(&self.dst).image.inner);

        unsafe { device.inner.cmd_copy_image2(*command_buffer, &copy_image) };
    }
}

pub struct Downsample {
    downsample_ops: Vec<ImageBlit<1>>,
    mips: Vec<MipCopy>,
}

impl Downsample {
    pub fn new(rmg: &mut Rmg, image: ImageHandle) -> Result<Self, TaskError<DownsampleError>> {
        let levels = image.image_desc().mip_levels as usize;
        let mut mips: Vec<MipCopy> = Vec::with_capacity(levels);
        let mut downsample_ops = Vec::with_capacity(levels - 1);

        let mut desc = image.image_desc().clone();
        let mut mip_size = image.extent_3d().clone();
        let mut parent = image.clone();
        for mip in 1..levels {
            mip_size.width = (mip_size.width / 2).max(1);
            mip_size.height = (mip_size.height / 2).max(1);
            mip_size.depth = (mip_size.depth / 2).max(1);
            //update size
            desc.extent = mip_size;
            //reset mip level
            desc.mip_levels = 1;
            let mip_img =
                rmg.new_image_uninitialized(desc.clone(), Some(&format!("Mip[{}]", mip)))?;
            let downsample_blit = crate::ImageBlit::new(parent.clone(), mip_img.clone())
                .with_blits([(parent.region_all(), mip_img.region_all())]);

            parent = mip_img.clone();
            downsample_ops.push(downsample_blit);
            mips.push(MipCopy {
                mip: mip_img,
                dst_mip: mip as u32,
                dst: image.clone(),
            });
        }

        Ok(Downsample {
            mips,
            downsample_ops,
        })
    }
}

impl MetaTask for Downsample {
    fn record<'a>(
        &'a mut self,
        mut recorder: marpii_rmg::Recorder<'a>,
    ) -> Result<marpii_rmg::Recorder<'a>, marpii_rmg::RecordError> {
        for down in &mut self.downsample_ops {
            recorder = recorder.add_task(down).unwrap();
        }
        //now copy all mips
        for mipcpy in &mut self.mips {
            recorder = recorder.add_task(mipcpy).unwrap();
        }

        Ok(recorder)
    }
}

/* TODO: I actually wanted to do the fancy "downsample in one go" trick from AMD. However, this needs that the image is bound as an array,
 *       or at least each mip level as a singel ImageHandle. Which is currently not possible in rmg. Instead
 *       We implement a meta pass for now that build separate views and uses image blit instead.
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
