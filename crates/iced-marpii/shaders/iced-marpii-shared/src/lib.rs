#![no_std]
#![allow(unexpected_cfgs)]

pub use bytemuck;
pub use marpii_rmg_shared::ResourceHandle;
pub use spirv_std;

mod util;
pub use util::{gradient, saturate, smoothstep};

mod quad;
pub use quad::{CmdQuad, CmdQuadGradient, QuadCmdBuffer, QuadGradientCmdBuffer, QuadPush};

mod glyph;
pub use glyph::{GlyphInstance, TextPush};

mod mesh;
pub use mesh::*;
