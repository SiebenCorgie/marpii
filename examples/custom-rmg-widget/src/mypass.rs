use std::sync::Arc;
use std::time::Instant;

use iced_marpii::marpii;
use iced_marpii::marpii::resources::{ImageType, ImgDesc};
use iced_marpii::marpii_rmg::marpii_rmg_shared::ResourceHandle;
use iced_marpii::marpii_rmg::{self, Rmg};
use marpii::ash::vk;
use marpii::resources::{ComputePipeline, PushConstant, ShaderModule};
use marpii::OoS;
use marpii_rmg::{ImageHandle, Task};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CSPush {
    pub target_color: ResourceHandle,
    pub target_depth: ResourceHandle,
    pub resolution: [u32; 2],

    pub bound_offset: [f32; 2],
    pub bound_size: [f32; 2],

    pub layer_depth: f32,
    pub time: f32,
    pad0: [f32; 2],
}

impl Default for CSPush {
    fn default() -> Self {
        Self {
            target_color: ResourceHandle::INVALID,
            target_depth: ResourceHandle::INVALID,
            resolution: [0; 2],
            bound_size: [0.0; 2],
            bound_offset: [0.0; 2],
            layer_depth: 0.0,
            time: 0.0,
            pad0: [0.0; 2],
        }
    }
}

#[derive(Clone)]
pub struct MyRenderPass {
    pipeline: Arc<ComputePipeline>,
    pub color_image: ImageHandle,
    pub depth_image: ImageHandle,
    pub push: PushConstant<CSPush>,
    start: Instant,
}

impl MyRenderPass {
    const SHADER_COMP: &[u8] = include_bytes!("custom_shader.spirv");
    const COLOR_USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::from_raw(
        vk::ImageUsageFlags::STORAGE.as_raw() | vk::ImageUsageFlags::TRANSFER_SRC.as_raw(),
    );
    const DEPTH_USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::from_raw(
        vk::ImageUsageFlags::STORAGE.as_raw()
            | vk::ImageUsageFlags::TRANSFER_SRC.as_raw()
            | vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT.as_raw(),
    );

    pub fn create(rmg: &mut Rmg, ext: vk::Extent2D) -> Self {
        let shader_module =
            ShaderModule::new_from_bytes(&rmg.ctx.device, Self::SHADER_COMP).unwrap();
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
            .unwrap(),
        );

        let color_format = rmg
            .ctx
            .device
            .select_format(
                Self::COLOR_USAGE,
                vk::ImageTiling::OPTIMAL,
                &[
                    vk::Format::R8G8B8A8_UNORM,
                    vk::Format::R16G16B16A16_UNORM,
                    vk::Format::R16G16B16A16_SFLOAT,
                    vk::Format::R32G32B32A32_SFLOAT,
                ],
            )
            .expect("Could not select buffer storage format");

        let depth_format = rmg
            .ctx
            .device
            .select_format(
                Self::DEPTH_USAGE,
                vk::ImageTiling::OPTIMAL,
                &[
                    vk::Format::D16_UNORM,
                    vk::Format::D16_UNORM_S8_UINT,
                    vk::Format::D24_UNORM_S8_UINT,
                    vk::Format::D32_SFLOAT,
                    vk::Format::D32_SFLOAT_S8_UINT,
                ],
            )
            .expect("Could not select depth buffer storage format");
        let color_image = rmg
            .new_image_uninitialized(
                ImgDesc::storage_image_2d(ext.width, ext.height, color_format),
                Some("compute-storage-color"),
            )
            .unwrap();

        let depth_desc = ImgDesc {
            usage: Self::DEPTH_USAGE,
            img_type: ImageType::Tex2d,
            extent: vk::Extent3D {
                width: ext.width,
                height: ext.height,
                depth: 0,
            },
            format: depth_format,

            ..Default::default()
        };

        let depth_image = rmg
            .new_image_uninitialized(depth_desc, Some("compute-storage-depth"))
            .unwrap();

        Self {
            pipeline,
            color_image,
            depth_image,
            push: PushConstant::new(CSPush::default(), vk::ShaderStageFlags::COMPUTE),
            start: Instant::now(),
        }
    }

    fn dispatch_count(&self) -> [u32; 3] {
        [
            ((self.color_image.extent_2d().width as f32 / 8.0).ceil() as u32).max(1),
            ((self.color_image.extent_2d().height as f32 / 8.0).ceil() as u32).max(1),
            1,
        ]
    }

    pub fn resize(&mut self, rmg: &mut Rmg, ext: vk::Extent2D) {
        if self.color_image.extent_2d() != ext {
            let mut color_desc = self.color_image.image_desc().clone();
            color_desc.extent.width = ext.width;
            color_desc.extent.height = ext.height;
            self.color_image = rmg
                .new_image_uninitialized(color_desc, Some("compute-storage-color"))
                .unwrap();
        }
        if self.depth_image.extent_2d() != ext {
            let mut depth_desc = self.depth_image.image_desc().clone();
            depth_desc.extent.width = ext.width;
            depth_desc.extent.height = ext.height;
            self.depth_image = rmg
                .new_image_uninitialized(depth_desc, Some("compute-storage-depth"))
                .unwrap();
        }

        self.push.get_content_mut().resolution = [ext.width, ext.height];
    }
}

impl Task for MyRenderPass {
    fn name(&self) -> &'static str {
        "RmgPrimitive"
    }
    fn pre_record(
        &mut self,
        resources: &mut marpii_rmg::Resources,
        _ctx: &marpii_rmg::CtxRmg,
    ) -> Result<(), marpii_rmg::RecordError> {
        self.push.get_content_mut().target_color =
            resources.resource_handle_or_bind(&self.color_image)?;
        self.push.get_content_mut().target_depth =
            resources.resource_handle_or_bind(&self.depth_image)?;
        Ok(())
    }
    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry
            .request_image(
                &self.color_image,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
            )
            .unwrap();
        registry
            .request_image(
                &self.depth_image,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
            )
            .unwrap();
        registry.register_asset(self.pipeline.clone());
    }
    fn queue_flags(&self) -> marpii::ash::vk::QueueFlags {
        marpii::ash::vk::QueueFlags::COMPUTE | marpii::ash::vk::QueueFlags::GRAPHICS
    }
    fn record(
        &mut self,
        device: &Arc<marpii::context::Device>,
        command_buffer: &marpii::ash::vk::CommandBuffer,
        _resources: &marpii_rmg::Resources,
    ) {
        let time = self.start.elapsed().as_secs_f32();
        self.push.get_content_mut().time = time;

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

            let [dx, dy, dz] = self.dispatch_count();

            device.inner.cmd_dispatch(*command_buffer, dx, dy, dz);
        }
    }
}
