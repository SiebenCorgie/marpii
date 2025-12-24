use easy_gltf::Scene;
use marpii::{
    ash::vk,
    resources::{BufDesc, ImgDesc, ShaderModule},
};
use marpii_rmg::{
    helper::{
        rasterpass::{GenericRasterPass, RasterDrawCall, RasterPassBuilder},
        BufferUsage, ImageUsage,
    },
    BufferHandle, ImageHandle, Rmg, RmgError,
};
use marpii_rmg_tasks::UploadBuffer;
use shared::{SimObj, Ubo, Vertex};

use crate::{model_loading::load_model, OBJECT_COUNT};

const SHADER_VS: &[u8] = include_bytes!("../resources/forward_vs.spv");
const SHADER_FS: &[u8] = include_bytes!("../resources/forward_fs.spv");

pub struct ForwardPass {
    pub color_image: ImageHandle,
    depth_image: ImageHandle,
    pub sim_src: BufferHandle<SimObj>,

    pub pass: Option<GenericRasterPass<shared::ForwardPush>>,

    ///VertexBuffer we are using to draw objects
    vertex_buffer: BufferHandle<Vertex>,
    index_buffer: BufferHandle<u32>,

    //Camera data used
    ubo_buffer: BufferHandle<Ubo>,
}

impl ForwardPass {
    pub fn new(
        rmg: &mut Rmg,
        ubo: BufferHandle<Ubo>,
        simulation_buffer: BufferHandle<SimObj>,
        gltf: &[Scene],
        initial_framebuffer_size: [u32; 2],
    ) -> Result<Self, RmgError> {
        let (vertex_buffer_data, index_buffer_data) = load_model(gltf);

        let mut ver_upload = UploadBuffer::new_with_buffer(
            rmg,
            &vertex_buffer_data,
            BufDesc::storage_buffer::<Vertex>(vertex_buffer_data.len())
                .add_usage(vk::BufferUsageFlags::TRANSFER_DST),
        )?;

        let mut ind_upload = UploadBuffer::new_with_buffer(
            rmg,
            &index_buffer_data,
            BufDesc::index_buffer_u32(index_buffer_data.len())
                .add_usage(vk::BufferUsageFlags::STORAGE_BUFFER)
                .add_usage(vk::BufferUsageFlags::TRANSFER_DST),
        )?;
        rmg.record()
            .add_task(&mut ver_upload)
            .unwrap()
            .add_task(&mut ind_upload)
            .unwrap()
            .execute()?;

        let vertex_buffer = ver_upload.buffer;
        let index_buffer = ind_upload.buffer;

        let color_format = rmg
            .ctx
            .device
            .select_format(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_SRC,
                vk::ImageTiling::OPTIMAL,
                &[
                    vk::Format::R16G16B16A16_SFLOAT,
                    vk::Format::R32G32B32A32_SFLOAT,
                    vk::Format::R8G8B8A8_UNORM,
                ],
            )
            .unwrap();

        let depth_format = rmg
            .ctx
            .device
            .select_format(
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                vk::ImageTiling::OPTIMAL,
                &[
                    vk::Format::D32_SFLOAT,
                    vk::Format::D24_UNORM_S8_UINT,
                    vk::Format::D16_UNORM,
                ],
            )
            .unwrap();

        let color_image = rmg
            .new_image_uninitialized(
                ImgDesc::color_attachment_2d(
                    initial_framebuffer_size[0],
                    initial_framebuffer_size[1],
                    color_format,
                )
                .add_usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::STORAGE,
                ),
                Some("Target Image"),
            )
            .unwrap();
        let depth_desc = ImgDesc::depth_attachment_2d(
            initial_framebuffer_size[0],
            initial_framebuffer_size[1],
            depth_format,
        )
        .add_usage(vk::ImageUsageFlags::SAMPLED);
        let depth_image = rmg.new_image_uninitialized(depth_desc, None)?;

        let shader_module_vert = ShaderModule::new_from_bytes(&rmg.ctx.device, SHADER_VS).unwrap();
        let shader_module_frag = ShaderModule::new_from_bytes(&rmg.ctx.device, SHADER_FS).unwrap();

        let ressim = rmg.resource_handle(&simulation_buffer).unwrap();
        let resubo = rmg.resource_handle(&ubo).unwrap();
        let resvertex = rmg.resource_handle(&vertex_buffer).unwrap();

