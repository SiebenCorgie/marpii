#![cfg_attr(
    target_arch = "spirv",
    no_std,
)]
//! Shared objects between the example's CPU side and GPU side code.

pub use marpii_rmg_shared;
pub use spirv_std;
pub use spirv_std::glam;
pub use marpii_rmg_shared::ResourceHandle;

#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};

///EGui push constants for a draw command
#[cfg_attr(not(target_arch = "spirv"), derive(Pod, Zeroable))]
#[derive(Clone, Copy)]
#[repr(C, align(16))]
pub struct EGuiPush{
    pub texture: ResourceHandle,
    pub sampler: ResourceHandle,
    pub pad0: [ResourceHandle; 2],
    pub screen_size: [f32; 2],
    pub pad1: [f32; 2]
}
