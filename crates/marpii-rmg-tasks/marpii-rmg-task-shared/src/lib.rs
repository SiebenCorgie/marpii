#![cfg_attr(
    target_arch = "spirv",
    no_std,
    feature(register_attr),
    register_attr(spirv)
)]
//! Shared objects between the example's CPU side and GPU side code.

pub use marpii_rmg_shared;
pub use spirv_std;
pub use spirv_std::glam;
pub use marpii_rmg_shared::ResourceHandle;


///EGui push constants for a draw command
#[repr(C, align(16))]
pub struct EGuiPush{
    pub texture: ResourceHandle,
    pub pad0: [ResourceHandle; 3],
    pub screen_size: [f32; 2],
    pub pad1: [f32; 2]
}
