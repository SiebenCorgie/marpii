use marpii::{util::ImageRegion, ash::vk};
use marpii_rmg::ImageHandle;



///Blends two images based on their alpha channel creating a third image.
///
/// Note that the images are always assumed to be 2D. If either of the regions specified by `*_offset` and `extent` exceed the images bound, UB can
/// ocure.
//TODO: Check for other image types and start different blend shader? Or create different task.
pub struct AlphaBlend{
    ///Image one
    pub src_one: ImageHandle,
    ///Offset into the `src_one` image. Can be used to blend a sub-area of the image.
    pub src_one_offset: vk::Offset2D,
    ///Image two
    pub src_two: ImageHandle,
    ///Offset into the `src_two` image. Can be used to blend a sub-area of the image.
    pub src_two_offset: vk::Offset2D,

    ///The output image. The images `extent` also specifies the size of the blending region.
    pub target: ImageHandle,
}


impl AlphaBlend{

    ///Creates the task for an target image with the specified `extent` and `format`. Note that the no blending occurs if
    /// any of the src images does not have an alpha channel.
    pub fn new(src_one: ImageHandle, src_two: ImageHandle, extent: vk::Extent2D, format: vk::Format) -> Self{
        todo!()
    }
}

//TODO implement task.
//     Currently waiting for better "registry".
