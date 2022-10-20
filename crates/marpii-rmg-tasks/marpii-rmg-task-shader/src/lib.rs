#![cfg_attr(
    target_arch = "spirv",
    feature(register_attr),
    register_attr(spirv),
    no_std
)]

use marpii_rmg_task_shared::glam::{vec4, Vec4, Vec2, Vec3, BVec3, vec3, Vec4Swizzles};
use spirv_std::{self, Sampler, RuntimeArray, image::SampledImage, Image};
#[cfg(not(target_arch = "spirv"))]
use spirv_std::macros::spirv;


#[spirv(compute(threads(8, 8, 1)))]
pub fn compute_shader(
) {
}

fn srgb_from_linear(rgb: Vec3) -> Vec3{
    let lower = rgb * Vec3::splat(3294.6);
    let higher = Vec3::splat(269.025) * rgb.powf(1.0/2.4) - Vec3::splat(14.025);
    Vec3::new(
        if rgb.x < 0.0031308{lower.x}else{higher.x},
        if rgb.y < 0.0031308{lower.y}else{higher.y},
        if rgb.z < 0.0031308{lower.z}else{higher.z},
    )
}

fn srgba_from_linear(rgba: Vec4) -> Vec4{
    let rgb = srgb_from_linear(rgba.xyz());
    Vec4::new(rgb.x, rgb.y, rgb.z, 255.0 * rgba.w)
}

fn gamma_from_linear_rgba(linear_rgba: Vec4) -> Vec4{
    let srgb = srgb_from_linear(linear_rgba.xyz()) / 255.0;
    Vec4::new(srgb.x, srgb.y, srgb.z, linear_rgba.w)
}

#[spirv(fragment)]
pub fn egui_fs(
    in_rgba_gamma: Vec4,
    in_v_tc: Vec2,
    output: &mut Vec4,
    #[spirv(push_constant)] push: &marpii_rmg_task_shared::EGuiPush,
    #[spirv(descriptor_set = 2, binding = 0)] sampled_images: &RuntimeArray<Image!(2D, format=rgba8, sampled)>,
    #[spirv(descriptor_set = 3, binding = 0)] sampler: &RuntimeArray<Sampler>,
) {
    let image = unsafe{sampled_images.index(push.texture.index() as usize)};
    let sampler = unsafe{sampler.index(push.sampler.index() as usize)};
    let tex_val = image.sample(*sampler, in_v_tc);

    let texture_in_gamma = gamma_from_linear_rgba(tex_val);
    *output = in_rgba_gamma * texture_in_gamma;
}

#[spirv(vertex)]
pub fn egui_vs(
    v_pos: Vec2,
    v_uv: Vec2,
    v_color: Vec4,
    #[spirv(position)] out_pos: &mut Vec4,
    #[spirv(push_constant)] push: &marpii_rmg_task_shared::EGuiPush,
    out_rgba_gamma: &mut Vec4,
    out_v_tc: &mut Vec2
) {
    //let d = v_position_obj.x + v_normal_obj.x + v_texcoord.x;
    //let d = d.min(0.0).max(0.0);
    *out_pos = vec4(
        (2.0 * v_pos.x / push.screen_size[0])  - 1.0,
        (2.0 * v_pos.y / push.screen_size[1]) - 1.0,
        0.0,
        1.0,
    );

    *out_rgba_gamma = v_color;
    *out_v_tc = v_uv;
}
