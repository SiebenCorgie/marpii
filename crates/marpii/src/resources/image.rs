use ash::vk::{self, SamplerCreateInfoBuilder};

use crate::{
    allocator::{Allocation, Allocator, AnonymAllocation, ManagedAllocation, MemoryUsage},
    context::Device,
    resources::SharingMode,
    util::ImageRegion,
};
use std::{
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageType {
    Tex1d,
    Tex1dArray(u32),
    Tex2d,
    ///Array of 2d textures, u32 is number of layers
    Tex2dArray(u32),
    Tex3d,
    Tex3dArray(u32),
    Cube,
    CubeArray(u32),
}

impl ImageType {
    ///Modifies `extent` based on `self` to be valid. For instance sets height and depth to 1 for a 1d image
    pub fn valid_extent(&self, extent: ash::vk::Extent3D) -> ash::vk::Extent3D {
        match self {
            ImageType::Tex1d => ash::vk::Extent3D {
                width: extent.width,
                height: 1,
                depth: 1,
            },
            ImageType::Tex1dArray(_) => ash::vk::Extent3D {
                width: extent.width,
                height: 1,
                depth: 1,
            },
            ImageType::Tex2d => ash::vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            },
            ImageType::Tex2dArray(_) => ash::vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            },
            ImageType::Tex3d => extent,
            ImageType::Tex3dArray(_) => extent,
            ImageType::Cube => ash::vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            },
            ImageType::CubeArray(_) => ash::vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            },
        }
    }

    ///Returns the correct number of layers for this image type
    pub fn layer_count(&self) -> u32 {
        match self {
            ImageType::Tex1d => 1,
            ImageType::Tex1dArray(i) => *i,
            ImageType::Tex2d => 1,
            ImageType::Tex2dArray(i) => *i,
            ImageType::Tex3d => 1,
            ImageType::Tex3dArray(i) => *i,
            ImageType::Cube => 6,
            ImageType::CubeArray(i) => 6 * i,
        }
    }
}

impl From<ImageType> for ash::vk::ImageType {
    fn from(ty: ImageType) -> ash::vk::ImageType {
        match ty {
            ImageType::Tex1d => ash::vk::ImageType::TYPE_1D,
            ImageType::Tex1dArray(_) => ash::vk::ImageType::TYPE_1D,
            ImageType::Tex2d => ash::vk::ImageType::TYPE_2D,
            ImageType::Tex2dArray(_) => ash::vk::ImageType::TYPE_2D,
            ImageType::Tex3d => ash::vk::ImageType::TYPE_3D,
            ImageType::Tex3dArray(_) => ash::vk::ImageType::TYPE_3D,
            ImageType::Cube => ash::vk::ImageType::TYPE_2D,
            ImageType::CubeArray(_) => ash::vk::ImageType::TYPE_3D,
        }
    }
}

///Describes all static parameters of an image view. The easiest way is to create the view description via a
/// helper function on an image. This fills in all parameters with default value. Those can then be changed base don the needed
/// usage. Usually only the subresource range is changed.
pub struct ImgViewDesc {
    pub view_type: ash::vk::ImageViewType,
    pub format: ash::vk::Format,
    pub component_mapping: ash::vk::ComponentMapping,
    pub range: ash::vk::ImageSubresourceRange,
}

impl ImgViewDesc {
    ///Overwrites all fields (that apply) of `build` with the data in `self`
    pub fn set_on_builder<'a>(
        &'a self,
        builder: ash::vk::ImageViewCreateInfoBuilder<'a>,
    ) -> ash::vk::ImageViewCreateInfoBuilder<'a> {
        builder
            .components(self.component_mapping)
            .view_type(self.view_type)
            .format(self.format)
            .subresource_range(self.range)
    }
    pub fn with_aspect(mut self, aspect_flag: ash::vk::ImageAspectFlags) -> Self {
        self.range.aspect_mask |= aspect_flag;
        self
    }
}

///[ash::vk::ImageView](ash::vk::ImageView) wrapper that safes its description data, source image and destroys itself when not in use anymore.
pub struct ImageView {
    pub desc: ImgViewDesc,
    pub device: Arc<crate::context::Device>,
    pub view: ash::vk::ImageView,
    pub src_img: Arc<Image>,
}

impl Drop for ImageView {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_image_view(self.view, None) };
    }
}

