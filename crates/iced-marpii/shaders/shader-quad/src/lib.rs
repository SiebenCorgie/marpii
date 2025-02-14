//! Shader entry points.
//!
//! Contains an example vertex shader, fragment shader and one example compute
//! shader.
#![no_std]
#![allow(unexpected_cfgs)]
use glam::{UVec2, Vec2, Vec4, Vec4Swizzles};
use iced_marpii_shared::{spirv_std, QuadCmdBuffer, QuadPush};
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

fn distance_quad(frag_coord: Vec2, position: Vec2, size: Vec2, radius: f32) -> f32 {
    let inner_half = (size - Vec2::splat(radius) * 2.0) / 2.0;
    let top_left = position + Vec2::splat(radius);
    sdf_rounded_box(frag_coord - top_left - inner_half, inner_half, 0.0)
}

fn sdf_rounded_box(to_center: Vec2, size: Vec2, radius: f32) -> f32 {
    (to_center.abs() - size + Vec2::splat(radius))
        .max(Vec2::ZERO)
        .length()
        - radius
}

// Order matches CSS border radius attribute:
// radii.x = top-left, radii.y = top-right, radii.z = bottom-right, radii.w = bottom-left
fn select_border_radius(radii: Vec4, position: Vec2, center: Vec2) -> f32 {
    let dx = position.x < center.x;
    let dy = position.y < center.y;

    match (dx, dy) {
        (true, true) => radii.x,
        (true, false) => radii.w,
        (false, true) => radii.y,
        (false, false) => radii.z,
    }
}

/// Vertex shader that renders an implicit quad.
#[spirv(vertex)]
pub fn vertex(
    #[spirv(push_constant)] push: &QuadPush,
    #[spirv(vertex_index)] vertex_id: u32,
    out_uv: &mut Vec2,
    #[spirv(position)] clip_pos: &mut Vec4,
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] draw_commands: &RuntimeArray<
        TypedBuffer<QuadCmdBuffer>,
    >,
) {
    //load the call
    let cmd = if push.cmd_buffer.is_valid() {
        let buffers = unsafe { draw_commands.index(push.cmd_buffer.index() as usize) };
        &buffers.cmds[push.offset as usize]
    } else {
        *out_uv = Vec2::ZERO;
        *clip_pos = Vec4::ZERO;
        return;
    };

    //We calculate the vertex-position in pixel space
    // [0.0; 2] ..[1.0; 2]
    //
    // Once we are done, we translate it into ndc
    //
    let vindex = vertex_id as usize % 6;
    let vertex_local_offset = VERTEX_OFFSETS[vindex];
    //this is the vertex's locatio in pixel space
    let pixel_space_position =
        Vec2::from(cmd.position) + vertex_local_offset * Vec2::from(cmd.size);

    //now translate to NDC which is [-1; 2] .. [1; 2]
    let ndc_pos = ((pixel_space_position / UVec2::from(push.resolution).as_vec2())
        * Vec2::splat(2.0))
        - Vec2::ONE;
    //to vec4
    let ndc_pos = ndc_pos.extend(push.layer_depth).extend(1.0);

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
    #[spirv(push_constant)] push: &QuadPush,
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] draw_commands: &RuntimeArray<
        TypedBuffer<QuadCmdBuffer>,
    >,
) {
    //load the command

    let cmd = if push.cmd_buffer.is_valid() {
        let buffers = unsafe { draw_commands.index(push.cmd_buffer.index() as usize) };
        &buffers.cmds[push.offset as usize]
    } else {
        *frag_color = Vec4::X;
        return;
    };

    //Initial color
    let mixed_color = Vec4::from(cmd.color);

    let box_center = Vec2::from(cmd.position) + (Vec2::from(cmd.size) * 0.5);

    let border_radius = select_border_radius(
        Vec4::from(cmd.border_radius),
        in_frag_coord.xy(),
        box_center,
    );

    let dist = distance_quad(
        in_frag_coord.xy(),
        Vec2::from(cmd.position),
        Vec2::from(cmd.size),
        border_radius,
    );

    let radius_alpha = 1.0
        - iced_marpii_shared::smoothstep((border_radius - 0.5).max(0.0), border_radius + 0.5, dist);

    let mut quad_color = mixed_color;
    quad_color.w = quad_color.w * radius_alpha;

    *frag_color = quad_color;
}
