use marpii::ash::vk::{self, ImageLayout};
use tinyvec::{Array, TinyVec};

///Barrier building helper. Lets you add barriers for images and buffers
/// via a simple builder API.
///
/// Convinient whenever building a simple array for the barriers is not possible.
///
/// Uses tinyvec internally. `N` sets the amount of barriers for each type that are pre allocated into an array. The barrier
/// however can outgrow that value.
pub struct BarrierBuilder{
    images: TinyVec<[vk::ImageMemoryBarrier2; Self::STACK_ALLCATION]>,
    buffers: TinyVec<[vk::BufferMemoryBarrier2; Self::STACK_ALLCATION]>,
}

///By default we pre allocate two barriers per type, since this is a pretty common pattern for simple
/// transitions.
impl Default for BarrierBuilder {
    fn default() -> Self {
        BarrierBuilder {
            images: TinyVec::default(),
            buffers: TinyVec::default(),
        }
    }
}

impl BarrierBuilder{

    ///Ammount of barriers that can be stack allocated.
    pub const STACK_ALLCATION: usize = 2;

    ///Creates new builder with `N` stack allocated barriers per type.
    pub fn new() -> Self{
        BarrierBuilder{
            images: TinyVec::default(),
            buffers: TinyVec::default(),
        }
    }

    ///Adds this barrier to the internal collection.
    ///
    /// # Safety
    ///
    /// Make sure that the `buffer` handle is alive until the barrier is used on the GPU.
    /// This is not enforced by this object since it is dropped whenever the commandbuffer is build. Therefore, there is no
    /// way for it to extent the lifetime as needed.
    pub fn buffer_barrier(
        mut self,
        buffer: vk::Buffer,
        offset: u64,
        size: u64,
        src_access_mask: vk::AccessFlags2,
        src_pipeline_stage: vk::PipelineStageFlags2,
        src_queue_family: u32,
        dst_access_mask: vk::AccessFlags2,
        dst_pipeline_stage: vk::PipelineStageFlags2,
        dst_queue_family: u32
    ) -> Self{
        let item = vk::BufferMemoryBarrier2::builder()
            .buffer(buffer)
            .src_access_mask(src_access_mask)
            .src_stage_mask(src_pipeline_stage)
            .src_queue_family_index(src_queue_family)
            .dst_access_mask(dst_access_mask)
            .dst_stage_mask(dst_pipeline_stage)
            .dst_queue_family_index(dst_queue_family)
            .offset(offset)
            .size(size)
            .build();
        self.buffers.push(item);

        self
    }

    ///pushes only a queue transition for the given region.
    ///
    /// # Safety see [buffer_barrier].
    pub fn buffer_queue_transition(
        mut self,
        buffer: vk::Buffer,
        offset: u64,
        size: u64,
        src_queue_family: u32,
        dst_queue_family: u32
    ) -> Self{
        let item = vk::BufferMemoryBarrier2::builder()
            .buffer(buffer)
            .src_queue_family_index(src_queue_family)
            .dst_queue_family_index(dst_queue_family)
            .offset(offset)
            .size(size)
            .build();
        self.buffers.push(item);

        self
    }

    pub fn buffer_custom_barrier(mut self, barrier: vk::BufferMemoryBarrier2) -> Self{
        self.buffers.push(barrier);
        self
    }

    ///Adds this barrier.
    ///
    /// # Safety
    ///
    /// Make sure that the `image` handle is alive until the barrier is used on the GPU.
    /// This is not enforced by this object since it is dropped whenever the commandbuffer is build. Therefore, there is no
    /// way for it to extent the lifetime as needed.
    pub fn image_barrier(
        mut self,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
        src_access_mask: vk::AccessFlags2,
        src_pipeline_stage: vk::PipelineStageFlags2,
        src_layout: vk::ImageLayout,
        src_queue_family: u32,
        dst_access_mask: vk::AccessFlags2,
        dst_pipeline_stage: vk::PipelineStageFlags2,
        dst_layout: ImageLayout,
        dst_queue_family: u32
    ) -> Self{
        let item = vk::ImageMemoryBarrier2::builder()
            .image(image)
            .subresource_range(subresource_range)
            .src_access_mask(src_access_mask)
            .src_stage_mask(src_pipeline_stage)
            .src_queue_family_index(src_queue_family)
            .old_layout(src_layout)
            .dst_access_mask(dst_access_mask)
            .dst_stage_mask(dst_pipeline_stage)
            .dst_queue_family_index(dst_queue_family)
            .new_layout(dst_layout)
            .build();
        self.images.push(item);

        self
    }


    ///pushes only a queue transition for the given region.
    ///
    /// # Safety see [image_barrier].
    pub fn image_queue_transition(
        mut self,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
        src_queue_family: u32,
        dst_queue_family: u32
    ) -> Self{
        let item = vk::ImageMemoryBarrier2::builder()
            .image(image)
            .subresource_range(subresource_range)
            .src_queue_family_index(src_queue_family)
            .dst_queue_family_index(dst_queue_family)
            .build();
        self.images.push(item);

        self
    }

    ///pushes only a layout transition for the given region.
    ///
    /// # Safety see [image_barrier].
    pub fn image_layout_transition(
        mut self,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
        src_layout: vk::ImageLayout ,
        dst_layout: ImageLayout
    ) -> Self{
        let item = vk::ImageMemoryBarrier2::builder()
            .image(image)
            .subresource_range(subresource_range)
            .old_layout(src_layout)
            .new_layout(dst_layout)
            .build();
        self.images.push(item);

        self
    }

    pub fn image_custom_barrier(mut self, barrier: vk::ImageMemoryBarrier2) -> Self{
        self.images.push(barrier);
        self
    }

    ///Returns a reference to a barrier, containig the currently pushed barriers
    // TODO: allow adding flags?
    pub fn as_dependency_info<'a>(&'a self) -> vk::DependencyInfoBuilder<'a>{
        vk::DependencyInfo::builder()
            .image_memory_barriers(self.images.as_slice())
            .buffer_memory_barriers(self.buffers.as_slice())
    }
}
