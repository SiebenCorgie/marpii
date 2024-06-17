#![no_std]
#![allow(unexpected_cfgs)]
//! Resources that are needed in multiple crates. Mostly RustGpu shader crates, and marpii-rmg itself.
#[cfg(feature = "marpii")]
use marpii::ash::vk;

#[cfg(not(target_arch = "spirv"))]
use bytemuck::{Pod, Zeroable};

///By definition when interpreted as big endian the highest byte is the handle type and the lower bytes are the actual index.
///
/// Note that the descriptor set index is the same as the type
//NOTE: Only derive Hash, Debug etc, on non-shader target. Otherwise panics the compiler atm.
#[cfg_attr(
    not(target_arch = "spirv"),
    derive(Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Debug, Pod, Zeroable)
)]
#[cfg_attr(target_auch = "spirv", derive(Clone, Copy))]
#[repr(C)]
pub struct ResourceHandle(u32);

impl ResourceHandle {
    pub const TYPE_STORAGE_BUFFER: u8 = 1 << 0;
    pub const TYPE_STORAGE_IMAGE: u8 = 1 << 1;
    pub const TYPE_SAMPLED_IMAGE: u8 = 1 << 2;
    pub const TYPE_SAMPLER: u8 = 1 << 3;
    pub const TYPE_ACCELERATION_STRUCTURE: u8 = 1 << 4;
    pub const TYPE_INVALID: u8 = 0xff;

    pub const INVALID: Self = Self::new_unchecked(Self::TYPE_INVALID, 0);

    ///Returns the handle type bits of this handle.
    pub const fn handle_type(&self) -> u8 {
        //self.0.to_be_bytes()[0]
        self.0 as u8
    }

    ///Returns the index of this handle into its own descriptor.
    pub const fn index(&self) -> u32 {
        //lowest byte is type, rest is index, therfore move 8bit, that should be it
        self.0 >> 8
        /*
        let mut bytes = self.0.to_be_bytes();
        bytes[0] = 0;
        u32::from_be_bytes(bytes)
         */
    }

    ///Returns true if the handle is invalid. Note that this contains **any** invalid
    /// `handle_type` bits, not just `TYPE_INVALID`
    pub const fn is_invalid(&self) -> bool {
        self.handle_type() > Self::TYPE_ACCELERATION_STRUCTURE
    }

    ///Returns true whenever this is a valid handle type. **Don't confuse with [is_invalid]**
    pub const fn is_valid(&self) -> bool {
        !self.is_invalid()
    }

    ///Creates a new handle, panics if the type is outside the defined types, or the index exceeds (2^56)-1.
    pub const fn new_unchecked(ty: u8, index: u32) -> Self {
        let bytes = (index << 8) | ty as u32;
        ResourceHandle(bytes)
    }

    ///Creates a new handle, panics if the type is outside the defined types, or the index exceeds (2^56)-1.
    pub const fn new(ty: u8, index: u32) -> Self {
        assert!(ty <= Self::TYPE_ACCELERATION_STRUCTURE);
        assert!(index < 2u32.pow(24));
        Self::new_unchecked(ty, index)
    }

    ///Returns true if the descriptor owns an index into the descriptor set for type `ty`.
    pub fn contains_type(&self, ty: u8) -> bool {
        //NOTE: using one bit per type, so this would be 0 if `ty`'s bit isn't set in the
        // handle_type as well
        (self.handle_type() & ty) > 0
    }

    #[cfg(feature = "marpii")]
    pub fn descriptor_ty(&self) -> vk::DescriptorType {
        match self.handle_type() {
            Self::TYPE_SAMPLED_IMAGE => vk::DescriptorType::SAMPLED_IMAGE,
            Self::TYPE_STORAGE_IMAGE => vk::DescriptorType::STORAGE_IMAGE,
            Self::TYPE_STORAGE_BUFFER => vk::DescriptorType::STORAGE_BUFFER,
            Self::TYPE_SAMPLER => vk::DescriptorType::SAMPLER,
            Self::TYPE_ACCELERATION_STRUCTURE => vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            _ => {
                //NOTE: This can't happen, but for compleatness we add it
                #[cfg(feature = "logging")]
                log::error!("Found unknown Resource handle, returning SampledImage");

                vk::DescriptorType::SAMPLED_IMAGE
            }
        }
    }

