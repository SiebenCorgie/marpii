//! Shader entry points.
//!
//! Contains an example vertex shader, fragment shader and one example compute
//! shader.
#![no_std]
#![allow(unexpected_cfgs)]
use glam::{UVec2, Vec2, Vec3, Vec4, Vec4Swizzles};
use iced_marpii_shared::{spirv_std, MeshPush, Vertex};
use spirv_std::glam;
use spirv_std::{spirv, RuntimeArray, TypedBuffer};

#[cfg(target_arch = "spirv")]
use iced_marpii_shared::spirv_std::num_traits::Float;

pub const UV_COORD_QUAD_CCW: [Vec2; 6] = {
    let tl = Vec2::new(0.0, 0.0);
    let tr = Vec2::new(1.0, 0.0);
    let bl = Vec2::new(0.0, 1.0);
    let br = Vec2::new(1.0, 1.0);
    [bl, br, tr, tr, tl, bl]
};

//NOTE: hack till rust-gpu supports unbound arrays like this: TypeBuffer<[u32]>
struct IndexBuffer {
    indices: [u32; 1_0000_000],
}
struct VertexBuffer {
    vertices: [Vertex; 1_0000_000],
}

/// Vertex shader that renders an implicit quad.
#[spirv(vertex)]
pub fn vertex(
    #[spirv(push_constant)] push: &MeshPush,
    #[spirv(vertex_index)] vertex_id: u32,

    out_color: &mut Vec4,
    //out_uv: &mut Vec2,
    #[spirv(position)] clip_pos: &mut Vec4,
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] index_buffer: &RuntimeArray<
        TypedBuffer<IndexBuffer>,
    >,
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] vertex_buffer: &RuntimeArray<
        TypedBuffer<VertexBuffer>,
    >,
) {
    if push.ibuf.is_invalid() || push.vbuf.is_invalid() {
        *out_color = Vec3::X.extend(1.0);
        //*out_uv = Vec2::ZERO;
        *clip_pos = Vec3::ZERO.extend(1.0);
        return;
    }

    //Load the vertex_buffer_relative offset
    let ibuffers = unsafe { index_buffer.index(push.ibuf.index() as usize) };
    //load the index by offsetting based on the push constant into the global buffer,
    //and then adding
    let relative_offset = ibuffers.indices[push.index_offset as usize + vertex_id as usize];

    //now offset into the vertex buffer based on the global offset, and the local offset
    let vbuffers = unsafe { vertex_buffer.index(push.vbuf.index() as usize) };
    let vertex = &vbuffers.vertices[(push.vertex_offset + relative_offset) as usize];

    let mut pos: Vec2 = vertex.pos.into();
    //let uv: Vec2 = vertex.uv.into();
    let color: Vec4 = vertex.color.into();

    //offset and scale into pixel-space
    pos *= Vec2::splat(push.scale);
    pos += Vec2::from(push.pos);
    //now translate into ndc
    let ndcpos = ((pos / (UVec2::from(push.resolution).as_vec2())) * 2.0) - 1.0;
    *clip_pos = ndcpos.extend(push.layer_depth).extend(1.0);
    //*out_uv = uv;
    *out_color = color;
}

/// Fragment shader that uses UV coords passed in from the vertex shader
/// to render a simple gradient.
#[spirv(fragment)]
pub fn fragment(
    //in_uv: Vec2,
    in_color: Vec4,
    frag_color: &mut Vec4,
    #[spirv(push_constant)] push: &MeshPush,
) {
    *frag_color = Vec4::from(in_color);
}