///Image description. Collects all meta data related to an [Image](Image).
///
/// This is basically a [ImageCreateInfo](ash::vk::ImageCreateInfo) where creation-time specifics like the `push_next` chain or
/// ImageCreateFlags are removed. Therefore, follow the linked vulkan specification if you want to create an image that is not
/// "standard".
///
///
/// In most cases the provided helper function should cover 99% of the use cases.
#[derive(Clone, Debug)]
pub struct ImgDesc {
    pub img_type: ImageType,
    pub format: ash::vk::Format,
    pub extent: ash::vk::Extent3D,
    pub mip_levels: u32,
    pub samples: ash::vk::SampleCountFlags,
    pub tiling: ash::vk::ImageTiling,
    pub usage: ash::vk::ImageUsageFlags,
    pub sharing_mode: SharingMode,
}

impl Default for ImgDesc {
    ///Creates a convervative image desciption for a 2d 8bit 4-channel image without mipmapping or multisampling.
    /// with an extend of 512x512
    fn default() -> Self {
        ImgDesc {
            img_type: ImageType::Tex2d,
            format: ash::vk::Format::R8G8B8A8_UINT,
            extent: ash::vk::Extent3D {
                width: 512,
                height: 512,
                depth: 1,
            },
            mip_levels: 1,
            samples: ash::vk::SampleCountFlags::TYPE_1,
            tiling: ash::vk::ImageTiling::OPTIMAL,
            usage: ash::vk::ImageUsageFlags::COLOR_ATTACHMENT,
            sharing_mode: SharingMode::Exclusive,
        }
    }
}

impl ImgDesc {
    ///Converts an [ImageType](ash::vk::ImageType) to an [ImageViewType](ash::vk::ImageViewType).
    pub fn convert_imagety_to_image_viewty(image_type: ImageType) -> ash::vk::ImageViewType {
        match image_type {
            ImageType::Tex1d => ash::vk::ImageViewType::TYPE_1D,
            ImageType::Tex1dArray(_) => ash::vk::ImageViewType::TYPE_1D_ARRAY,
            ImageType::Tex2d => ash::vk::ImageViewType::TYPE_2D,
            ImageType::Tex2dArray(_) => ash::vk::ImageViewType::TYPE_2D_ARRAY,
            ImageType::Tex3d => ash::vk::ImageViewType::TYPE_3D,
            ImageType::Tex3dArray(_) => ash::vk::ImageViewType::TYPE_3D,
            ImageType::Cube => ash::vk::ImageViewType::CUBE,
            ImageType::CubeArray(_) => ash::vk::ImageViewType::CUBE_ARRAY,
        }
    }

    ///overwrites all infos that apply of `builder` with the data of `self`.
    pub fn set_on_builder<'a>(
        &'a self,
        mut builder: ash::vk::ImageCreateInfoBuilder<'a>,
    ) -> ash::vk::ImageCreateInfoBuilder<'a> {
        builder = builder
            .image_type(self.img_type.into())
            .format(self.format)
            .extent(self.img_type.valid_extent(self.extent))
            .mip_levels(self.mip_levels)
            .array_layers(self.img_type.layer_count())
            .samples(self.samples)
            .tiling(self.tiling)
            .usage(self.usage);

        match &self.sharing_mode {
            SharingMode::Exclusive => {
                builder = builder.sharing_mode(ash::vk::SharingMode::EXCLUSIVE)
            }
            SharingMode::Concurrent {
                queue_family_indices,
            } => {
                builder = builder
                    .sharing_mode(ash::vk::SharingMode::CONCURRENT)
                    .queue_family_indices(&queue_family_indices)
            }
        }

        builder
    }

    ///Appends the additional usage
    pub fn add_usage(mut self, usage: ash::vk::ImageUsageFlags) -> Self {
        self.usage |= usage;
        self
    }

    ///Creates a simple 2d image description meant as color attachment. You might have to add additional usages
    ///The only standard usage is `COLOR_ATTACHMENT`.
    pub fn color_attachment_2d(width: u32, height: u32, format: ash::vk::Format) -> Self {
        ImgDesc {
            extent: ash::vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            format,
            ..Default::default()
        }
    }
    ///Creates a simple 2d image description meant as depth attachment. You might have to add additional usages
    ///The only standard usage is `DEPTH_ATTACHMENT`.
    pub fn depth_attachment_2d(width: u32, height: u32, format: ash::vk::Format) -> Self {
        ImgDesc {
            extent: ash::vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            format,
            usage: ash::vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            ..Default::default()
        }
    }

    ///Creates a simple storage image that has the storage bit set as well as transfere bits.
    pub fn storage_image_2d(width: u32, height: u32, format: ash::vk::Format) -> Self {
        ImgDesc {
            extent: ash::vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            format,
            usage: ash::vk::ImageUsageFlags::STORAGE
                | ash::vk::ImageUsageFlags::TRANSFER_SRC
                | ash::vk::ImageUsageFlags::TRANSFER_DST,
            ..Default::default()
        }
    }


    ///Creates a simple texture image that has the sampeld bit set as well as transfere bits.
    pub fn texture_2d(width: u32, height: u32, format: ash::vk::Format) -> Self {
        ImgDesc {
            extent: ash::vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            format,
            usage: ash::vk::ImageUsageFlags::SAMPLED
                | ash::vk::ImageUsageFlags::TRANSFER_DST,
            ..Default::default()
        }
    }

    //TODO: add more complex init methodes. for instance difference between 2dArray vs 3d images
    //      or cube maps.
}

