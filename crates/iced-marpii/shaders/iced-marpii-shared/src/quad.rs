#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};
pub use marpii_rmg_shared::ResourceHandle;
pub use spirv_std;
use spirv_std::glam::{Mat4, Vec4};

pub struct QuadCmdBuffer {
    pub cmds: [CmdQuad; 10_000],
}

///GPU/CPU shared quad command defintion.
#[repr(C)]
#[cfg_attr(
    not(target_arch = "spirv"),
    derive(Debug, Clone, Copy, Default, Pod, Zeroable)
)]
pub struct CmdQuad {
    pub color: [f32; 4],
    pub position: [f32; 2],
    pub size: [f32; 2],

    pub border_color: [f32; 4],
    pub border_radius: [f32; 4],
    pub shadow_color: [f32; 4],

    pub border_width: f32,
    pub shadow_blur_radius: f32,
    pub shadow_offset: [f32; 2],
}

///Somewhat flawed hashing implementation. We basically hash the content of [CmdQuad], which might not be valid, debending on what you are doing.
#[cfg(not(target_arch = "spirv"))]
impl core::hash::Hash for CmdQuad {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        for value in self
            .position
            .iter()
            .chain(self.color.iter())
            .chain(self.size.iter())
            .chain(self.border_color.iter())
            .chain(self.border_radius.iter())
            .chain(self.shadow_color.iter())
            .chain(self.shadow_offset.iter())
            .chain([self.border_width, self.shadow_blur_radius].iter())
        {
            state.write_u32(value.to_bits());
        }
    }
}

pub struct QuadGradientCmdBuffer {
    pub cmds: [CmdQuadGradient; 10_000],
}

///GPU/CPU shared quad-gradient command defintion.
#[repr(C)]
#[cfg_attr(
    not(target_arch = "spirv"),
    derive(Debug, Clone, Copy, Default, Pod, Zeroable)
)]
pub struct CmdQuadGradient {
    //16bit packed gradient colors
    pub colors_0: [f32; 4],
    pub colors_1: [f32; 4],
    pub colors_2: [f32; 4],
    pub colors_3: [f32; 4],
    pub colors_4: [f32; 4],
    pub colors_5: [f32; 4],
    pub colors_6: [f32; 4],
    pub colors_7: [f32; 4],
    //gradient stop offsets
    pub offsets_0: [f32; 4],
    pub offsets_1: [f32; 4],
    //direction of the gradient,
    //currently only linear is supported
    pub direction: [f32; 4],

    //Position of the gradients's rect (in px)
    pub position: [f32; 2],
    //scale of the gradients's rect (in px)
    pub size: [f32; 2],

    pub border_color: [f32; 4],
    pub border_radius: [f32; 4],
    pub shadow_color: [f32; 4],

    pub border_width: f32,
    pub shadow_blur_radius: f32,
    pub shadow_offset: [f32; 2],
}

impl CmdQuadGradient {
    pub fn unpack_offsets(&self) -> [f32; 8] {
        [
            self.offsets_0[0],
            self.offsets_0[1],
            self.offsets_0[2],
            self.offsets_0[3],
            self.offsets_1[0],
            self.offsets_1[1],
            self.offsets_1[2],
            self.offsets_1[3],
        ]
    }

    pub fn unpack_colors(&self) -> [Vec4; 8] {
        [
            Vec4::from(self.colors_0),
            Vec4::from(self.colors_1),
            Vec4::from(self.colors_2),
            Vec4::from(self.colors_3),
            Vec4::from(self.colors_4),
            Vec4::from(self.colors_5),
            Vec4::from(self.colors_6),
            Vec4::from(self.colors_7),
        ]
    }
}

///Somewhat flawed hashing implementation. We basically hash the content of [CmdQuadGradient], which might not be valid, debending on what you are doing.
#[cfg(not(target_arch = "spirv"))]
impl core::hash::Hash for CmdQuadGradient {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        for float_value in self
            .position
            .iter()
            .chain(self.direction.iter())
            .chain(self.size.iter())
            .chain(self.colors_0.iter())
            .chain(self.colors_1.iter())
            .chain(self.colors_2.iter())
            .chain(self.colors_3.iter())
            .chain(self.colors_4.iter())
            .chain(self.colors_5.iter())
            .chain(self.colors_6.iter())
            .chain(self.colors_7.iter())
            .chain(self.border_color.iter())
            .chain(self.border_radius.iter())
            .chain(self.shadow_color.iter())
            .chain(self.shadow_offset.iter())
            .chain(self.offsets_0.iter())
            .chain(self.offsets_1.iter())
            .chain([self.border_width, self.shadow_blur_radius].iter())
        {
            state.write_u32(float_value.to_bits());
        }
    }
}

///The push command currently just signals where to read our information from.
#[repr(C)]
pub struct QuadPush {
    ///The command buffer we found our data in
    pub cmd_buffer: ResourceHandle,
    ///The offset into the cmd_buffer where our command is written
    pub pad1: u32,
    pub resolution: [u32; 2],
    pub transform: [f32; 16],
    pub scale: f32,
    pub layer_depth: f32,
    pub pad0: [f32; 2],
}

impl Default for QuadPush {
    fn default() -> Self {
        Self {
            cmd_buffer: ResourceHandle::INVALID,
            pad1: 0,
            resolution: [1; 2],
            transform: Mat4::IDENTITY.to_cols_array(),
            scale: 1.0,
            layer_depth: 0.0,
            pad0: [0.0; 2],
        }
    }
}
