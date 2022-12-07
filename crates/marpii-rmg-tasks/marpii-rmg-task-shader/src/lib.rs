#![cfg_attr(target_arch = "spirv", no_std)]

use marpii_rmg_task_shared::glam::{vec4, Vec2, Vec3, Vec4, Vec4Swizzles, UVec3, Vec3Swizzles, UVec2};
use spirv_std::{self, Image, RuntimeArray, Sampler};

//include spirv macro
use spirv_std::spirv;


/*
#[spirv(compute(threads(8, 8, 1)))]
pub fn alphablendf(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(push_constant)] push: &marpii_rmg_task_shared::AlphaBlendPush,
    #[spirv(descriptor_set = 1, binding = 0)] storage_images: &RuntimeArray<
        Image!(2D, type=f32, sampled=false),
    >,
) {
    if push.add.is_invalid() || push.dst.is_invalid(){
        return;
    }

    let thread_id = id.xy();
    if thread_id.x >= push.extent[0] || thread_id.y > push.extent[1]{
        //early return outside of image
        return;
    }

    let add = unsafe{storage_images.index(push.add.index() as usize)};
    let dst = unsafe{storage_images.index(push.dst.index() as usize)};

    let add_size: UVec2 = add.query_size();
    if add_size.x < thread_id.x || add_size.y < thread_id.y{
        return;
    }

    let dst_size: UVec2 = dst.query_size();
    if dst_size.x < thread_id.x || dst_size.y < thread_id.y{
        return;
    }


    //read both image if valid
    let add_val: Vec4 = add.read(thread_id);
    let dst_val: Vec4 = dst.read(thread_id);

    //mix and store
    let mix = dst_val.lerp(add_val, add_val.w);
    unsafe{dst.write(thread_id, mix)};

}
*/
#[allow(dead_code)]
fn srgb_from_linear(rgb: Vec3) -> Vec3 {
    let lower = rgb * Vec3::splat(3294.6);
    let higher = Vec3::splat(269.025) * rgb.powf(1.0 / 2.4) - Vec3::splat(14.025);
    Vec3::new(
        if rgb.x < 0.0031308 { lower.x } else { higher.x },
        if rgb.y < 0.0031308 { lower.y } else { higher.y },
        if rgb.z < 0.0031308 { lower.z } else { higher.z },
    )
}

#[allow(dead_code)]
fn srgba_from_linear(rgba: Vec4) -> Vec4 {
    let rgb = srgb_from_linear(rgba.xyz());
    Vec4::new(rgb.x, rgb.y, rgb.z, 255.0 * rgba.w)
}

#[allow(dead_code)]
fn gamma_from_linear_rgba(linear_rgba: Vec4) -> Vec4 {
    let srgb = srgb_from_linear(linear_rgba.xyz()) / 255.0;
    Vec4::new(srgb.x, srgb.y, srgb.z, linear_rgba.w)
}

fn srgb_to_linear(srgb: Vec3) -> Vec3 {
    let lower = srgb / Vec3::splat(12.92);
    let higher = ((srgb + Vec3::splat(0.055)) / Vec3::splat(1.055)).powf(2.4);
    Vec3::new(
        if srgb.x < 0.0031308 {
            lower.x
        } else {
            higher.x
        },
        if srgb.y < 0.0031308 {
            lower.y
        } else {
            higher.y
        },
        if srgb.z < 0.0031308 {
            lower.z
        } else {
            higher.z
        },
    )
}

#[spirv(fragment)]
pub fn egui_fs(
    in_rgba_gamma: Vec4,
    in_v_tc: Vec2,
    output: &mut Vec4,
    #[spirv(push_constant)] push: &marpii_rmg_task_shared::EGuiPush,
    #[spirv(descriptor_set = 2, binding = 0)] sampled_images: &RuntimeArray<
        Image!(2D, format=rgba8, sampled),
    >,
    #[spirv(descriptor_set = 3, binding = 0)] sampler: &RuntimeArray<Sampler>,
) {
    if push.texture.is_invalid() || push.sampler.is_invalid() {
        *output = Vec4::ZERO;
        return;
    }

    let image = unsafe { sampled_images.index(push.texture.index() as usize) };
    let sampler = unsafe { sampler.index(push.sampler.index() as usize) };
    let tex_val: Vec4 = image.sample(*sampler, in_v_tc);

    let texture_in_gamma = srgb_to_linear(tex_val.xyz()).extend(tex_val.w);
    let rgba_gamma = texture_in_gamma * in_rgba_gamma;
    *output = rgba_gamma;
}

#[spirv(vertex)]
pub fn egui_vs(
    v_pos: Vec2,
    v_uv: Vec2,
    v_color: Vec4,
    #[spirv(position)] out_pos: &mut Vec4,
    #[spirv(push_constant)] push: &marpii_rmg_task_shared::EGuiPush,
    out_rgba_gamma: &mut Vec4,
    out_v_tc: &mut Vec2,
) {
    //let d = v_position_obj.x + v_normal_obj.x + v_texcoord.x;
    //let d = d.min(0.0).max(0.0);
    *out_pos = vec4(
        (2.0 * v_pos.x / push.screen_size[0]) - 1.0,
        (2.0 * v_pos.y / push.screen_size[1]) - 1.0,
        0.0,
        1.0,
    );

    *out_rgba_gamma = v_color;
    *out_v_tc = v_uv;
}
