#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};
pub use marpii_rmg_shared::ResourceHandle;
pub use spirv_std;

///Generic vertex for the mesh-draming pass
#[repr(C)]
#[cfg_attr(
    not(target_arch = "spirv"),
    derive(Debug, Clone, Copy, Default, Pod, Zeroable)
)]
pub struct Vertex {
    ///2d float position
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

//A mesh draw call
#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug, Clone, Copy, Pod, Zeroable))]
pub struct MeshPush {
    pub ibuf: ResourceHandle,
    pub vbuf: ResourceHandle,

    //offset into the indexbuffer, where we find the index into the vertex-buffer
    pub index_offset: u32,
    //offset into the vertex_buffer, from where we can read relative indices.
    pub vertex_offset: u32,

    //resolution of the frame-buffer, used for translating
    //pixel-space to ndc
    pub resolution: [u32; 2],
    pad1: [u32; 2],

    pub pos: [f32; 2],
    pub scale: f32,
    pub layer_depth: f32,
}

impl Default for MeshPush {
    fn default() -> Self {
        MeshPush {
            ibuf: ResourceHandle::INVALID,
            vbuf: ResourceHandle::INVALID,
            index_offset: 0,
            vertex_offset: 0,
            resolution: [0; 2],
            pad1: [0; 2],
            pos: [0.0; 2],
            scale: 0.0,
            layer_depth: 0.0,
        }
    }
}
