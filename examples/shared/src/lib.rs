#![cfg_attr(
    target_arch = "spirv",
    no_std,
    feature(register_attr),
    register_attr(spirv)
)]
//! Shared objects between the example's CPU side and GPU side code.


#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Zeroable, Pod};

pub use marpii_rmg_shared::ResourceHandle;

//rmg rendering object type
#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug, Pod, Zeroable))]
#[derive(Clone, Copy)]
pub struct SimObj {
    pub location: [f32; 4],
    pub velocity: [f32; 4],
}


#[cfg_attr(not(target_arch = "spirv"), derive(Debug, Clone, Copy, Pod, Zeroable))]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[repr(C)]
pub struct SimPush {
    pub sim_buffer: ResourceHandle,
    pub is_init: u32,
    pub buf_size: u32,
    pub img_handle: ResourceHandle,
    pub img_width: u32,
    pub img_height: u32,
    pub pad: [u32; 2],
}

#[cfg_attr(not(target_arch = "spirv"), derive(Clone, Copy, Debug, Pod, Zeroable))]
#[repr(C)]
pub struct ForwardPush {
    pub ubo: ResourceHandle,
    pub sim: ResourceHandle,
    pub pad: [u32; 2],
}


#[cfg_attr(not(target_arch = "spirv"), derive(Debug, Clone, Copy, Pod, Zeroable))]
#[repr(C)]
pub struct Ubo {
    pub model_view: [[f32; 4]; 4],
    pub perspective: [[f32; 4]; 4],
}
