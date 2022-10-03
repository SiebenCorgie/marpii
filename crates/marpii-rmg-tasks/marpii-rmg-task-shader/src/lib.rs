#![cfg_attr(
    target_arch = "spirv",
    feature(register_attr),
    register_attr(spirv),
    no_std
)]

use marpii_rmg_task_shared::glam::{vec4, Vec4};
use spirv_std;
#[cfg(not(target_arch = "spirv"))]
use spirv_std::macros::spirv;


#[spirv(compute(threads(8, 8, 1)))]
pub fn compute_shader(
) {
}


#[spirv(fragment)]
pub fn main_fs(output: &mut Vec4) {
    *output = vec4(1.0, 0.0, 0.0, 1.0);
}

#[spirv(vertex)]
pub fn main_vs(
    //v_position_obj: Vec3,
    //v_normal_obj: Vec3,
    //v_texcoord: Vec2,
    #[spirv(vertex_index)] vert_id: i32,
    #[spirv(position, invariant)] out_pos: &mut Vec4,
) {
    //let d = v_position_obj.x + v_normal_obj.x + v_texcoord.x;
    //let d = d.min(0.0).max(0.0);
    *out_pos = vec4(
        (vert_id - 1) as f32,
        ((vert_id & 1) * 2 - 1) as f32,
        0.0,
        1.0,
    );
}
