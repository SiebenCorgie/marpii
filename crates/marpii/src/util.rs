use ash::vk;
mod timestamp;
pub use timestamp::{Timestamp, Timestamps};
mod query_pool;
pub use query_pool::QueryPool;

///Converts a [Extent3D](ash::vk::Extent3D) to an offset. Needed for instance to convert
/// an image's extent to the offset parameter for image-blit or copy operations.
///
/// If `zero_to_one` is set, makes coordinates 1 that are 0 in the extent. This is for instance the requirement on the `dst_offset` parameter
/// of image_blit.
pub fn extent_to_offset(extent: ash::vk::Extent3D, zero_to_one: bool) -> ash::vk::Offset3D {
    if zero_to_one {
        vk::Offset3D {
            //Note: max is correct since we are casting from a u32
            x: (extent.width as i32).max(1),
            y: (extent.height as i32).max(1),
            z: (extent.depth as i32).max(1),
        }
    } else {
        vk::Offset3D {
            x: extent.width as i32,
            y: extent.height as i32,
            z: extent.depth as i32,
        }
    }
}

///Defines a region of some image. Starting at `offset` ranging till `offset + extent`.
#[derive(Clone, Copy, Debug)]
pub struct ImageRegion {
    pub offset: vk::Offset3D,
    pub extent: vk::Extent3D,
}

impl ImageRegion {
    pub const ZERO: ImageRegion = ImageRegion {
        offset: vk::Offset3D { x: 0, y: 0, z: 0 },
        extent: vk::Extent3D {
            width: 0,
            height: 0,
            depth: 0,
        },
    };

    pub fn to_blit_offsets(&self) -> [vk::Offset3D; 2] {
        [
            self.offset,
            vk::Offset3D {
                x: self.offset.x + self.extent.width as i32,
                y: (self.offset.y + self.extent.height as i32).max(1),
                z: (self.offset.z + self.extent.depth as i32).max(1),
            },
        ]
    }

    ///Clamps `self` to be fully within `region`. This might move the offset into "within" `region`, and if the region
    /// exceeds `region` it might shrink `self.extent`.
    pub fn clamp_to(&mut self, region: &ImageRegion) {
        //TODO Currently moves `self.offset` in relation to `region`. In practise the relative
        // distance should be kept.

        //self.offset.x = self.offset.x.max(region.offset.x);
        //self.offset.y = self.offset.y.max(region.offset.y);
        //self.offset.z = self.offset.z.max(region.offset.z);

        self.extent.width = self.extent.width.clamp(0, region.extent.width);
        self.extent.height = self.extent.height.clamp(0, region.extent.height);
        self.extent.depth = self.extent.depth.clamp(0, region.extent.depth);
    }

    ///Converts this image region to a viewport. Note that offset and extent are set accordingly.
    ///
    /// The depth range is set to 0..1 by default.
    ///
    ///# Hint
    /// If you use this function in your shader the clip space will reach from (x,y) till (width,height).
    /// A more common convention is to use a range from 0..1 for x/y or -1..1 .
    pub fn as_viewport(&self) -> vk::Viewport {
        vk::Viewport {
            height: self.extent.height as f32,
            width: self.extent.width as f32,
            x: self.offset.x as f32,
            y: self.offset.y as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }
    }

    pub fn as_rect_2d(&self) -> vk::Rect2D {
        vk::Rect2D {
            extent: vk::Extent2D {
                width: self.extent.width,
                height: self.extent.height,
            },
            offset: vk::Offset2D {
                x: self.offset.x,
                y: self.offset.y,
            },
        }
    }

    ///Dissolves self into the pair (offset, extent).
    pub fn offset_extent_2d(&self) -> ([i32; 2], [u32; 2]) {
        (
            [self.offset.x, self.offset.y],
            [self.extent.width, self.extent.height],
        )
    }

    ///Dissolves self into the pair (offset, extent).
    pub fn offset_extent_3d(&self) -> ([i32; 3], [u32; 3]) {
        (
            [self.offset.x, self.offset.y, self.offset.z],
            [self.extent.width, self.extent.height, self.extent.depth],
        )
    }
}

