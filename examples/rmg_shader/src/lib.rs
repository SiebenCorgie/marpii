/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
#![cfg_attr(
    target_arch = "spirv",
    no_std,
    feature(register_attr),
    register_attr(spirv)
)]

// HACK(eddyb) can't easily see warnings otherwise from `spirv-builder` builds.
//#![deny(warnings)]

use shared::{SimPush, ForwardPush};
use spirv_std::{image::SampledImage, ByteAddressableBuffer, glam::{Vec4, UVec3, Vec3Swizzles}};

#[cfg(not(target_arch = "spirv"))]
use spirv_std::macros::spirv;

use spirv_std::{RuntimeArray, Image, Sampler};

/*
#[spirv(fragment)]
pub fn forward_main_fs(
    f_normal_world: Vec3,
    f_texcoord: Vec2,
    f_tangent_world: Vec4,
    f_position_world: Vec3,
    output: &mut Vec4,
    #[spirv(descriptor_set = 0, binding = 0)] storage_buffer: &mut RuntimeArray<u32>,
    #[spirv(descriptor_set = 1, binding = 0)] storage_images: &RuntimeArray<Image!(2D, type=f32, sampled=false)>,
    #[spirv(descriptor_set = 2, binding = 0)] sampled_images: &RuntimeArray<SampledImage<Image!(2D, type=f32, sampled)>>,
    #[spirv(descriptor_set = 3, binding = 0)] sampler: &mut RuntimeArray<Sampler>,
    #[spirv(descriptor_set = 4, binding = 0)] accel_structures: &RuntimeArray<Image!(2D, type=f32, sampled)>
) {

    *output = Vec4::ONE;
}


#[spirv(vertex)]
pub fn forward_main_vs(
    v_position_obj: Vec3,
    v_normal_obj: Vec3,
    v_tangent_obj: Vec4,
    v_texcoord: Vec2,
    #[spirv(push_constant)] push: &ForwardPush,
    #[spirv(position)] a_position: &mut Vec4,

    a_normal_world: &mut Vec3,
    a_texcoord: &mut Vec2,
    a_tangent_world: &mut Vec4,
    a_position_world: &mut Vec3,
) {
    let transform = Mat4::from_rotation_translation(
        Quat::from_array(push.rotation),
        Vec3::new(
            push.location_aspect[0],
            push.location_aspect[1],
            push.location_aspect[2],
        ),
    );

    //let transform = transform.inverse();

    let perspective =
        Mat4::perspective_lh(90.0f32.to_radians(), push.location_aspect[3], 0.001, 100.0);

    let transform = perspective * transform;

    let v_position_obj = v_position_obj * -1.0;
    let pos: Vec3 = transform.transform_point3(v_position_obj);
    let pos4 = transform * v_position_obj.extend(1.0);



    *a_normal_world = v_normal_obj;
    *a_texcoord = v_texcoord;
    *a_tangent_world = v_tangent_obj;
    *a_position_world = pos;
    *a_position = pos4;
}

*/
#[spirv(compute(threads(8,8,1)))]
pub fn forward_main(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(push_constant)] push: &ForwardPush,
    //#[spirv(storage_buffer, descriptor_set = 0, binding = 0)] storage_buffer: &mut RuntimeArray<ByteAddressableBuffer>,
    #[spirv(descriptor_set = 1, binding = 0)] storage_images: &RuntimeArray<Image!(2D, format=rgba32f, sampled=false)>,
    //#[spirv(descriptor_set = 2, binding = 0)] sampled_images: &RuntimeArray<SampledImage<Image!(2D, type=f32, sampled)>>,
    //#[spirv(descriptor_set = 3, binding = 0)] sampler: &mut RuntimeArray<Sampler>,
    //#[spirv(descriptor_set = 4, binding = 0)] accel_structures: &RuntimeArray<Image!(2D, type=f32, sampled)>
){

    let img = unsafe { storage_images.index(push.target_img as usize) };
    let a: Vec4 = img.read(id.xy());
    let res = a.max(Vec4::ONE).min(Vec4::ONE);
    unsafe{img.write(id.xy(), res)}
}