        let pass = rmg
            .new_raster_pass(
                rmg.new_raster_pipeline(
                    "main",
                    shader_module_vert,
                    "main",
                    shader_module_frag,
                    std::slice::from_ref(&color_format),
                    Some(depth_format),
                    |t| t,
                )
                .unwrap(),
            )
            .with_name("ForwardPass")
            .with_push_constant::<shared::ForwardPush>()
            .use_image(
                color_image.clone(),
                ImageUsage::ColorAttachment {
                    attachment_index: 0,
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    store_op: vk::AttachmentStoreOp::STORE,
                    clear_color: [0.1, 0.1, 0.6, 1.0],
                },
            )
            .unwrap()
            .use_image(
                depth_image.clone(),
                ImageUsage::DepthStencilAttachment {
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    store_op: vk::AttachmentStoreOp::STORE,
                    clear_depth: 1.0,
                },
            )
            .unwrap()
            .use_buffer(ubo.clone(), BufferUsage::Read)
            .use_buffer(simulation_buffer.clone(), BufferUsage::Read)
            .use_buffer(vertex_buffer.clone(), BufferUsage::Read)
            .draw(RasterDrawCall::Instanced {
                index_buffer: index_buffer.clone(),
                push_constant: shared::ForwardPush {
                    ubo: resubo,
                    sim: ressim,
                    vertex_buffer: resvertex,
                    ..Default::default()
                },
                instance_count: OBJECT_COUNT as u32,
            })
            .finish()
            .unwrap();

        Ok(ForwardPass {
            color_image,
            depth_image,
            sim_src: simulation_buffer,
            pass: Some(pass),
            index_buffer,
            vertex_buffer,
            ubo_buffer: ubo,
        })
    }

    ///Notifies the forward-pass that it got resized.
    pub fn notify_resize(&mut self, rmg: &mut Rmg, width: u32, height: u32) {
        //Only resize on actual change
        if self.pass.as_ref().unwrap().framebuffer_extent().width == width
            || self.pass.as_ref().unwrap().framebuffer_extent().height == height
        {
            return;
        }

        //we currently just setup a compleatly new pass based on the (static) data
        let cdesc = ImgDesc {
            extent: vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            ..self.color_image.image_desc().clone()
        };
        self.color_image = rmg
            .new_image_uninitialized(cdesc, Some("Forward Color"))
            .unwrap();

        let ddesc = ImgDesc {
            extent: vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            ..self.depth_image.image_desc().clone()
        };
        self.depth_image = rmg
            .new_image_uninitialized(ddesc, Some("Forward Depth"))
            .unwrap();

        let builder = self
            .pass
            .take()
            .unwrap()
            .reconfigure(rmg, false)
            //attach both new images
            .use_image(
                self.color_image.clone(),
                ImageUsage::ColorAttachment {
                    attachment_index: 0,
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    store_op: vk::AttachmentStoreOp::STORE,
                    clear_color: [0.1, 0.1, 0.6, 1.0],
                },
            )
            .unwrap()
            .use_image(
                self.depth_image.clone(),
                ImageUsage::DepthStencilAttachment {
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    store_op: vk::AttachmentStoreOp::STORE,
                    clear_depth: 1.0,
                },
            )
            .unwrap();
        self.pass = Some(self.standard_configure(builder).finish().unwrap());
    }

    ///Appends the standard draw call and buffer bindings
    fn standard_configure<'rmg>(
        &mut self,
        mut builder: RasterPassBuilder<'rmg, shared::ForwardPush>,
    ) -> RasterPassBuilder<'rmg, shared::ForwardPush> {
        let ressim = builder
            .on_rmg(|rmg| rmg.resource_handle(&self.sim_src))
            .unwrap();
        let resubo = builder
            .on_rmg(|rmg| rmg.resource_handle(&self.ubo_buffer))
            .unwrap();
        let resvertex = builder
            .on_rmg(|rmg| rmg.resource_handle(&self.vertex_buffer))
            .unwrap();

        builder
            //add both current buffers
            .use_buffer(self.ubo_buffer.clone(), BufferUsage::Read)
            .use_buffer(self.sim_src.clone(), BufferUsage::Read)
            .use_buffer(self.vertex_buffer.clone(), BufferUsage::Read)
            //and the draw call
            .draw(RasterDrawCall::Instanced {
                index_buffer: self.index_buffer.clone(),
                push_constant: shared::ForwardPush {
                    ubo: resubo,
                    sim: ressim,
                    vertex_buffer: resvertex,
                    ..Default::default()
                },
                instance_count: OBJECT_COUNT as u32,
            })
    }

    pub fn notify_simulation_buffer(
        &mut self,
        rmg: &mut Rmg,
        new_simulation_buffer: BufferHandle<SimObj>,
    ) {
        //overwrite
        self.sim_src = new_simulation_buffer;
        let builder = self.pass.take().unwrap().reconfigure(rmg, true);
        self.pass = Some(self.standard_configure(builder).finish().unwrap());
    }
}
