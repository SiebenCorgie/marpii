use spirv_std::glam::{Vec2, Vec4, Vec4Swizzles};
#[cfg(target_arch = "spirv")]
use spirv_std::num_traits::Float;

pub fn lerp(x: f32, y: f32, a: f32) -> f32 {
    (x * (1.0 - a)) + (y * a)
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    // Scale, bias and saturate x to 0..1 range
    let x = saturate((x - edge0) / (edge1 - edge0));
    // Evaluate polynomial
    x * x * (3.0 - 2.0 * x)
}

pub fn saturate(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

///The _classic_ random source found all over shadertoy
pub fn random(coords: Vec2) -> f32 {
    const SEED: Vec2 = Vec2::new(12.9898, 78.233);
    (coords.dot(SEED).sin() * 43_758.547).fract()
}

//Calculates the max 8-stop gradient.
//This is pretty much a copy of Iced's WGSL implementation.
pub fn gradient(
    raw_pos: Vec2,
    direction: Vec4,
    colors: &[Vec4; 8],
    offsets: &[f32; 8],
    last_index: usize,
) -> Vec4 {
    //just to be sure, we chose 6, since
    //we have at most 8-fstops, and the loop indexes into i+1.
    //
    //pretty sure the reference is _somewhat_ wrong at that point.
    let last_index = last_index.min(6);

    let start = direction.xy();
    let end = direction.zw();

    let v1 = end - start;
    let v2 = raw_pos - start;
    let unit = v1.normalize();
    let coord_offset = unit.dot(v2) / v1.length();

    let mut color = Vec4::ZERO;
    //NOTE: Thats from the reference implementation, I guess found by
    //      fair dice roll :D
    let noise_granularity: f32 = 0.3 / 255.0;

    for i in 0..last_index {
        let curr_offset = offsets[i];
        let next_offset = offsets[i + 1];

        if coord_offset <= offsets[0] {
            color = colors[0];
        }

        if curr_offset <= coord_offset && coord_offset <= next_offset {
            let from = colors[i];
            let to = colors[i + 1];
            let factor = smoothstep(curr_offset, next_offset, coord_offset);
            color = interpolate_color(from, to, factor);
        }

        if coord_offset >= offsets[last_index] {
            color = colors[last_index];
        }
    }

    color + lerp(-noise_granularity, noise_granularity, random(raw_pos))
}

fn interpolate_color(from: Vec4, to: Vec4, factor: f32) -> Vec4 {
    //TODO: should probably use OkLab at some point
    from.lerp(to, factor)
}