    #[cfg(feature = "marpii")]
    pub fn new_from_desc_ty(ty: vk::DescriptorType, index: u32) -> Self {
        let ty = Self::descriptor_type_to_u8(ty);

        Self::new(ty, index)
    }

    ///Builds the u8 for this descriptor type
    #[cfg(feature = "marpii")]
    pub fn descriptor_type_to_u8(ty: vk::DescriptorType) -> u8 {
        match ty {
            vk::DescriptorType::SAMPLED_IMAGE => Self::TYPE_SAMPLED_IMAGE,
            vk::DescriptorType::STORAGE_IMAGE => Self::TYPE_STORAGE_IMAGE,
            vk::DescriptorType::STORAGE_BUFFER => Self::TYPE_STORAGE_BUFFER,
            vk::DescriptorType::SAMPLER => Self::TYPE_SAMPLER,
            vk::DescriptorType::ACCELERATION_STRUCTURE_KHR => Self::TYPE_ACCELERATION_STRUCTURE,
            _ => {
                #[cfg(feature = "logging")]
                log::error!(
                    "Descriptor type {:?} unsupported, using STORAGE_BUFFER instead",
                    ty
                );

                Self::TYPE_STORAGE_BUFFER
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ResourceHandle;

    #[test]
    fn jee() {
        let res = ResourceHandle::new(ResourceHandle::TYPE_SAMPLER, 42);
        assert!(res.index() == 42);
        assert!(res.handle_type() == ResourceHandle::TYPE_SAMPLER);
    }

    #[cfg(feature = "marpii")]
    #[test]
    fn resource_handle_access() {
        use super::{vk, ResourceHandle};
        let sa_img = ResourceHandle::new_from_desc_ty(vk::DescriptorType::SAMPLED_IMAGE, 42);
        let st_img = ResourceHandle::new_from_desc_ty(vk::DescriptorType::STORAGE_IMAGE, 43);
        let st_buf = ResourceHandle::new_from_desc_ty(vk::DescriptorType::STORAGE_BUFFER, 44);
        let sa = ResourceHandle::new_from_desc_ty(vk::DescriptorType::SAMPLER, 10);
        let acc =
            ResourceHandle::new_from_desc_ty(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR, 45);
        let combined = ResourceHandle::new(
            ResourceHandle::TYPE_STORAGE_IMAGE | ResourceHandle::TYPE_SAMPLED_IMAGE,
            46,
        );

        assert!(sa_img.index() == 42, "42 != {}", sa_img.index());
        assert!(sa_img.descriptor_ty() == vk::DescriptorType::SAMPLED_IMAGE);
        assert!(st_img.index() == 43);
        assert!(st_img.descriptor_ty() == vk::DescriptorType::STORAGE_IMAGE);
        assert!(st_buf.index() == 44);
        assert!(st_buf.descriptor_ty() == vk::DescriptorType::STORAGE_BUFFER);
        assert!(sa.index() == 10);
        assert!(sa.descriptor_ty() == vk::DescriptorType::SAMPLER);
        assert!(acc.index() == 45);
        assert!(acc.descriptor_ty() == vk::DescriptorType::ACCELERATION_STRUCTURE_KHR);
        assert!(combined.index() == 46);
        assert!(combined.contains_type(ResourceHandle::TYPE_STORAGE_IMAGE));
        assert!(combined.contains_type(ResourceHandle::TYPE_SAMPLED_IMAGE));
    }
}
