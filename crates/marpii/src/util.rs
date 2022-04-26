///Converts a [Extent3D](ash::vk::Extent3D) to an offset. Needed for instance to convert
/// an image's extent to the offset parameter for image-blit or copy operations.
///
/// If `zero_to_one` is set, makes coordinates 1 that are 0 in the extent. This is for instance the requirement on the `dst_offset` parameter
/// of image_blit.
pub fn extent_to_offset(extent: ash::vk::Extent3D, zero_to_one: bool) -> ash::vk::Offset3D {
    if zero_to_one {
        ash::vk::Offset3D {
            //Note: max is correct since we are casting from a u32
            x: (extent.width as i32).max(1),
            y: (extent.height as i32).max(1),
            z: (extent.depth as i32).max(1),
        }
    } else {
        ash::vk::Offset3D {
            x: extent.width as i32,
            y: extent.height as i32,
            z: extent.depth as i32,
        }
    }
}