///Self managing image that uses the allocator `A` to allocate and free its bound memory.
//Note Freeing happens in `ManagedAllocation`'s implementation.
pub struct Image {
    ///vulkan image handle
    pub inner: ash::vk::Image,
    ///assosiated allocation that is freed when the image is dropped
    pub allocation: Box<dyn AnonymAllocation + Send + Sync + 'static>,
    pub desc: ImgDesc,
    pub usage: MemoryUsage,
    pub device: Arc<Device>,
    ///True if the image should not be destroyed on [Drop](Drop) of `Self`.
    /// This should usually be false, except for swapchain images.
    pub do_not_destroy: bool,
}

///The hash implementation is based on [Image](ash::vk::Image)'s hash.
impl Hash for Image {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.inner.hash(hasher)
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        if !self.do_not_destroy {
            unsafe { self.device.inner.destroy_image(self.inner, None) }
        }
    }
}

impl Image {
    ///creates the image based on the description and the provided flags. `extend` can be used to modifiy the image create info
    /// before it is used. For instance for pushing feature specific creation info.
    ///
    ///
    /// Note that the image is just created with an initial "Undefined" layout.
    pub fn new<A: Allocator + Send + Sync + 'static>(
        device: &Arc<Device>,
        allocator: &Arc<Mutex<A>>,
        description: ImgDesc,
        memory_usage: MemoryUsage,
        name: Option<&str>,
        create_flags: Option<ash::vk::ImageCreateFlags>,
    ) -> Result<Self, anyhow::Error> {
        //per definition the image layout is undefined when creating an image.
        let initial_layout = ash::vk::ImageLayout::UNDEFINED;

        let mut builder = ash::vk::ImageCreateInfo::builder().initial_layout(initial_layout);
        if let Some(flags) = create_flags {
            builder = builder.flags(flags);
        }

        //now apply the description
        builder = description.set_on_builder(builder);

        //Time to create the image handle
        let image = unsafe { device.inner.create_image(&builder, None)? };

        //if we got the image successfuly, retrieve allocation information and ask the allocator for an allocation fitting the image
        let allocation = allocator.lock().unwrap().allocate_image(
            &device.inner,
            name,
            &image,
            memory_usage,
            true,
        )?;

        //if allocation was successfull bind image to memory
        unsafe {
            device
                .inner
                .bind_image_memory(image, allocation.memory(), allocation.offset())?
        };

        Ok(Image {
            allocation: Box::new(ManagedAllocation {
                allocation: Some(allocation),
                allocator: allocator.clone(),
                device: device.clone(),
            }),
            desc: description,
            inner: image,
            device: device.clone(),
            usage: memory_usage,
            do_not_destroy: false,
        })
    }

    pub fn extent_3d(&self) -> ash::vk::Extent3D {
        self.desc.extent
    }

    ///In case of 3d image formats the depth is ignored.
    pub fn extent_2d(&self) -> ash::vk::Extent2D {
        ash::vk::Extent2D {
            width: self.desc.extent.width,
            height: self.desc.extent.height,
        }
    }

    ///Returns the *whole* image region
    pub fn image_region(&self) -> ImageRegion {
        ImageRegion {
            offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            extent: self.extent_3d(),
        }
    }

    ///Returns a sub resource range that encloses the whole image.
    pub fn subresource_all(&self) -> ash::vk::ImageSubresourceRange {
        ash::vk::ImageSubresourceRange {
            aspect_mask: if self
                .desc
                .usage
                .contains(ash::vk::ImageUsageFlags::COLOR_ATTACHMENT)
            {
                ash::vk::ImageAspectFlags::COLOR
            } else if self
                .desc
                .usage
                .contains(ash::vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            {
                //use depth only aspect flag for depth only format, otherwise use both flags
                match self.desc.format{
                    vk::Format::D16_UNORM | vk::Format::D32_SFLOAT => ash::vk::ImageAspectFlags::DEPTH,
                    _ => ash::vk::ImageAspectFlags::DEPTH | ash::vk::ImageAspectFlags::STENCIL
                }
            } else {
                #[cfg(feature = "logging")]
                log::warn!("Could not find COLOR_ATTACHMENT nor DEPTH_STENCIL_ATTACHMENT bit while trying to decide for an initial aspect mask. Using COLOR.");
                ash::vk::ImageAspectFlags::COLOR
            },
            base_array_layer: 0,
            base_mip_level: 0,
            layer_count: self.desc.img_type.layer_count(),
            level_count: self.desc.mip_levels,
        }
    }

    ///Creates a subresource layer for the first mip level. It is choosen based on `Self::subresource_all`'s nase_mip_level.
    pub fn subresource_layers_all(&self) -> ash::vk::ImageSubresourceLayers {
        let ash::vk::ImageSubresourceRange {
            aspect_mask,
            base_array_layer,
            layer_count,
            base_mip_level,
            ..
        } = self.subresource_all();
        ash::vk::ImageSubresourceLayers {
            aspect_mask,
            base_array_layer,
            layer_count,
            mip_level: base_mip_level,
        }
    }

    ///Creates an [ImgViewDesc](ImgViewDesc) that encloses the whole image.
    pub fn view_all(&self) -> ImgViewDesc {
        ImgViewDesc {
            component_mapping: ash::vk::ComponentMapping {
                r: ash::vk::ComponentSwizzle::R,
                g: ash::vk::ComponentSwizzle::G,
                b: ash::vk::ComponentSwizzle::B,
                a: ash::vk::ComponentSwizzle::A,
            },
            format: self.desc.format,
            range: self.subresource_all(),
            view_type: ImgDesc::convert_imagety_to_image_viewty(self.desc.img_type),
        }
    }

    //TODO: create helper function like initializing the image based on data
    //      or event better, typed data. So that you can specify "create 32bit float image from this 8bit uint src"
    //      probably by uploading data to a buffer, copying this to an image, and then blitting the image to the final initialized
    //      image.
}