///Converts ImageUsageFlags to FormatFeatureFlags needed to satisfy the usage flags. This does not contain all convertions. Only the basic ones.
pub fn image_usage_to_format_features(
    usage: ash::vk::ImageUsageFlags,
) -> ash::vk::FormatFeatureFlags {
    let mut properties = ash::vk::FormatFeatureFlags::empty();

    if usage.contains(ash::vk::ImageUsageFlags::COLOR_ATTACHMENT) {
        properties |= ash::vk::FormatFeatureFlags::COLOR_ATTACHMENT;
    }
    if usage.contains(ash::vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT) {
        properties |= ash::vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT;
    }
    if usage.contains(ash::vk::ImageUsageFlags::SAMPLED) {
        properties |= ash::vk::FormatFeatureFlags::SAMPLED_IMAGE;
    }
    if usage.contains(ash::vk::ImageUsageFlags::STORAGE) {
        properties |= ash::vk::FormatFeatureFlags::STORAGE_IMAGE;
    }
    if usage.contains(ash::vk::ImageUsageFlags::TRANSFER_SRC) {
        properties |= ash::vk::FormatFeatureFlags::TRANSFER_SRC;
    }
    if usage.contains(ash::vk::ImageUsageFlags::TRANSFER_DST) {
        properties |= ash::vk::FormatFeatureFlags::TRANSFER_DST;
    }

    properties
}

// Simple offset_of macro akin to C++ offsetof
//TODO retire in favour of std version if there is one at some point
#[macro_export]
macro_rules! offset_of {
    ($base:path, $field:ident) => {{
        #[allow(unused_unsafe)]
        unsafe {
            let b: $base = core::mem::zeroed();
            (&b.$field as *const _ as isize) - (&b as *const _ as isize)
        }
    }};
}

///All image format types. Note that this is different to the actual format. This directly translates to the format less types used in spirv to read/write formatless
/// storage images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatType {
    F32,
    F64,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    Other,
}

impl FormatType {
    pub fn parse(format: vk::Format) -> Self {
        match format {
            vk::Format::D32_SFLOAT
            | vk::Format::R32_SFLOAT
            | vk::Format::R32G32_SFLOAT
            | vk::Format::R32G32B32_SFLOAT
            | vk::Format::R32G32B32A32_SFLOAT => FormatType::F32,

            vk::Format::R64_SFLOAT
            | vk::Format::R64G64_SFLOAT
            | vk::Format::R64G64B64_SFLOAT
            | vk::Format::R64G64B64A64_SFLOAT => FormatType::F64,
            vk::Format::R8_UINT
            | vk::Format::R8G8_UINT
            | vk::Format::B8G8R8_UINT
            | vk::Format::R8G8B8_UINT
            | vk::Format::A8B8G8R8_UINT_PACK32
            | vk::Format::B8G8R8A8_UINT
            | vk::Format::R8G8B8A8_UINT => FormatType::U8,
            vk::Format::R16_UINT
            | vk::Format::R16G16_UINT
            | vk::Format::R16G16B16_UINT
            | vk::Format::R16G16B16A16_UINT => FormatType::U16,
            vk::Format::R32_UINT
            | vk::Format::R32G32_UINT
            | vk::Format::R32G32B32_UINT
            | vk::Format::R32G32B32A32_UINT => FormatType::U32,
            vk::Format::R64_UINT
            | vk::Format::R64G64_UINT
            | vk::Format::R64G64B64_UINT
            | vk::Format::R64G64B64A64_UINT => FormatType::U64,
            vk::Format::R8_SINT
            | vk::Format::R8G8_SINT
            | vk::Format::B8G8R8_SINT
            | vk::Format::R8G8B8_SINT
            | vk::Format::A8B8G8R8_SINT_PACK32
            | vk::Format::B8G8R8A8_SINT
            | vk::Format::R8G8B8A8_SINT => FormatType::I8,
            vk::Format::R16_SINT
            | vk::Format::R16G16_SINT
            | vk::Format::R16G16B16_SINT
            | vk::Format::R16G16B16A16_SINT => FormatType::I16,
            vk::Format::R32_SINT
            | vk::Format::R32G32_SINT
            | vk::Format::R32G32B32_SINT
            | vk::Format::R32G32B32A32_SINT => FormatType::I32,
            vk::Format::R64_SINT
            | vk::Format::R64G64_SINT
            | vk::Format::R64G64B64_SINT
            | vk::Format::R64G64B64A64_SINT => FormatType::I64,
            _ => FormatType::Other,
        }
    }
}

///Parsed extended set of format properties. Allows you to querry runtime information
pub struct FormatProperties {
    ///If known, contains the number of byte per pixel. Note that this is only parsed for *core*
    /// formats
    pub byte_per_pixel: Option<u8>,
    pub is_srgb: bool,
    ///FormatType defines the datatype per channel of that format.
    pub format_type: FormatType,
}

