use marpii::ash::vk::{Format as VKF, ImageUsageFlags};
use wgpu::TextureFormat as WF;
use wgpu::{AstcBlock, AstcChannel};

///Maps a [wgpu format](WF) to a [Ash format](VKF). Note that it does not respect capablities of a device. So it might map, for instance, to [vk::Format::X8_D24_UNORM_PACK32], but that format might not be supported by a device.
pub fn map_wgpu_to_vk_texture_format(format: WF) -> VKF {
    match format {
        WF::R8Unorm => VKF::R8_UNORM,
        WF::R8Snorm => VKF::R8_SNORM,
        WF::R8Uint => VKF::R8_UINT,
        WF::R8Sint => VKF::R8_SINT,
        WF::R16Uint => VKF::R16_UINT,
        WF::R16Sint => VKF::R16_SINT,
        WF::R16Unorm => VKF::R16_UNORM,
        WF::R16Snorm => VKF::R16_SNORM,
        WF::R16Float => VKF::R16_SFLOAT,
        WF::Rg8Unorm => VKF::R8G8_UNORM,
        WF::Rg8Snorm => VKF::R8G8_SNORM,
        WF::Rg8Uint => VKF::R8G8_UINT,
        WF::Rg8Sint => VKF::R8G8_SINT,
        WF::Rg16Unorm => VKF::R16G16_UNORM,
        WF::Rg16Snorm => VKF::R16G16_SNORM,
        WF::R32Uint => VKF::R32_UINT,
        WF::R32Sint => VKF::R32_SINT,
        WF::R32Float => VKF::R32_SFLOAT,
        WF::Rg16Uint => VKF::R16G16_UINT,
        WF::Rg16Sint => VKF::R16G16_SINT,
        WF::Rg16Float => VKF::R16G16_SFLOAT,
        WF::Rgba8Unorm => VKF::R8G8B8A8_UNORM,
        WF::Rgba8UnormSrgb => VKF::R8G8B8A8_SRGB,
        WF::Bgra8UnormSrgb => VKF::B8G8R8A8_SRGB,
        WF::Rgba8Snorm => VKF::R8G8B8A8_SNORM,
        WF::Bgra8Unorm => VKF::B8G8R8A8_UNORM,
        WF::Rgba8Uint => VKF::R8G8B8A8_UINT,
        WF::Rgba8Sint => VKF::R8G8B8A8_SINT,
        WF::Rgb10a2Uint => VKF::A2B10G10R10_UINT_PACK32,
        WF::Rgb10a2Unorm => VKF::A2B10G10R10_UNORM_PACK32,
        WF::Rg11b10Ufloat => VKF::B10G11R11_UFLOAT_PACK32,
        WF::Rg32Uint => VKF::R32G32_UINT,
        WF::Rg32Sint => VKF::R32G32_SINT,
        WF::Rg32Float => VKF::R32G32_SFLOAT,
        WF::Rgba16Uint => VKF::R16G16B16A16_UINT,
        WF::Rgba16Sint => VKF::R16G16B16A16_SINT,
        WF::Rgba16Unorm => VKF::R16G16B16A16_UNORM,
        WF::Rgba16Snorm => VKF::R16G16B16A16_SNORM,
        WF::Rgba16Float => VKF::R16G16B16A16_SFLOAT,
        WF::Rgba32Uint => VKF::R32G32B32A32_UINT,
        WF::Rgba32Sint => VKF::R32G32B32A32_SINT,
        WF::Rgba32Float => VKF::R32G32B32A32_SFLOAT,
        WF::Depth32Float => VKF::D32_SFLOAT,
        WF::Depth32FloatStencil8 => VKF::D32_SFLOAT_S8_UINT,
        WF::Depth24Plus => VKF::X8_D24_UNORM_PACK32,
        WF::Depth24PlusStencil8 => VKF::D24_UNORM_S8_UINT,
        WF::Stencil8 => VKF::S8_UINT,
        WF::Depth16Unorm => VKF::D16_UNORM,
        WF::NV12 => VKF::G8_B8R8_2PLANE_420_UNORM,
        WF::Rgb9e5Ufloat => VKF::E5B9G9R9_UFLOAT_PACK32,
        WF::Bc1RgbaUnorm => VKF::BC1_RGBA_UNORM_BLOCK,
        WF::Bc1RgbaUnormSrgb => VKF::BC1_RGBA_SRGB_BLOCK,
        WF::Bc2RgbaUnorm => VKF::BC2_UNORM_BLOCK,
        WF::Bc2RgbaUnormSrgb => VKF::BC2_SRGB_BLOCK,
        WF::Bc3RgbaUnorm => VKF::BC3_UNORM_BLOCK,
        WF::Bc3RgbaUnormSrgb => VKF::BC3_SRGB_BLOCK,
        WF::Bc4RUnorm => VKF::BC4_UNORM_BLOCK,
        WF::Bc4RSnorm => VKF::BC4_SNORM_BLOCK,
        WF::Bc5RgUnorm => VKF::BC5_UNORM_BLOCK,
        WF::Bc5RgSnorm => VKF::BC5_SNORM_BLOCK,
        WF::Bc6hRgbUfloat => VKF::BC6H_UFLOAT_BLOCK,
        WF::Bc6hRgbFloat => VKF::BC6H_SFLOAT_BLOCK,
        WF::Bc7RgbaUnorm => VKF::BC7_UNORM_BLOCK,
        WF::Bc7RgbaUnormSrgb => VKF::BC7_SRGB_BLOCK,
        WF::Etc2Rgb8Unorm => VKF::ETC2_R8G8B8_UNORM_BLOCK,
        WF::Etc2Rgb8UnormSrgb => VKF::ETC2_R8G8B8_SRGB_BLOCK,
        WF::Etc2Rgb8A1Unorm => VKF::ETC2_R8G8B8A1_UNORM_BLOCK,
        WF::Etc2Rgb8A1UnormSrgb => VKF::ETC2_R8G8B8A1_SRGB_BLOCK,
        WF::Etc2Rgba8Unorm => VKF::ETC2_R8G8B8A8_UNORM_BLOCK,
        WF::Etc2Rgba8UnormSrgb => VKF::ETC2_R8G8B8A8_SRGB_BLOCK,
        WF::EacR11Unorm => VKF::EAC_R11_UNORM_BLOCK,
        WF::EacR11Snorm => VKF::EAC_R11_SNORM_BLOCK,
        WF::EacRg11Unorm => VKF::EAC_R11G11_UNORM_BLOCK,
        WF::EacRg11Snorm => VKF::EAC_R11G11_SNORM_BLOCK,
        WF::Astc { block, channel } => match channel {
            AstcChannel::Unorm => match block {
                AstcBlock::B4x4 => VKF::ASTC_4X4_UNORM_BLOCK,
                AstcBlock::B5x4 => VKF::ASTC_5X4_UNORM_BLOCK,
                AstcBlock::B5x5 => VKF::ASTC_5X5_UNORM_BLOCK,
                AstcBlock::B6x5 => VKF::ASTC_6X5_UNORM_BLOCK,
                AstcBlock::B6x6 => VKF::ASTC_6X6_UNORM_BLOCK,
                AstcBlock::B8x5 => VKF::ASTC_8X5_UNORM_BLOCK,
                AstcBlock::B8x6 => VKF::ASTC_8X6_UNORM_BLOCK,
                AstcBlock::B8x8 => VKF::ASTC_8X8_UNORM_BLOCK,
                AstcBlock::B10x5 => VKF::ASTC_10X5_UNORM_BLOCK,
                AstcBlock::B10x6 => VKF::ASTC_10X6_UNORM_BLOCK,
                AstcBlock::B10x8 => VKF::ASTC_10X8_UNORM_BLOCK,
                AstcBlock::B10x10 => VKF::ASTC_10X10_UNORM_BLOCK,
                AstcBlock::B12x10 => VKF::ASTC_12X10_UNORM_BLOCK,
                AstcBlock::B12x12 => VKF::ASTC_12X12_UNORM_BLOCK,
            },
            AstcChannel::UnormSrgb => match block {
                AstcBlock::B4x4 => VKF::ASTC_4X4_SRGB_BLOCK,
                AstcBlock::B5x4 => VKF::ASTC_5X4_SRGB_BLOCK,
                AstcBlock::B5x5 => VKF::ASTC_5X5_SRGB_BLOCK,
                AstcBlock::B6x5 => VKF::ASTC_6X5_SRGB_BLOCK,
                AstcBlock::B6x6 => VKF::ASTC_6X6_SRGB_BLOCK,
                AstcBlock::B8x5 => VKF::ASTC_8X5_SRGB_BLOCK,
                AstcBlock::B8x6 => VKF::ASTC_8X6_SRGB_BLOCK,
                AstcBlock::B8x8 => VKF::ASTC_8X8_SRGB_BLOCK,
                AstcBlock::B10x5 => VKF::ASTC_10X5_SRGB_BLOCK,
                AstcBlock::B10x6 => VKF::ASTC_10X6_SRGB_BLOCK,
                AstcBlock::B10x8 => VKF::ASTC_10X8_SRGB_BLOCK,
                AstcBlock::B10x10 => VKF::ASTC_10X10_SRGB_BLOCK,
                AstcBlock::B12x10 => VKF::ASTC_12X10_SRGB_BLOCK,
                AstcBlock::B12x12 => VKF::ASTC_12X12_SRGB_BLOCK,
            },
            AstcChannel::Hdr => match block {
                AstcBlock::B4x4 => VKF::ASTC_4X4_SFLOAT_BLOCK_EXT,
                AstcBlock::B5x4 => VKF::ASTC_5X4_SFLOAT_BLOCK_EXT,
                AstcBlock::B5x5 => VKF::ASTC_5X5_SFLOAT_BLOCK_EXT,
                AstcBlock::B6x5 => VKF::ASTC_6X5_SFLOAT_BLOCK_EXT,
                AstcBlock::B6x6 => VKF::ASTC_6X6_SFLOAT_BLOCK_EXT,
                AstcBlock::B8x5 => VKF::ASTC_8X5_SFLOAT_BLOCK_EXT,
                AstcBlock::B8x6 => VKF::ASTC_8X6_SFLOAT_BLOCK_EXT,
                AstcBlock::B8x8 => VKF::ASTC_8X8_SFLOAT_BLOCK_EXT,
                AstcBlock::B10x5 => VKF::ASTC_10X5_SFLOAT_BLOCK_EXT,
                AstcBlock::B10x6 => VKF::ASTC_10X6_SFLOAT_BLOCK_EXT,
                AstcBlock::B10x8 => VKF::ASTC_10X8_SFLOAT_BLOCK_EXT,
                AstcBlock::B10x10 => VKF::ASTC_10X10_SFLOAT_BLOCK_EXT,
                AstcBlock::B12x10 => VKF::ASTC_12X10_SFLOAT_BLOCK_EXT,
                AstcBlock::B12x12 => VKF::ASTC_12X12_SFLOAT_BLOCK_EXT,
            },
        },
    }
}

