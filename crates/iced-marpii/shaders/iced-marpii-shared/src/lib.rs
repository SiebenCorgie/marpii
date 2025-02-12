#![no_std]

pub use marpii_rmg_shared::ResourceHandle;
pub use spirv_std;

///GPU/CPU shared quad definition
#[repr(C)]
pub struct Quad {
    position: spirv_std::glam::Vec2,
}