///If implemented, creates a self managing image view that keeps its source image and device alive long enough
/// to destroy the inner view when dropped.
pub trait SafeImageView {
    fn view(&self, device: &Arc<Device>, desc: ImgViewDesc) -> Result<ImageView, anyhow::Error>;
}

impl SafeImageView for Arc<Image> {
    ///Creates an image view for this image based on the based `desc`.
    fn view(&self, device: &Arc<Device>, desc: ImgViewDesc) -> Result<ImageView, anyhow::Error> {
        let mut builder = ash::vk::ImageViewCreateInfo::builder().image(self.inner);
        builder = desc.set_on_builder(builder);

        let view = unsafe { device.inner.create_image_view(&builder, None)? };

        Ok(ImageView {
            desc,
            device: device.clone(),
            view,
            src_img: self.clone(),
        })
    }
}

pub struct Sampler {
    pub inner: ash::vk::Sampler,
    pub device: Arc<Device>,
}

impl Sampler {
    pub fn new(
        device: &Arc<Device>,
        create_info: &SamplerCreateInfoBuilder,
    ) -> Result<Self, anyhow::Error> {
        let sampler = unsafe { device.inner.create_sampler(create_info, None)? };

        Ok(Sampler {
            device: device.clone(),
            inner: sampler,
        })
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_sampler(self.inner, None) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_impl_all;

    #[test]
    fn impl_send_sync() {
        assert_impl_all!(Image: Send, Sync);
        assert_impl_all!(ImageView: Send, Sync);
        assert_impl_all!(Sampler: Send, Sync);
    }
}
