/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
#![cfg_attr(target_arch = "spirv", no_std)]
// HACK(eddyb) can't easily see warnings otherwise from `spirv-builder` builds.
#![deny(warnings)]

use spirv_std::glam::{UVec3, Vec2, Vec3, Vec3Swizzles};
use spirv_std::Image;

//spirv macro
use spirv_std::spirv;

#[cfg(target_arch = "spirv")]
use spirv_std::num_traits::Float;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PushConst {
    radius: f32,
    opening: f32, //in radians
    offset: [f32; 2],
}

fn sign(f: f32) -> f32 {
    if f > 0.0 {
        1.0
    } else if f == 0.0 {
        0.0
    } else {
        -1.0
    }
}

fn fabs(f: f32) -> f32 {
    if f < 0.0 {
        -f
    } else {
        f
    }
}

/* Snacked from: https://www.shadertoy.com/view/3tGXRc under
// The MIT License
// Copyright © 2021 Inigo Quilez
// .x = f(p)
// .y = ∂f(p)/∂x
// .z = ∂f(p)/∂y
// .yz = ∇f(p) with ‖∇f(p)‖ = 1
// c is the sin/cos of the angle. r is the radius
*/
fn sdg_pie(mut p: Vec2, c: Vec2, r: f32) -> GradientResult {
    let s = sign(p.x);
    p.x = p.x.abs();

    let l = p.length();
    let n = l - r;
    let q = p - c * (p.dot(c)).clamp(0.0, r);
    let m = q.length() * sign(c.y * p.x - c.x * p.y);

    let res = if n > m {
        Vec3::new(n, p.x / l, p.y / l)
    } else {
        Vec3::new(m, q.x / m, q.y / m)
    };

    GradientResult(Vec3::new(res.x, s * res.y, res.z))
}

struct GradientResult(Vec3);

#[spirv(compute(threads(8, 8, 1)))]
pub fn main(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(push_constant)] push: &PushConst,
    #[spirv(descriptor_set = 0, binding = 0)] target_image: &Image!(2D, format=rgba32f, sampled=false),
) {
    //fake a triangle via 2d sdf
    let mut coord = id.xy().as_vec2();
    coord += -Vec2::from(push.offset);
    let c = Vec2::new(push.opening.sin(), push.opening.cos());

    let res = sdg_pie(coord, c, push.radius);

    let nrm = Vec2::new(fabs(res.0.y), fabs(res.0.z));

    let color = if res.0.x < 0.0 {
        Vec3::new(0.0, nrm.x, nrm.y)
    } else {
        Vec3::new(nrm.x, nrm.y, 0.0)
    };
    unsafe {
        target_image.write(id.xy(), color.extend(1.0));
    }
}
