#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};
use spirv_std::glam::{Vec2, Vec3};

#[cfg(target_arch = "spirv")]
use crate::spirv_std::num_traits::Float;

//DataLayout
//Bezier:
//   Start:     p0.xy
//   Ctrl:      p0.za
//   End:       p1.xy,
//   Thickness: p1.z
//Line:
//   Start:     p0.xy
//   End:       p0.za
//   Thickness: p1.x
#[repr(u32)]
pub enum ShapeType {
    Line = 0,
    Bezier = 1,
}

#[repr(C)]
#[cfg_attr(
    not(target_arch = "spirv"),
    derive(Debug, Clone, Copy, Default, Pod, Zeroable)
)]
pub struct CmdShape {
    pub ty: u32,
    pub pad0: [u32; 3],

    pub color: [f32; 4],

    pub border_color: [f32; 4],
    pub shadow_color: [f32; 4],

    pub border_width: f32,
    pub shadow_blur_radius: f32,
    pub shadow_offset: [f32; 2],

    pub bound_position: [f32; 2],
    pub bound_extent: [f32; 2],

    pub payload0: [f32; 4],
    pub payload1: [f32; 4],
}

///Somewhat flawed hashing implementation. We basically hash the content of [CmdShape], which might not be valid, debending on what you are doing.
#[cfg(not(target_arch = "spirv"))]
impl core::hash::Hash for CmdShape {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        for value in self
            .color
            .iter()
            .chain(self.border_color.iter())
            .chain(self.shadow_color.iter())
            .chain(self.shadow_offset.iter())
            .chain(self.bound_position.iter())
            .chain(self.bound_extent.iter())
            .chain(self.payload0.iter())
            .chain(self.payload1.iter())
            .chain([self.border_width, self.shadow_blur_radius].iter())
        {
            state.write_u32(value.to_bits());
        }
        //append ty
        state.write_u32(self.ty);
    }
}

impl CmdShape {
    ///Generates the bounds of this shape as (min, max), cliped by the frame bounds.
    pub fn bound(&self) -> (Vec2, Vec2) {
        let (min, max) = match self.ty {
            //Line
            0 => {
                let start = Vec2::new(self.payload0[0], self.payload0[1]);
                let end = Vec2::new(self.payload0[2], self.payload0[3]);
                let thickness = self.payload1[0];
                let additional_border = thickness + self.shadow_blur_radius + self.border_width;

                (
                    start.min(end) - additional_border,
                    start.max(end) + additional_border,
                )
            }
            //Bezier
            1 => {
                let start = Vec2::new(self.payload0[0], self.payload0[1]);
                let ctrl = Vec2::new(self.payload0[2], self.payload0[3]);
                let end = Vec2::new(self.payload1[0], self.payload1[1]);
                let thickness = self.payload1[2];
                let additional_border = thickness
                    + self.shadow_blur_radius
                    + self.border_width
                    + Vec2::from(self.shadow_offset).abs().max_element();
                (
                    start.min(end).min(ctrl) - additional_border,
                    start.max(end).max(ctrl) + additional_border,
                )
            }
            _ => (Vec2::ZERO, Vec2::ZERO),
        };

        //clip by frame bounds
        (
            min.max(Vec2::from(self.bound_position)),
            max.min(Vec2::from(self.bound_position) + Vec2::from(self.bound_extent)),
        )
    }

    pub fn bound_origin_extent(&self) -> (Vec2, Vec2) {
        let (min, max) = self.bound();
        let extent = (max - min).max(Vec2::ZERO);
        (min, extent)
    }

    //NOTE: some taken from here: https://iquilezles.org/articles/distfunctions2d/
    // MIT Inigo Quilez
    pub fn distance(&self, coord: Vec2) -> f32 {
        match self.ty {
            0 => self.distance_line(coord),
            1 => self.distance_bezier(coord),
            //Others always fail
            _ => f32::INFINITY,
        }
    }

    fn distance_line(&self, coord: Vec2) -> f32 {
        let start = Vec2::new(self.payload0[0], self.payload0[1]);
        let end = Vec2::new(self.payload0[2], self.payload0[3]);
        let thickness = self.payload1[0];

        let es = end - start;
        let cs = coord - start;

        let h = (cs.dot(es) / es.dot(es)).clamp(0.0, 1.0);
        (cs - h * es).length() - thickness
    }

    //Using this: https://www.shadertoy.com/view/XtdyDn
    // by FabriceNeyret2
    fn distance_bezier(&self, coord: Vec2) -> f32 {
        let start = Vec2::new(self.payload0[0], self.payload0[1]);
        let ctrl = Vec2::new(self.payload0[2], self.payload0[3]);
        let end = Vec2::new(self.payload1[0], self.payload1[1]);
        let thickness = self.payload1[2];

        //Overwrite control point
        let ctrl = vlerp(
            ctrl + Vec2::splat(1e-4),
            ctrl,
            (ctrl * 2.0 - start - end).signum().abs(),
        );

        let pa = ctrl - start;
        let pb = end - ctrl - pa;
        let pc = coord - start;
        let pd = pa * 2.0;

        let mut pp = solev_cubic(
            Vec3::new(-3.0 * pa.dot(pb), pc.dot(pb) - 2.0 * dd(pa), pc.dot(pa)) / (-(dd(pb))),
        );
        pp = pp.clamp(Vec2::ZERO, Vec2::ONE);
        (dd((pd + pb * pp.x) * pp.x - pc))
            .min(dd((pd + pb * pp.y) * pp.y - pc))
            .sqrt()
            - thickness
    }
}
#[inline]
fn dd(a: Vec2) -> f32 {
    a.dot(a)
}
//NOTE: something is buggy when importing glam's lerp on floats on GPU :/
fn slerp(lhs: f32, rhs: f32, t: f32) -> f32 {
    lhs + (rhs - lhs) * t
}
fn vlerp(a: Vec2, b: Vec2, alpha: Vec2) -> Vec2 {
    Vec2::new(slerp(a.x, b.x, alpha.x), slerp(a.y, b.y, alpha.x))
}

//See https://www.shadertoy.com/view/XtdyDn
fn solev_cubic(a: Vec3) -> Vec2 {
    let p = a.y - a.x * a.x / 3.0;
    let p3 = p * p * p;
    let q = a.x * (2. * a.x * a.x - 9. * a.y) / 27. + a.z;
    let d = q * q + 4. * p3 / 27.;

    //Outside
    if d > 0.0 {
        let mut x = (Vec2::new(1.0, -1.0) * d.sqrt() - q) * 0.5;
        x = x.signum() * x.abs().powf(1.0 / 3.0);
        return Vec2::splat(x.x + x.y - a.x / 3.0);
    }
    //inside
    let v = ((-(-27. / p3).sqrt()) * q * 0.5).acos() / 3.0;
    let m = v.cos();
    let n = v.sin() * (3.0f32).sqrt();

    Vec2::new(m + m, -n - m) * (-p / 3.0).sqrt() - a.x / 3.0
}
