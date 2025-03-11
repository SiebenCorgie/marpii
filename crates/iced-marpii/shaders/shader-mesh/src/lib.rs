//! Shader entry points.
//!
//! Contains an example vertex shader, fragment shader and one example compute
//! shader.
#![no_std]
#![allow(unexpected_cfgs)]
use glam::{UVec2, Vec2, Vec4, Vec4Swizzles};
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
    out_uv: &mut Vec2,
    #[spirv(position)] clip_pos: &mut Vec4,
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] index_buffer: &RuntimeArray<
        TypedBuffer<IndexBuffer>,
    >,
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] vertex_buffer: &RuntimeArray<
        TypedBuffer<VertexBuffer>,
    >,
) {
    //load the call
    let vindex = if push.index_buffer.is_valid() {
        let idxbuf = unsafe { index_buffer.index(push.index_buffer.index() as usize) };
        //load the vertex index from the index-buffer + offset
        idxbuf.indices[push.index_offset as usize + vertex_id as usize]
    } else {
        *out_uv = Vec2::ZERO;
        *clip_pos = Vec4::ZERO;
        return;
    };

    let vertex = if push.vertex_buffer.is_valid() {
        //use the just fetched vertex-index to get the vertex we are working on
        let vertbuf = unsafe { vertex_buffer.index(push.vertex_buffer.index() as usize) };
        &vertbuf.vertices[vindex as usize]
    } else {
        *out_uv = Vec2::ZERO;
        *clip_pos = Vec4::ZERO;
        return;
    };

    let mut pos = Vec2::from(vertex.pos);
    let uv = Vec2::from(vertex.uv);

    //offset and scale into pixel-space
    pos *= Vec2::from(push.scale);
    pos += Vec2::from(push.pos);
    //now translate into ndc
    let ndcpos = ((pos / (UVec2::from(push.resolution).as_vec2())) * 2.0) - 1.0;

    let vindex = vertex_id as usize % 6;
    *out_uv = UV_COORD_QUAD_CCW[vindex];
    *clip_pos = ndcpos.extend(0.0).extend(1.0)
}

/// Fragment shader that uses UV coords passed in from the vertex shader
/// to render a simple gradient.
#[spirv(fragment)]
pub fn fragment(_in_uv: Vec2, frag_color: &mut Vec4, #[spirv(push_constant)] push: &MeshPush) {
    *frag_color = Vec4::from(push.color);
}
