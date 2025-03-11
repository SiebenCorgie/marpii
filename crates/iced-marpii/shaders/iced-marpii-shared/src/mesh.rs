#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};
pub use marpii_rmg_shared::ResourceHandle;
pub use spirv_std;

//A mesh's vertex
#[repr(C)]
#[cfg_attr(
    not(target_arch = "spirv"),
    derive(Debug, Clone, Copy, Default, Pod, Zeroable)
)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
}

//A mesh draw call
#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug, Clone, Copy, Pod, Zeroable))]
pub struct MeshPush {
    pub index_buffer: ResourceHandle,
    pub vertex_buffer: ResourceHandle,
    //offset into the indexbuffer, where we find the index into the vertex-buffer
    pub index_offset: u32,
    pub layer_depth_float: u32,
    //resolution of the frame-buffer, used for translating
    //pixel-space to ndc
    pub resolution: [u32; 2],
    pad1: [u32; 2],
    //Mesh wide color
    pub color: [f32; 4],
    pub pos: [f32; 2],
    pub scale: [f32; 2],
}

impl Default for MeshPush {
    fn default() -> Self {
        MeshPush {
            index_buffer: ResourceHandle::INVALID,
            vertex_buffer: ResourceHandle::INVALID,
            index_offset: 0,
            layer_depth_float: 0,
            resolution: [0; 2],
            pad1: [0; 2],
            color: [0.0; 4],
            pos: [0.0; 2],
            scale: [0.0; 2],
        }
    }
}
