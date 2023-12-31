#![cfg_attr(target_arch = "spirv", no_std)]
//! Shared objects between the example's CPU side and GPU side code.

pub use marpii_rmg_shared;
pub use marpii_rmg_shared::ResourceHandle;

#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};

///EGui push constants for a draw command
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
#[derive(Clone, Copy)]
#[repr(C, align(16))]
pub struct EGuiPush {
    pub texture: ResourceHandle,
    pub sampler: ResourceHandle,
    pub pad0: [ResourceHandle; 2],
    pub screen_size: [f32; 2],
    pub gamma: f32,
    pub pad1: f32,
}

///Used for for alpha based blending effect
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
#[derive(Clone, Copy)]
#[repr(C, align(16))]
pub struct AlphaBlendPush {
    pub add: ResourceHandle,
    pub dst: ResourceHandle,
    pub pad0: [ResourceHandle; 2],
    pub add_offset: [i32; 2],
    pub dst_offset: [i32; 2],
    pub extent: [u32; 2],
    pub pad1: [u32; 2],
}

///Used for for alpha based blending effect
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
#[derive(Clone, Copy)]
#[repr(C, align(16))]
pub struct DownsamplePush {
    pub img: ResourceHandle,
    pub pad0: [ResourceHandle; 3],
    pub mip_level: u32,
    pub pad1: [u32; 3],
}