///Maps a vulkan format to a wgpu [TextureFormat]. Note that not all formats are supported.
pub fn map_vk_to_wgpu_texture_format(format: VKF) -> Option<WF> {
    let f = match format {
        VKF::R8_UNORM => WF::R8Unorm,
        VKF::R8_SNORM => WF::R8Snorm,
        VKF::R8_UINT => WF::R8Uint,
        VKF::R8_SINT => WF::R8Sint,
        VKF::R16_UINT => WF::R16Uint,
        VKF::R16_SINT => WF::R16Sint,
        VKF::R16_UNORM => WF::R16Unorm,
        VKF::R16_SNORM => WF::R16Snorm,
        VKF::R16_SFLOAT => WF::R16Float,
        VKF::R8G8_UNORM => WF::Rg8Unorm,
        VKF::R8G8_SNORM => WF::Rg8Snorm,
        VKF::R8G8_UINT => WF::Rg8Uint,
        VKF::R8G8_SINT => WF::Rg8Sint,
        VKF::R16G16_UNORM => WF::Rg16Unorm,
        VKF::R16G16_SNORM => WF::Rg16Snorm,
        VKF::R32_UINT => WF::R32Uint,
        VKF::R32_SINT => WF::R32Sint,
        VKF::R32_SFLOAT => WF::R32Float,
        VKF::R16G16_UINT => WF::Rg16Uint,
        VKF::R16G16_SINT => WF::Rg16Sint,
        VKF::R16G16_SFLOAT => WF::Rg16Float,
        VKF::R8G8B8A8_UNORM => WF::Rgba8Unorm,
        VKF::R8G8B8A8_SRGB => WF::Rgba8UnormSrgb,
        VKF::B8G8R8A8_SRGB => WF::Bgra8UnormSrgb,
        VKF::R8G8B8A8_SNORM => WF::Rgba8Snorm,
        VKF::B8G8R8A8_UNORM => WF::Bgra8Unorm,
        VKF::R8G8B8A8_UINT => WF::Rgba8Uint,
        VKF::R8G8B8A8_SINT => WF::Rgba8Sint,
        VKF::A2B10G10R10_UINT_PACK32 => WF::Rgb10a2Uint,
        VKF::A2B10G10R10_UNORM_PACK32 => WF::Rgb10a2Unorm,
        VKF::B10G11R11_UFLOAT_PACK32 => WF::Rg11b10Ufloat,
        VKF::R32G32_UINT => WF::Rg32Uint,
        VKF::R32G32_SINT => WF::Rg32Sint,
        VKF::R32G32_SFLOAT => WF::Rg32Float,
        VKF::R16G16B16A16_UINT => WF::Rgba16Uint,
        VKF::R16G16B16A16_SINT => WF::Rgba16Sint,
        VKF::R16G16B16A16_UNORM => WF::Rgba16Unorm,
        VKF::R16G16B16A16_SNORM => WF::Rgba16Snorm,
        VKF::R16G16B16A16_SFLOAT => WF::Rgba16Float,
        VKF::R32G32B32A32_UINT => WF::Rgba32Uint,
        VKF::R32G32B32A32_SINT => WF::Rgba32Sint,
        VKF::R32G32B32A32_SFLOAT => WF::Rgba32Float,
        VKF::D32_SFLOAT => WF::Depth32Float,
        VKF::D32_SFLOAT_S8_UINT => WF::Depth32FloatStencil8,
        VKF::X8_D24_UNORM_PACK32 => WF::Depth24Plus,
        VKF::D24_UNORM_S8_UINT => WF::Depth24PlusStencil8,
        VKF::S8_UINT => WF::Stencil8,
        VKF::D16_UNORM => WF::Depth16Unorm,
        VKF::G8_B8R8_2PLANE_420_UNORM => WF::NV12,
        _ => return None,
        /*TODO: do the rest?
        WF::Rgb9e5Ufloat => VKF::E5B9G9R9_UFLOAT_PACK32,
        WF::Bc1RgbaUnorm => VKF::BC1_RGBA_UNORM_BLOCK,
        WF::Bc1RgbaUnormSrgb => VKF::BC1_RGBA_SRGB_BLOCK,
        WF::Bc2RgbaUnorm => VKF::BC2_UNORM_BLOCK,
        WF::Bc2RgbaUnormSrgb => VKF::BC2_SRGB_BLOCK,
        WF::Bc3RgbaUnorm => VKF::BC3_UNORM_BLOCK,
        WF::Bc3RgbaUnormSrgb => VKF::BC3_SRGB_BLOCK,
        WF::Bc4RUnorm => VKF::BC4_UNORM_BLOCK,
        WF::Bc4RSnorm => VKF::BC4_SNORM_BLOCK,
        WF::Bc5RgUnorm => VKF::BC5_UNORM_BLOCK,
        WF::Bc5RgSnorm => VKF::BC5_SNORM_BLOCK,
        WF::Bc6hRgbUfloat => VKF::BC6H_UFLOAT_BLOCK,
        WF::Bc6hRgbFloat => VKF::BC6H_SFLOAT_BLOCK,
        WF::Bc7RgbaUnorm => VKF::BC7_UNORM_BLOCK,
        WF::Bc7RgbaUnormSrgb => VKF::BC7_SRGB_BLOCK,
        WF::Etc2Rgb8Unorm => VKF::ETC2_R8G8B8_UNORM_BLOCK,
        WF::Etc2Rgb8UnormSrgb => VKF::ETC2_R8G8B8_SRGB_BLOCK,
        WF::Etc2Rgb8A1Unorm => VKF::ETC2_R8G8B8A1_UNORM_BLOCK,
        WF::Etc2Rgb8A1UnormSrgb => VKF::ETC2_R8G8B8A1_SRGB_BLOCK,
        WF::Etc2Rgba8Unorm => VKF::ETC2_R8G8B8A8_UNORM_BLOCK,
        WF::Etc2Rgba8UnormSrgb => VKF::ETC2_R8G8B8A8_SRGB_BLOCK,
        WF::EacR11Unorm => VKF::EAC_R11_UNORM_BLOCK,
        WF::EacR11Snorm => VKF::EAC_R11_SNORM_BLOCK,
        WF::EacRg11Unorm => VKF::EAC_R11G11_UNORM_BLOCK,
        WF::EacRg11Snorm => VKF::EAC_R11G11_SNORM_BLOCK,
        WF::Astc { block, channel } => match channel {
            AstcChannel::Unorm => match block {
                AstcBlock::B4x4 => VKF::ASTC_4X4_UNORM_BLOCK,
                AstcBlock::B5x4 => VKF::ASTC_5X4_UNORM_BLOCK,
                AstcBlock::B5x5 => VKF::ASTC_5X5_UNORM_BLOCK,
                AstcBlock::B6x5 => VKF::ASTC_6X5_UNORM_BLOCK,
                AstcBlock::B6x6 => VKF::ASTC_6X6_UNORM_BLOCK,
                AstcBlock::B8x5 => VKF::ASTC_8X5_UNORM_BLOCK,
                AstcBlock::B8x6 => VKF::ASTC_8X6_UNORM_BLOCK,
                AstcBlock::B8x8 => VKF::ASTC_8X8_UNORM_BLOCK,
                AstcBlock::B10x5 => VKF::ASTC_10X5_UNORM_BLOCK,
                AstcBlock::B10x6 => VKF::ASTC_10X6_UNORM_BLOCK,
                AstcBlock::B10x8 => VKF::ASTC_10X8_UNORM_BLOCK,
                AstcBlock::B10x10 => VKF::ASTC_10X10_UNORM_BLOCK,
                AstcBlock::B12x10 => VKF::ASTC_12X10_UNORM_BLOCK,
                AstcBlock::B12x12 => VKF::ASTC_12X12_UNORM_BLOCK,
            },
            AstcChannel::UnormSrgb => match block {
                AstcBlock::B4x4 => VKF::ASTC_4X4_SRGB_BLOCK,
                AstcBlock::B5x4 => VKF::ASTC_5X4_SRGB_BLOCK,
                AstcBlock::B5x5 => VKF::ASTC_5X5_SRGB_BLOCK,
                AstcBlock::B6x5 => VKF::ASTC_6X5_SRGB_BLOCK,
                AstcBlock::B6x6 => VKF::ASTC_6X6_SRGB_BLOCK,
                AstcBlock::B8x5 => VKF::ASTC_8X5_SRGB_BLOCK,
                AstcBlock::B8x6 => VKF::ASTC_8X6_SRGB_BLOCK,
                AstcBlock::B8x8 => VKF::ASTC_8X8_SRGB_BLOCK,
                AstcBlock::B10x5 => VKF::ASTC_10X5_SRGB_BLOCK,
                AstcBlock::B10x6 => VKF::ASTC_10X6_SRGB_BLOCK,
                AstcBlock::B10x8 => VKF::ASTC_10X8_SRGB_BLOCK,
                AstcBlock::B10x10 => VKF::ASTC_10X10_SRGB_BLOCK,
                AstcBlock::B12x10 => VKF::ASTC_12X10_SRGB_BLOCK,
                AstcBlock::B12x12 => VKF::ASTC_12X12_SRGB_BLOCK,
            },
            AstcChannel::Hdr => match block {
                AstcBlock::B4x4 => VKF::ASTC_4X4_SFLOAT_BLOCK_EXT,
                AstcBlock::B5x4 => VKF::ASTC_5X4_SFLOAT_BLOCK_EXT,
                AstcBlock::B5x5 => VKF::ASTC_5X5_SFLOAT_BLOCK_EXT,
                AstcBlock::B6x5 => VKF::ASTC_6X5_SFLOAT_BLOCK_EXT,
                AstcBlock::B6x6 => VKF::ASTC_6X6_SFLOAT_BLOCK_EXT,
                AstcBlock::B8x5 => VKF::ASTC_8X5_SFLOAT_BLOCK_EXT,
                AstcBlock::B8x6 => VKF::ASTC_8X6_SFLOAT_BLOCK_EXT,
                AstcBlock::B8x8 => VKF::ASTC_8X8_SFLOAT_BLOCK_EXT,
                AstcBlock::B10x5 => VKF::ASTC_10X5_SFLOAT_BLOCK_EXT,
                AstcBlock::B10x6 => VKF::ASTC_10X6_SFLOAT_BLOCK_EXT,
                AstcBlock::B10x8 => VKF::ASTC_10X8_SFLOAT_BLOCK_EXT,
                AstcBlock::B10x10 => VKF::ASTC_10X10_SFLOAT_BLOCK_EXT,
                AstcBlock::B12x10 => VKF::ASTC_12X10_SFLOAT_BLOCK_EXT,
                AstcBlock::B12x12 => VKF::ASTC_12X12_SFLOAT_BLOCK_EXT,
            },
        },
        */
    };

    Some(f)
}

