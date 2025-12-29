use iced_graphics::{Mesh, Settings};
use iced_marpii_shared::Vertex;
use marpii::ash::vk;
use marpii_rmg::{ImageHandle, MetaTask, Rmg};
use marpii_rmg_tasks::DynamicBuffer;

use crate::util::clip_to_rect2d;

mod cache;
mod pass;

pub struct LayerMesh {
    pub vertex_buffer: Vec<Vertex>,
    pub index_buffer: Vec<u32>,
    pub scale: f32,
    pub translation: [f32; 2],
    pub clip: vk::Rect2D,
}

impl From<&iced_graphics::Mesh> for LayerMesh {
    fn from(value: &iced_graphics::Mesh) -> Self {
        match value {
            iced_graphics::Mesh::Solid {
                buffers,
                transformation,
                clip_bounds,
            } => LayerMesh {
                vertex_buffer: buffers
                    .vertices
                    .iter()
                    .map(|v| Vertex {
                        pos: v.position,
                        color: v.color.components(),
                        uv: [0.0; 2],
                    })
                    .collect(),
                index_buffer: buffers.indices.clone(),
                scale: transformation.scale_factor(),
                translation: transformation.translation().into(),
                clip: clip_to_rect2d(*clip_bounds),
            },
            iced_graphics::Mesh::Gradient {
                buffers,
                transformation,
                clip_bounds,
            } => {
                //TODO: interpolate the vertex color based on the gradient?
                //      and then just let the linear-interpolation on the hardware do its thing.
                log::error!("Mesh gradient not supported!");
                LayerMesh {
                    vertex_buffer: buffers
                        .vertices
                        .iter()
                        .map(|v| Vertex {
                            pos: v.position,
                            color: [1.0, 0.0, 0.0, 1.0],
                            uv: [0.0; 2],
                        })
                        .collect(),
                    index_buffer: buffers.indices.clone(),
                    scale: transformation.scale_factor(),
                    translation: transformation.translation().into(),
                    clip: clip_to_rect2d(*clip_bounds),
                }
            }
        }
    }
}

pub type Batch = Vec<Mesh>;

#[derive(Debug)]
struct BatchRecord {
    ///The offset for this batch into the shared index-buffer.
    index_offset: u32,
    vertex_offset: u32,
    index_count: u32,
    layer_height: f32,
    translation: [f32; 2],
    bound: vk::Rect2D,
    scale: f32,
}

///Mesh-Layer renderer.
///
///Contrary to the wgpu implementation this renderer unifies
///gradient and non-gradient meshes into one single draw-call.
pub struct MeshRenderer {
    ///The buffer-pipeline we use for uploads
    vertex_buffer: DynamicBuffer<Vertex>,
    index_buffer: DynamicBuffer<u32>,

    render_pass: pass::MeshPass,

    vertex_cache: Vec<Vertex>,
    index_cache: Vec<u32>,
}

impl MeshRenderer {
    const INITIAL_BUFFER_SIZE: usize = 512;
    pub fn new(
        rmg: &mut Rmg,
        settings: &Settings,
        color_image: ImageHandle,
        depth_image: ImageHandle,
    ) -> Self {
        let vertex_buffer =
            DynamicBuffer::new(rmg, &[Vertex::default(); Self::INITIAL_BUFFER_SIZE]).unwrap();
        let index_buffer = DynamicBuffer::new(rmg, &[0; Self::INITIAL_BUFFER_SIZE]).unwrap();
        let pass = pass::MeshPass::new(
            rmg,
            settings,
            color_image,
            depth_image,
            vertex_buffer.buffer_handle().clone(),
            index_buffer.buffer_handle().clone(),
        );

        Self {
            vertex_buffer,
            index_buffer,
            render_pass: pass,
            vertex_cache: Vec::new(),
            index_cache: Vec::new(),
        }
    }

    pub fn new_frame(&mut self) {
        self.render_pass.batches.clear();
        self.vertex_cache.clear();
        self.index_cache.clear();
    }

    pub fn notify_resize(&mut self, color_buffer: ImageHandle, depth_buffer: ImageHandle) {
        self.render_pass.resize(color_buffer, depth_buffer);
    }

    pub fn push_mesh_batch(&mut self, batch: &Batch, layer_height: f32) {
        for mesh in batch {
            let index_offset = self.index_cache.len().try_into().unwrap();
            let vertex_offset = self.vertex_cache.len().try_into().unwrap();

            let mut converted = LayerMesh::from(mesh);
            let record = BatchRecord {
                index_count: converted.index_buffer.len().try_into().unwrap(),
                vertex_offset,
                index_offset,
                layer_height,
                translation: converted.translation,
                bound: converted.clip,
                scale: converted.scale,
            };

            //now push everything
            self.render_pass.batches.push(record);
            self.vertex_cache.append(&mut converted.vertex_buffer);
            self.index_cache.append(&mut converted.index_buffer);
        }
    }

    pub fn prepare(&mut self, rmg: &mut Rmg) {
        //Either reuse, or create new dynamic buffer
        if self.vertex_buffer.element_count() < self.vertex_cache.len() {
            //create new dynamic buffer
            self.vertex_buffer = DynamicBuffer::new(rmg, &self.vertex_cache).unwrap();
            self.render_pass.vertex_buffer = self.vertex_buffer.buffer_handle().clone();
        } else {
            self.vertex_buffer.write(&self.vertex_cache, 0).unwrap();
        }
        //same for index data
        if self.index_buffer.element_count() < self.index_cache.len() {
            //create new dynamic buffer
            self.index_buffer = DynamicBuffer::new(rmg, &self.index_cache).unwrap();
            self.render_pass.index_buffer = self.index_buffer.buffer_handle().clone();
        } else {
            self.index_buffer.write(&self.index_cache, 0).unwrap();
        }

        rmg.record()
            .add_task(&mut self.vertex_buffer)
            .unwrap()
            .add_task(&mut self.index_buffer)
            .unwrap()
            .execute()
            .unwrap();
    }

    pub fn end_frame(&mut self) {
        //currently not caching, so ignoring
    }
}

impl MetaTask for MeshRenderer {
    fn record<'a>(
        &'a mut self,
        recorder: marpii_rmg::Recorder<'a>,
    ) -> Result<marpii_rmg::Recorder<'a>, marpii_rmg::RecordError> {
        recorder.add_task(&mut self.render_pass)
    }
}
