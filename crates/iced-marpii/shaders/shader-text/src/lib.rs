//! Shader entry points.
//!
//! Contains an example vertex shader, fragment shader and one example compute
//! shader.
#![no_std]
#![allow(unexpected_cfgs)]
use glam::{UVec2, Vec2, Vec4, Vec4Swizzles};
use iced_marpii_shared::{spirv_std, GlyphInstance, TextPush};
use spirv_std::glam;
use spirv_std::{spirv, Image, RuntimeArray, Sampler, TypedBuffer};

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
    let tl = Vec2::new(0.0, 1.0);
    let tr = Vec2::new(1.0, 1.0);
    let bl = Vec2::new(0.0, 0.0);
    let br = Vec2::new(1.0, 0.0);
    [bl, br, tr, tr, tl, bl]
};

struct InstanceBuffer {
    data: [GlyphInstance; 1_0000_000],
}

/// Vertex shader that renders an implicit quad.
#[spirv(vertex)]
pub fn vertex(
    #[spirv(push_constant)] push: &TextPush,
    #[spirv(vertex_index)] vertex_id: u32,
    out_uv: &mut Vec2,
    out_instance_index: &mut u32,
    #[spirv(position)] clip_pos: &mut Vec4,
    #[spirv(instance_index)] instance_id: u32,
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] instance_data: &RuntimeArray<
        TypedBuffer<InstanceBuffer>,
    >,
) {
    //load instance data
    let instance_data_index = instance_id + push.instance_data_offset;
    let instance = if push.instance_data.is_valid() {
        let buffers = unsafe { instance_data.index(push.instance_data.index() as usize) };
        &buffers.data[instance_data_index as usize]
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
        Vec2::from(instance.pos) + vertex_local_offset * Vec2::from(instance.size);

    //now translate to NDC which is [-1; 2] .. [1; 2]
    let ndc_pos = ((pixel_space_position / UVec2::from(push.resolution).as_vec2())
        * Vec2::splat(2.0))
        - Vec2::ONE;
    //to vec4
    let ndc_pos = ndc_pos.extend(instance.layer_depth).extend(1.0);
    //..and write
    *clip_pos = ndc_pos;

    //calculate the uv coord in the atlas.
    //uv 0.0 .. 1.0
    let base_uv = UV_COORD_QUAD_CCW[vindex];

    //select resolution based on glyph type
    let atlas_resolution = if instance.glyph_type == 0 {
        UVec2::from(push.mask_atlas_resolution).as_vec2()
    } else {
        UVec2::from(push.mask_atlas_resolution).as_vec2()
    };

    //Calculates gylph's offset into the atlas texture, and size in that texture in uv-coords
    let atlas_uv_offset = UVec2::from(instance.atlas_offset).as_vec2() / atlas_resolution;
    let atlas_uv_size = UVec2::from(instance.atlas_size).as_vec2() / atlas_resolution;
    *out_uv = atlas_uv_offset + base_uv * atlas_uv_size;
    //also write back instance index for each fragment
    *out_instance_index = instance_id;
}

/// Fragment shader that uses UV coords passed in from the vertex shader
/// to render a simple gradient.
#[spirv(fragment)]
pub fn fragment(
    in_uv: Vec2,
    #[spirv(flat)] instance_id: u32,
    #[spirv(frag_coord)] in_frag_coord: Vec4,
    frag_color: &mut Vec4,
    #[spirv(push_constant)] push: &TextPush,
    #[spirv(descriptor_set = 0, binding = 0, storage_buffer)] instance_data: &RuntimeArray<
        TypedBuffer<InstanceBuffer>,
    >,
    #[spirv(descriptor_set = 2, binding = 0)] mask_atlas: &RuntimeArray<
        Image!(2D, sampled, type = f32),
    >,
    #[spirv(descriptor_set = 2, binding = 0)] color_atlas: &RuntimeArray<
        Image!(2D, sampled, type = f32),
    >,
    #[spirv(descriptor_set = 3, binding = 0)] sampler: &RuntimeArray<Sampler>,
) {
    //Load instance data, then select the correct texture  to sample, based on the gylph type
    let instance_data_index = instance_id + push.instance_data_offset;
    let instance = if push.instance_data.is_valid() {
        let buffers = unsafe { instance_data.index(push.instance_data.index() as usize) };
        &buffers.data[instance_data_index as usize]
    } else {
        *frag_color = Vec4::new(1.0, 0.0, 0.0, 1.0);
        return;
    };

    let fragcoord = in_frag_coord.xy();

    let clip_bound_start = Vec2::from(instance.clip_offset);
    let clip_size = Vec2::from(instance.clip_size);

    ///If the point is outside of the clip bound, bail
    if fragcoord.cmplt(clip_bound_start).any()
        || fragcoord.cmpgt(clip_bound_start + clip_size).any()
    {
        *frag_color = Vec4::new(0.0, 0.0, 0.0, 0.0);
        return;
    }

    //bail if either the sampler or the chosen texture is invalid.
    if push.glyph_sampler.is_invalid() {
        *frag_color = Vec4::new(1.0, 0.0, 0.0, 1.0);
        return;
    }

    let color = if instance.glyph_type == 0 {
        if push.glyph_atlas_alpha.is_invalid() {
            *frag_color = Vec4::new(1.0, 0.0, 0.0, 1.0);
            return;
        }
        //sample texture
        let image = unsafe { mask_atlas.index(push.glyph_atlas_alpha.index() as usize) };
        let sampler = unsafe { sampler.index(push.glyph_sampler.index() as usize) };
        let tex_val: Vec4 = image.sample(*sampler, in_uv);
        let mut color = Vec4::from(instance.color);
        color.w = tex_val.x;
        color
    } else {
        if push.glyph_atlas_color.is_invalid() {
            *frag_color = Vec4::new(1.0, 0.0, 0.0, 1.0);
            return;
        }
        //sample texture
        let image = unsafe { color_atlas.index(push.glyph_atlas_color.index() as usize) };
        let sampler = unsafe { sampler.index(push.glyph_sampler.index() as usize) };
        let tex_val: Vec4 = image.sample(*sampler, in_uv);
        tex_val
    };

    //and write back
    *frag_color = color;
}