pub fn map_vk_to_wgpu_texture_usage(
    usage: marpii::ash::vk::ImageUsageFlags,
) -> Option<wgpu::TextureUsages> {
    let f = match usage {
        ImageUsageFlags::COLOR_ATTACHMENT
        | ImageUsageFlags::INPUT_ATTACHMENT
        | ImageUsageFlags::TRANSIENT_ATTACHMENT
        | ImageUsageFlags::ATTACHMENT_FEEDBACK_LOOP_EXT
        | ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
        | ImageUsageFlags::FRAGMENT_SHADING_RATE_ATTACHMENT_KHR => {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        }
        ImageUsageFlags::STORAGE => wgpu::TextureUsages::STORAGE_BINDING,
        ImageUsageFlags::SAMPLED => wgpu::TextureUsages::TEXTURE_BINDING,
        ImageUsageFlags::TRANSFER_SRC => wgpu::TextureUsages::COPY_SRC,
        ImageUsageFlags::TRANSFER_DST => wgpu::TextureUsages::COPY_DST,
        _ => return None,
    };

    Some(f)
}

///Maps the `usage` to an equivalent vulkan ImageUsageFlag. Note, that this is not a 1:1 translation. For instance `RENDER_ATTACHMENT` is always translated to `COLOR_ATTCHENT`, while `INPUT_ATTACHMENT` might also be valid.
pub fn map_wgpu_to_vk_image_usage(usage: wgpu::TextureUsages) -> marpii::ash::vk::ImageUsageFlags {
    let mut vkusage = marpii::ash::vk::ImageUsageFlags::empty();

    if usage.contains(wgpu::TextureUsages::COPY_DST) {
        vkusage = vkusage | marpii::ash::vk::ImageUsageFlags::TRANSFER_DST
    }

    if usage.contains(wgpu::TextureUsages::COPY_SRC) {
        vkusage = vkusage | marpii::ash::vk::ImageUsageFlags::TRANSFER_SRC
    }
    if usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        vkusage = vkusage | marpii::ash::vk::ImageUsageFlags::COLOR_ATTACHMENT
    }
    if usage.contains(wgpu::TextureUsages::STORAGE_BINDING) {
        vkusage = vkusage | marpii::ash::vk::ImageUsageFlags::STORAGE
    }
    if usage.contains(wgpu::TextureUsages::TEXTURE_BINDING) {
        vkusage = vkusage | marpii::ash::vk::ImageUsageFlags::SAMPLED
    }
    vkusage
}
