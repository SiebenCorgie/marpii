/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
#![cfg_attr(
    target_arch = "spirv",
    feature(register_attr),
    register_attr(spirv),
    no_std
)]
// HACK(eddyb) can't easily see warnings otherwise from `spirv-builder` builds.
#![deny(warnings)]

use spirv_std;
use spirv_std::glam::{UVec3, Vec3, Vec3Swizzles};
use spirv_std::Image;

//Note this is needed to compile on cpu
#[cfg(not(target_arch = "spirv"))]
use spirv_std::macros::spirv;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PushConst {
    single: f32,
    color: [f32; 3],
}

#[spirv(compute(threads(8, 8, 1)))]
pub fn main(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(push_constant)] push: &PushConst,
    #[spirv(descriptor_set = 0, binding = 0)] target_image: &Image!(2D, format=rgba32f, sampled=false),
) {
    //fake a triangle via 2d sdf

    let color = Vec3::from(push.color);
    unsafe {
        target_image.write(id.xy(), color.extend(1.0));
    }
}