impl FormatProperties {
    pub fn parse(format: vk::Format) -> Self {
        let byte_per_pixel = byte_per_pixel(format);
        let is_srgb = is_srgb(format);
        let format_type = FormatType::parse(format);
        FormatProperties {
            byte_per_pixel,
            is_srgb,
            format_type,
        }
    }
}

///Returns the number of byte per pixel for the given format. This is usefull when trying to calculate how a given buffer translates to an image.
/// for instance, given a buffer and the target images width the height could be calculated.
///
/// # Note
/// The function is only implemented for core formats. Otherwise None is returned and an error is printed to the logs.
pub fn byte_per_pixel(format: vk::Format) -> Option<u8> {
    match format {
        vk::Format::R4G4_UNORM_PACK8 => Some(1),
        vk::Format::R4G4B4A4_UNORM_PACK16
        | vk::Format::R5G6B5_UNORM_PACK16
        | vk::Format::R5G5B5A1_UNORM_PACK16
        | vk::Format::A1R5G5B5_UNORM_PACK16 => Some(2),

        vk::Format::R8_UNORM
        | vk::Format::R8_SNORM
        | vk::Format::R8_USCALED
        | vk::Format::R8_SSCALED
        | vk::Format::R8_UINT
        | vk::Format::R8_SINT
        | vk::Format::R8_SRGB => Some(1),
        vk::Format::R8G8_UNORM
        | vk::Format::R8G8_SNORM
        | vk::Format::R8G8_USCALED
        | vk::Format::R8G8_SSCALED
        | vk::Format::R8G8_UINT
        | vk::Format::R8G8_SINT
        | vk::Format::R8G8_SRGB => Some(2),
        vk::Format::R8G8B8_UNORM
        | vk::Format::R8G8B8_SNORM
        | vk::Format::R8G8B8_USCALED
        | vk::Format::R8G8B8_SSCALED
        | vk::Format::R8G8B8_UINT
        | vk::Format::R8G8B8_SINT
        | vk::Format::R8G8B8_SRGB
        | vk::Format::B8G8R8_UNORM
        | vk::Format::B8G8R8_SNORM
        | vk::Format::B8G8R8_USCALED
        | vk::Format::B8G8R8_SSCALED
        | vk::Format::B8G8R8_UINT
        | vk::Format::B8G8R8_SINT
        | vk::Format::B8G8R8_SRGB => Some(3),

        vk::Format::R8G8B8A8_UNORM
        | vk::Format::R8G8B8A8_SNORM
        | vk::Format::R8G8B8A8_USCALED
        | vk::Format::R8G8B8A8_SSCALED
        | vk::Format::R8G8B8A8_UINT
        | vk::Format::R8G8B8A8_SINT
        | vk::Format::R8G8B8A8_SRGB
        | vk::Format::B8G8R8A8_UNORM
        | vk::Format::B8G8R8A8_SNORM
        | vk::Format::B8G8R8A8_USCALED
        | vk::Format::B8G8R8A8_SSCALED
        | vk::Format::B8G8R8A8_UINT
        | vk::Format::B8G8R8A8_SINT
        | vk::Format::B8G8R8A8_SRGB => Some(4),

        vk::Format::A2R10G10B10_UNORM_PACK32 => Some(4),
        vk::Format::A2R10G10B10_SNORM_PACK32 => Some(4),
        vk::Format::A2R10G10B10_USCALED_PACK32 => Some(4),
        vk::Format::A2R10G10B10_SSCALED_PACK32 => Some(4),
        vk::Format::A2R10G10B10_UINT_PACK32 => Some(4),
        vk::Format::A2R10G10B10_SINT_PACK32 => Some(4),
        vk::Format::A2B10G10R10_UNORM_PACK32 => Some(4),
        vk::Format::A2B10G10R10_SNORM_PACK32 => Some(4),
        vk::Format::A2B10G10R10_USCALED_PACK32 => Some(4),
        vk::Format::A2B10G10R10_SSCALED_PACK32 => Some(4),
        vk::Format::A2B10G10R10_UINT_PACK32 => Some(4),
        vk::Format::A2B10G10R10_SINT_PACK32 => Some(4),

        vk::Format::R16_UNORM
        | vk::Format::R16_SNORM
        | vk::Format::R16_USCALED
        | vk::Format::R16_SSCALED
        | vk::Format::R16_UINT
        | vk::Format::R16_SINT
        | vk::Format::R16_SFLOAT => Some(2),
        vk::Format::R16G16_UNORM
        | vk::Format::R16G16_SNORM
        | vk::Format::R16G16_USCALED
        | vk::Format::R16G16_SSCALED
        | vk::Format::R16G16_UINT
        | vk::Format::R16G16_SINT
        | vk::Format::R16G16_SFLOAT => Some(4),
        vk::Format::R16G16B16_UNORM
        | vk::Format::R16G16B16_SNORM
        | vk::Format::R16G16B16_USCALED
        | vk::Format::R16G16B16_SSCALED
        | vk::Format::R16G16B16_UINT
        | vk::Format::R16G16B16_SINT
        | vk::Format::R16G16B16_SFLOAT => Some(6),
        vk::Format::R16G16B16A16_UNORM
        | vk::Format::R16G16B16A16_SNORM
        | vk::Format::R16G16B16A16_USCALED
        | vk::Format::R16G16B16A16_SSCALED
        | vk::Format::R16G16B16A16_UINT
        | vk::Format::R16G16B16A16_SINT
        | vk::Format::R16G16B16A16_SFLOAT => Some(8),

        vk::Format::R32_UINT | vk::Format::R32_SINT | vk::Format::R32_SFLOAT => Some(4),
        vk::Format::R32G32_UINT | vk::Format::R32G32_SINT | vk::Format::R32G32_SFLOAT => Some(8),
        vk::Format::R32G32B32_UINT | vk::Format::R32G32B32_SINT | vk::Format::R32G32B32_SFLOAT => {
            Some(12)
        }
        vk::Format::R32G32B32A32_UINT
        | vk::Format::R32G32B32A32_SINT
        | vk::Format::R32G32B32A32_SFLOAT => Some(16),

        vk::Format::E5B9G9R9_UFLOAT_PACK32 => Some(4),
        _ => {
            #[cfg(feature = "logging")]
            log::error!("Format {:#?} is not supported by byte_per_pixel()", format);
            None
        }
    }
}

