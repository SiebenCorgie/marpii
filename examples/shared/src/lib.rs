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
#[derive(Clone, Copy)]
pub struct SimObj {
    pub location: [f32; 4],
    pub velocity: [f32; 4],
}


#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2]
}

#[repr(C)]
pub struct SimPush {
    pub sim_buffer: ResourceHandle,
    pub is_init: u32,
    pub buf_size: u32,
    pub pad: [u32; 1],
}

#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct ForwardPush {
    pub buf: ResourceHandle, //src we get our location data from
    pub buffer_size: u32,
    pub pad: [u32; 2],
}
