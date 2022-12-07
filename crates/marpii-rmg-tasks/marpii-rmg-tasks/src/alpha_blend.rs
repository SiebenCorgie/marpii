use marpii::{
    ash::vk,
    resources::{ComputePipeline, FormatType, PushConstant, ShaderModule},
};
use marpii_rmg::{ImageHandle, Rmg, Task};
use marpii_rmg_task_shared::{AlphaBlendPush, ResourceHandle};
use std::sync::Arc;

const BLEND_SHADER_F32: &'static [u8] = include_bytes!("../../resources/alphablend_f32.spv");
const BLEND_SHADER_U8: &'static [u8] = include_bytes!("../../resources/alphablend_u8.spv");

///Adds one image (`add`) to another (`dst`) using `add`'s alpha channel to determin blending.
///
/// Note that the images are always assumed to be 2D. The extent is clamped to the minimum of `add`'s offset+extent, `dst`'s offset+extent and the specified extent. This basically prevents the
/// compute shader from reading/writing outside the images.
///
/// # Image format
///
/// The format type declares the format type that the pipeline assumes for the supplied images.
/// Those should be in general be the same for `add` and `dst`.
///
/// If the format type changes a new pipeline needs to be build. In that case it is easiest to re-create the task.
//TODO: Check for other image types and start different blend shader? Or create different task.
pub struct AlphaBlend {
    ///Image that is added to dst
    pub add: ImageHandle,
    ///From which pixel on `add` `extent` is copied to the dst region.
    pub add_offset: vk::Offset2D,
    ///The destination image
    pub dst: ImageHandle,
    ///From which pixel the blending region on `dst` starts.
    pub dst_offset: vk::Offset2D,
    pub extent: vk::Extent2D,

    push_constant: PushConstant<AlphaBlendPush>,
    pipeline_format: FormatType,
    pipeline: Arc<ComputePipeline>,
}

impl AlphaBlend {
    ///Creates the task for an target image with the specified `extent` and `format`. Note that the no blending occurs if
    /// any of the src images does not have an alpha channel.
    pub fn new(
        rmg: &mut Rmg,
        add: ImageHandle,
        add_offset: vk::Offset2D,
        dst: ImageHandle,
        dst_offset: vk::Offset2D,
        extent: vk::Extent2D,
        format_type: FormatType,
    ) -> Result<Self, anyhow::Error> {
        let push_constant = PushConstant::new(
            AlphaBlendPush {
                add: ResourceHandle::INVALID,
                dst: ResourceHandle::INVALID,
                pad0: [ResourceHandle::INVALID; 2],
                add_offset: [0i32; 2],
                dst_offset: [0i32; 2],
                extent: [0; 2],
                pad1: [0; 2],
            },
            vk::ShaderStageFlags::COMPUTE,
        );

        #[cfg(feature = "logging")]
        log::trace!("Load rust shader module");

        let shader_module = match format_type {
            FormatType::F32 => ShaderModule::new_from_bytes(&rmg.ctx.device, BLEND_SHADER_F32)?,
            FormatType::U8 => ShaderModule::new_from_bytes(&rmg.ctx.device, BLEND_SHADER_U8)?,
            _ => {
                #[cfg(feature = "logging")]
                log::error!(
                    "FormatType {:?} not supported by alpha blending",
                    format_type
                );
                return Err(anyhow::anyhow!(
                    "FormatType {:?} not supported by alpha blending",
                    format_type
                ));
            }
        };

        #[cfg(feature = "logging")]
        log::trace!("Load blend module for {:?}", format_type);

        let shader_stage = shader_module.into_shader_stage(vk::ShaderStageFlags::COMPUTE, "main");
        //No additional descriptors for us
        let layout = rmg.resources().bindless_layout();
        let pipeline = Arc::new(ComputePipeline::new(
            &rmg.ctx.device,
            &shader_stage,
            None,
            layout,
        )?);

        Ok(AlphaBlend {
            add,
            add_offset,
            dst,
            dst_offset,
            extent,
            push_constant,
            pipeline_format: format_type,
            pipeline,
        })
    }

    pub fn format_type(&self) -> &FormatType {
        &self.pipeline_format
    }
}

impl Task for AlphaBlend {
    fn name(&self) -> &'static str {
        "AlphaBlend"
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }

    fn pre_record(
        &mut self,
        resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        self.push_constant.get_content_mut().add = resources.resource_handle_or_bind(&self.add)?;
        self.push_constant.get_content_mut().dst = resources.resource_handle_or_bind(&self.dst)?;
        self.push_constant.get_content_mut().add_offset = [self.add_offset.x, self.add_offset.y];
        self.push_constant.get_content_mut().extent = [self.extent.width, self.extent.height];
        self.push_constant.get_content_mut().dst_offset = [self.dst_offset.x, self.dst_offset.y];
        Ok(())
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_image(
                &self.add,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_STORAGE_READ,
                vk::ImageLayout::GENERAL,
            )
            .unwrap();
        registry
            .request_image(
                &self.dst,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
            )
            .unwrap();
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        _resources: &marpii_rmg::Resources,
    ) {
        //group size is 8x8x8
        let dispatch = [
            (self.extent.width / 8).max(1),
            (self.extent.height / 8).max(1),
        ];
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
                self.push_constant.content_as_bytes(),
            );

            device
                .inner
                .cmd_dispatch(*command_buffer, dispatch[0], dispatch[1], 1);
        }
    }
}