///Returns true if `format` is an `_SRGB` suffixed format. This is, simillar to [byte_per_pixel] only implemented for core formats. If unsure, have a look at the implementation.
pub fn is_srgb(format: vk::Format) -> bool {
    match format {
        vk::Format::R8_SRGB
        | vk::Format::BC2_SRGB_BLOCK
        | vk::Format::BC3_SRGB_BLOCK
        | vk::Format::BC7_SRGB_BLOCK
        | vk::Format::R8G8_SRGB
        | vk::Format::ASTC_4X4_SRGB_BLOCK
        | vk::Format::ASTC_5X5_SRGB_BLOCK
        | vk::Format::ASTC_6X6_SRGB_BLOCK
        | vk::Format::ASTC_8X5_SRGB_BLOCK
        | vk::Format::ASTC_8X6_SRGB_BLOCK
        | vk::Format::ASTC_8X8_SRGB_BLOCK
        | vk::Format::ASTC_10X5_SRGB_BLOCK
        | vk::Format::ASTC_10X6_SRGB_BLOCK
        | vk::Format::ASTC_10X8_SRGB_BLOCK
        | vk::Format::ASTC_10X10_SRGB_BLOCK
        | vk::Format::ASTC_12X10_SRGB_BLOCK
        | vk::Format::ASTC_12X12_SRGB_BLOCK
        | vk::Format::B8G8R8_SRGB
        | vk::Format::R8G8B8_SRGB
        | vk::Format::BC1_RGB_SRGB_BLOCK
        | vk::Format::A8B8G8R8_SRGB_PACK32
        | vk::Format::B8G8R8A8_SRGB
        | vk::Format::BC1_RGBA_SRGB_BLOCK
        | vk::Format::R8G8B8A8_SRGB
        | vk::Format::ETC2_R8G8B8_SRGB_BLOCK
        | vk::Format::PVRTC1_2BPP_SRGB_BLOCK_IMG
        | vk::Format::PVRTC1_4BPP_SRGB_BLOCK_IMG
        | vk::Format::PVRTC2_2BPP_SRGB_BLOCK_IMG
        | vk::Format::PVRTC2_4BPP_SRGB_BLOCK_IMG
        | vk::Format::ETC2_R8G8B8A1_SRGB_BLOCK
        | vk::Format::ETC2_R8G8B8A8_SRGB_BLOCK => true,
        _ => false,
    }
}
