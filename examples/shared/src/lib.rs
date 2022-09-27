#![cfg_attr(
    target_arch = "spirv",
    no_std,
    feature(register_attr),
    register_attr(spirv)
)]
//! Shared objects between the example's CPU side and GPU side code.

pub use marpii_rmg_shared::ResourceHandle;


//rmg rendering object type
#[repr(C)]
pub struct SimObj{
    pub location: [f32; 4],
    pub velocity: [f32; 4]
}

#[repr(C)]
pub struct SimPush{
    pub sim_src_buffer: ResourceHandle,
    pub sim_dst_buffer: ResourceHandle,
    pub is_init: u32,
    pub buf_size: u32,
    pub pad: [u32; 2]
}


#[cfg_attr(not(target_arch="spirv"), derive(Debug))]
#[repr(C)]
pub struct ForwardPush{
    pub buf: ResourceHandle,
    pub target_img: ResourceHandle,
    pub width: u32,
    pub height: u32,
    pub buffer_size: u32,
    pub pad: [u32; 3]
}
