//! Shader entry points.
//!
//! Contains an example vertex shader, fragment shader and one example compute
//! shader.
#![no_std]
#![allow(unexpected_cfgs)]
use glam::{UVec2, Vec2, Vec4, Vec4Swizzles};
use iced_marpii_shared::{CmdQuadGradient, QuadPush, gradient, spirv_std};
use spirv_std::glam;
use spirv_std::{RuntimeArray, TypedBuffer, spirv};

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

//NOTE: We roll our own box rendering algorithm.
//      - We use Inigo Quilez's https://www.shadertoy.com/view/4llXD7
//      distance function.
//      - for borders we use the classic abs(dist) - border_width trick
//      - For the shadow we offset the evaluation coords accordingly, and use the
//        dist value as a analytical blur which is clamped
fn sd_round_box(pos: Vec2, half_ext: Vec2, radii: Vec4) -> f32 {
    //select the left/righ radii
    let rlr = if pos.x > 0.0 { radii.xy() } else { radii.zw() };
    let rad = if pos.y > 0.0 { rlr.x } else { rlr.y };
    let q = pos.abs() - half_ext + rad;
    q.max_element().min(0.0) + q.max(Vec2::ZERO).length() - rad
}

/// Vertex shader that renders an implicit quad.
#[spirv(vertex)]
pub fn vertex(
    //input
    #[spirv(push_constant)] push: &QuadPush,
    #[spirv(vertex_index)] vertex_id: u32,
    #[spirv(instance_index)] instance_id: u32,
    //outputs
    #[spirv(position)] clip_pos: &mut Vec4,
    out_instance_index: &mut u32,
    out_border_color: &mut Vec4,
    out_border_width: &mut f32,
    out_shadow_color: &mut Vec4,
    out_shadow_offset: &mut Vec2,
    out_shadow_blur_radius: &mut f32,
    //bindless
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] draw_commands: &RuntimeArray<
        TypedBuffer<[CmdQuadGradient]>,
    >,
) {
    let cmd_offset = instance_id as usize;
    *out_instance_index = instance_id;
    //load the call
    let cmd = if push.cmd_buffer.is_valid() {
        let commands = unsafe { draw_commands.index(push.cmd_buffer.index() as usize) };
        &commands[cmd_offset]
    } else {
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
    let mut pixel_space_position = (Vec2::from(cmd.position) * push.scale)
        + (vertex_local_offset * Vec2::from(cmd.size) * push.scale);

    //grow, if there is a border
    if cmd.border_width > 0.0 {
        //-1 for 0, 1 for 1
        let offset_dir = (vertex_local_offset * 2.0) - Vec2::ONE;
        pixel_space_position = pixel_space_position + (offset_dir * cmd.border_width * push.scale);
    }

    //grow if there is a shadow
    if cmd.shadow_blur_radius > 0.0 {
        let offset_dir = (vertex_local_offset * 2.0) - Vec2::ONE;
        //NOTE: this might grow a little _too big_. But we can afford that I'd say :D.
        pixel_space_position =
            pixel_space_position + (offset_dir * Vec2::from(cmd.shadow_offset).abs() * push.scale);
    }

    let uv_pos = pixel_space_position / UVec2::from(push.resolution).as_vec2();

    //now translate to NDC which is [-1; 2] .. [1; 2]
    let ndc_pos = (uv_pos * Vec2::splat(2.0)) - Vec2::ONE;
    //to vec4
    let ndc_pos = ndc_pos.extend(push.layer_depth).extend(1.0);

    *clip_pos = ndc_pos;
    *out_border_color = Vec4::from(cmd.border_color);
    *out_border_width = push.scale * cmd.border_width;
    *out_shadow_color = Vec4::from(cmd.shadow_color);
    *out_shadow_offset = Vec2::from(cmd.shadow_offset) * push.scale;
    *out_shadow_blur_radius = cmd.shadow_blur_radius * push.scale;
}

/// Fragment shader that uses UV coords passed in from the vertex shader
/// to render a simple gradient.
#[spirv(fragment)]
pub fn fragment(
    //inputs
    #[spirv(push_constant)] push: &QuadPush,
    #[spirv(frag_coord)] in_frag_coord: Vec4,
    #[spirv(flat)] instance_id: u32,
    in_border_color: Vec4,
    in_border_width: f32,
    in_shadow_color: Vec4,
    in_shadow_offset: Vec2,
    in_shadow_blur_radius: f32,
    //outputs
    frag_color: &mut Vec4,
    //bindless
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] draw_commands: &RuntimeArray<
        TypedBuffer<[CmdQuadGradient]>,
    >,
) {
    //load the command
    let cmd = if push.cmd_buffer.is_valid() {
        let commands = unsafe { draw_commands.index(push.cmd_buffer.index() as usize) };
        &commands[instance_id as usize]
    } else {
        *frag_color = Vec4::X;
        return;
    };

    //unpack offsets (todo: that should actually unpack from f16 or something)
    let offsets = cmd.unpack_offsets();
    let colors = cmd.unpack_colors();
    //NOTE: we _could_ precalculate that on the CPU as well
    //iterate all offsets, to find the last offset, assuming the offsets
    //are _in-order_
    let mut last_index = 7;
    for i in 0..8 {
        if offsets[i] >= 1.0 {
            last_index = i;
            break;
        }
    }

    //Initial color, based on gradient
    let mut mixed_color = gradient(
        in_frag_coord.xy(),
        Vec4::from(cmd.direction),
        &colors,
        &offsets,
        last_index,
    );
    //find the distance to our rect for this fragment
    let half_extent = (Vec2::from(cmd.size) * 0.5) * push.scale;
    let box_center = (Vec2::from(cmd.position) * push.scale) + half_extent;
    let dist = sd_round_box(
        in_frag_coord.xy() - box_center,
        half_extent,
        Vec4::from(cmd.border_radius),
    );

    //Overall opacity based on the distance
    let base_opacity = dist.min(0.0).abs().clamp(0.0, 1.0);
    mixed_color.w = base_opacity;

    //maps dist=0.0..dist=0.0-border_width to 1..0
    //so its 1.0 if fully in border, and 0.0 if not in border at all.
    if in_border_width > 0.0 {
        //in case of border, remove the smooth step of the alpha,
        mixed_color.w = if dist <= 0.0 { 1.0 } else { 0.0 };
        //distance of the border ist just the good-old abs(d) - r trick
        //border_dist tells us _how much within the border_ we are with all negativ values,
        //and _how much from the border_ we are with the positive ones.
        //
        //NOTE on the 0.25: This basically grows edges to _at least_ 0.25, which basically makes
        //                  sure that they don't vanish for reaaallly small lines.
        let border_dist = dist.abs() - in_border_width - 0.25;
        //we now mix based on the inverse, clamped to 1.0
        let border_alpha = border_dist.min(0.0).abs().clamp(0.0, 1.0);
        let border_color = in_border_color.xyz();
        let border_weight = 1.0 - border_dist.clamp(-1.0, 0.0).abs();
        mixed_color = border_color
            .lerp(mixed_color.xyz(), border_weight)
            .extend(mixed_color.w.max(border_alpha));
    }

    //finally, handle shadow, if there is such a thing.
    //the idea is, similar to the rect itself, and the
    //borders to draw a _blured_ box _under_ this rect.
    //for that we first calculate the offsetted rectangle (via shadow_offset)
    //and then
    if in_shadow_blur_radius > 0.0 {
        //first, calculate the dist to the offseted rect
        let shadow_dist = sd_round_box(
            in_frag_coord.xy() - box_center - in_shadow_offset,
            half_extent,
            Vec4::from(cmd.border_radius) * push.scale,
        );

        //now calculate the color+opacity of the shadow
        let shadow_opacity =
            shadow_dist.min(0.0).abs().clamp(0.0, in_shadow_blur_radius) / in_shadow_blur_radius;
        let shadow_color = in_shadow_color.xyz().extend(shadow_opacity);
        //we now just mix based on the current color alpha. So IFF there
        //is already an opaque pixel, we'd use that color, otherwise we'd fate into whatever the shadow color is
        mixed_color = shadow_color.lerp(mixed_color, mixed_color.w);
    }

    //This _might_ apply gamma correction.
    *frag_color = push.color_to_display(mixed_color);
}
