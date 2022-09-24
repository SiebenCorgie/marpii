#![cfg_attr(
    target_arch = "spirv",
    no_std,
    feature(register_attr),
    register_attr(spirv)
)]
//! Shared objects between the exaples CPU side and GPU side code.

pub use marpii_rmg_shared::ResourceHandle;


//rmg rendering object type
#[repr(C, align(16))]
pub struct SimObj{
    pub location: [f32; 4],
    pub velocity: [f32; 4]
}

#[repr(C, align(16))]
pub struct SimPush{
    pub sim_src_buffer: ResourceHandle,
    pub sim_dst_buffer: ResourceHandle,
    pub is_init: u32,
    pub buf_size: u32,
    pub pad: [u32; 2]
}


#[repr(C, align(16))]
pub struct ForwardPush{
    pub rotation: [f32; 4],
    pub location_aspect: [f32; 4],
}
