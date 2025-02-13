//! Shader entry points.
//!
//! Contains an example vertex shader, fragment shader and one example compute
//! shader.
#![no_std]
#![allow(unexpected_cfgs)]
use glam::{UVec2, Vec2, Vec4, Vec4Swizzles};
use iced_marpii_shared::{spirv_std, TextPush};
use spirv_std::glam;
use spirv_std::{spirv, RuntimeArray, TypedBuffer};

#[cfg(target_arch = "spirv")]
use iced_marpii_shared::spirv_std::num_traits::Float;

pub const VERTEX_OFFSETS: [Vec2; 6] = {
    let tl = Vec2::new(0.0, 1.0);
    let tr = Vec2::new(1.0, 1.0);
    let bl = Vec2::new(0.0, 0.0);
    let br = Vec2::new(1.0, 0.0);
    [bl, br, tr, tr, tl, bl]
};

pub const UV_COORD_QUAD_CCW: [Vec2; 6] = {
    let tl = Vec2::new(0.0, 0.0);
    let tr = Vec2::new(1.0, 0.0);
    let bl = Vec2::new(0.0, 1.0);
    let br = Vec2::new(1.0, 1.0);
    [bl, br, tr, tr, tl, bl]
};

/// Vertex shader that renders an implicit quad.
#[spirv(vertex)]
pub fn vertex(
    #[spirv(push_constant)] push: &TextPush,
    #[spirv(vertex_index)] vertex_id: u32,
    out_uv: &mut Vec2,
    #[spirv(position)] clip_pos: &mut Vec4,
) {
    //We calculate the vertex-position in pixel space
    // [0.0; 2] ..[1.0; 2]
    //
    // Once we are done, we translate it into ndc
    //
    let vindex = vertex_id as usize % 6;
    let vertex_local_offset = VERTEX_OFFSETS[vindex];
    //this is the vertex's locatio in pixel space
    let pixel_space_position = Vec2::from(push.pos) + vertex_local_offset * Vec2::from(push.size);

    //now translate to NDC which is [-1; 2] .. [1; 2]
    let ndc_pos = ((pixel_space_position / UVec2::from(push.resolution).as_vec2())
        * Vec2::splat(2.0))
        - Vec2::ONE;
    //to vec4
    let ndc_pos = ndc_pos.extend(0.0).extend(1.0);

    *out_uv = UV_COORD_QUAD_CCW[vindex];
    *clip_pos = ndc_pos;
}

/// Fragment shader that uses UV coords passed in from the vertex shader
/// to render a simple gradient.
#[spirv(fragment)]
pub fn fragment(
    _in_uv: Vec2,
    #[spirv(frag_coord)] in_frag_coord: Vec4,
    frag_color: &mut Vec4,
    #[spirv(push_constant)] push: &TextPush,
    //#[spirv(descriptor_set = 0, binding = 0, storage_buffer)] draw_commands: &RuntimeArray<
    //TypedBuffer<QuadCmdBuffer>,
    //>,
) {
    *frag_color = Vec4::from(push.color);
}
