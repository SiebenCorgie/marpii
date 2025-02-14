#![no_std]
#![allow(unexpected_cfgs)]

#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};
pub use marpii_rmg_shared::ResourceHandle;
pub use spirv_std;
use spirv_std::glam::Mat4;

mod util;
pub use util::{saturate, smoothstep};

pub struct QuadCmdBuffer {
    pub cmds: [CmdQuad; 10_000],
}

///GPU/CPU shared quad command defintion. Some fields are only used in the gradient case.
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

///The push command currently just signals where to read our information from.
#[repr(C)]
pub struct QuadPush {
    ///The command buffer we found our data in
    pub cmd_buffer: ResourceHandle,
    ///The offset into the cmd_buffer where our command is written
    pub offset: u32,
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
            offset: 0,
            resolution: [1; 2],
            transform: Mat4::IDENTITY.to_cols_array(),
            scale: 1.0,
            layer_depth: 0.0,
            pad0: [0.0; 2],
        }
    }
}

//A mesh's vertex
#[repr(C)]
#[cfg_attr(
    not(target_arch = "spirv"),
    derive(Debug, Clone, Copy, Default, Pod, Zeroable)
)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
}

//A mesh draw call
#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug, Clone, Copy, Pod, Zeroable))]
pub struct MeshPush {
    pub index_buffer: ResourceHandle,
    pub vertex_buffer: ResourceHandle,
    //offset into the indexbuffer, where we find the index into the vertex-buffer
    pub index_offset: u32,
    pub layer_depth_float: u32,
    //resolution of the frame-buffer, used for translating
    //pixel-space to ndc
    pub resolution: [u32; 2],
    pad1: [u32; 2],
    //Mesh wide color
    pub color: [f32; 4],
    pub pos: [f32; 2],
    pub scale: [f32; 2],
}

impl Default for MeshPush {
    fn default() -> Self {
        MeshPush {
            index_buffer: ResourceHandle::INVALID,
            vertex_buffer: ResourceHandle::INVALID,
            index_offset: 0,
            layer_depth_float: 0,
            resolution: [0; 2],
            pad1: [0; 2],
            color: [0.0; 4],
            pos: [0.0; 2],
            scale: [0.0; 2],
        }
    }
}

///Represents the draw-call data for a single glyph
#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug, Clone, Copy, Pod, Zeroable))]
pub struct GlyphInstance {
    //position of the glyph in pixels
    pub pos: [f32; 2],
    //size of the glyph's rectangle in pixels
    pub size: [f32; 2],
    //the glyph's color
    pub color: [f32; 4],
    //offset into the atlas for sampling the glyph's pixel
    pub atlas_offset: [u32; 2],
    //atlas rectangle size (in pixels)
    pub atlas_size: [u32; 2],
    //the clip bound start in pixels
    pub clip_offset: [f32; 2],
    //the clip bound extend in pixels
    pub clip_size: [f32; 2],
    pub layer_depth: f32,
    pub pad0: [f32; 3],
    ///The glyph type:
    ///   0: 8bit alpha mask
    //    1: 32bit rgba subpixel mask
    //    2: 32bit rgba bitmap
    pub glyph_type: u32,
    pub pad1: [u32; 3],
}

impl Default for GlyphInstance {
    fn default() -> Self {
        Self {
            pos: [0.0; 2],
            size: [0.0; 2],
            color: [0.0; 4],
            atlas_offset: [0; 2],
            atlas_size: [0; 2],
            clip_offset: [0.0; 2],
            clip_size: [0.0; 2],
            layer_depth: 0.0,
            pad0: [0.0; 3],
            glyph_type: 0,
            pad1: [0; 3],
        }
    }
}

///Represents the push-command data for a text-render-batch
pub struct TextPush {
    //Where we find our instance data
    pub instance_data: ResourceHandle,
    pub glyph_atlas_alpha: ResourceHandle,
    pub glyph_atlas_color: ResourceHandle,
    pub glyph_sampler: ResourceHandle,
    ///Offset into _instance_data_ based on the layer that is currently drawn
    pub instance_data_offset: u32,
    pub resolution: [u32; 2],
    #[allow(unused)]
    pad0: u32,
    pub color_atlas_resolution: [u32; 2],
    pub mask_atlas_resolution: [u32; 2],
}

impl Default for TextPush {
    fn default() -> Self {
        TextPush {
            instance_data: ResourceHandle::INVALID,
            glyph_atlas_alpha: ResourceHandle::INVALID,
            glyph_atlas_color: ResourceHandle::INVALID,
            glyph_sampler: ResourceHandle::INVALID,
            resolution: [0; 2],
            instance_data_offset: 0,
            pad0: 0,
            color_atlas_resolution: [0; 2],
            mask_atlas_resolution: [0; 2],
        }
    }
}
