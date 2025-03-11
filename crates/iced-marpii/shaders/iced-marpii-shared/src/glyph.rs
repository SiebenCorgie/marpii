#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};
pub use marpii_rmg_shared::ResourceHandle;

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
