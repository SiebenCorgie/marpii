use std::sync::Arc;

use marpii::{
    allocator::Allocator,
    ash,
    context::Device,
    resources::{
        Buffer, DescriptorAllocator, DescriptorSet, DescriptorSetLayout, ImageView, Sampler,
    },
};

pub enum BindingError<A: Allocator + Send + Sync + 'static> {
    ///If the binding type did not match
    DescriptorTypeDoesNotMatch(Binding<A>),
    ///If binding failed while calling vulkan
    BindFailed {
        error: ash::vk::Result,
        binding: Binding<A>,
    },
    ///If the DescriptorType matched, but the image's layout dit not
    ImageLayoutMissmatch(Binding<A>),
    ///If the binding_id was not found
    NoSuchId(Binding<A>),
}

///Represent a single binding in the descriptor set.
pub enum Binding<A: Allocator + Send + Sync + 'static> {
    Image {
        ///Descriptor type. By default STORAGE_IMAGE
        ty: ash::vk::DescriptorType,
        layout: ash::vk::ImageLayout,
        image: Arc<ImageView<A>>,
    },
    SampledImage {
        ///Descriptor type. By default SAMPLED_IMAGE
        ty: ash::vk::DescriptorType,
        layout: ash::vk::ImageLayout,
        image: Arc<ImageView<A>>,
        sampler: Arc<Sampler>,
    },
    //TODO: expose offset and range?
    Buffer {
        ///Descriptor type. By default STORAGE_BUFFER
        ty: ash::vk::DescriptorType,
        buffer: Arc<Buffer<A>>,
    },
    //TODO implement array versions as well
}

impl<A: Allocator + Send + Sync + 'static> Binding<A> {
    pub fn new_image(image: Arc<ImageView<A>>, layout: ash::vk::ImageLayout) -> Self {
        Binding::Image {
            layout,
            image,
            ty: ash::vk::DescriptorType::STORAGE_IMAGE,
        }
    }
    pub fn new_sampled_image(
        image: Arc<ImageView<A>>,
        layout: ash::vk::ImageLayout,
        sampler: Arc<Sampler>,
    ) -> Self {
        Binding::SampledImage {
            layout,
            image,
            sampler,
            ty: ash::vk::DescriptorType::SAMPLED_IMAGE,
        }
    }
    pub fn new_buffer(buffer: Arc<Buffer<A>>) -> Self {
        Binding::Buffer {
            buffer,
            ty: ash::vk::DescriptorType::STORAGE_BUFFER,
        }
    }

    pub fn into_raw<'a>(
        &'a self,
        binding_id: u32,
        stage_flags: ash::vk::ShaderStageFlags,
    ) -> ash::vk::DescriptorSetLayoutBindingBuilder<'a> {
        match self {
            Binding::Image { ty, .. } => ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(binding_id)
                .stage_flags(stage_flags)
                .descriptor_count(1)
                .descriptor_type(*ty),
            Binding::SampledImage { ty, .. } => ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(binding_id)
                .stage_flags(stage_flags)
                .descriptor_count(1)
                .descriptor_type(*ty),
            Binding::Buffer { ty, .. } => ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(binding_id)
                .stage_flags(stage_flags)
                .descriptor_count(1)
                .descriptor_type(*ty),
        }
    }
}

///Wrapps the standard [DescriptorSet](marpii::resources::DescriptorSet) and keeps all resources that have
/// been written to the set alive.
pub struct ManagedDescriptorSet<A: Allocator + Send + Sync + 'static, P: DescriptorAllocator> {
    #[allow(dead_code)]
    inner: DescriptorSet<P>,
    ///bound resources
    #[allow(dead_code)]
    bindings: Vec<Binding<A>>,
}

impl<A: Allocator + Send + Sync + 'static, P: DescriptorAllocator> ManagedDescriptorSet<A, P> {
    ///creates a descriptorset that binds each item to the descriptor set.
    ///
    /// The binding id is derived from the location of each binding in the iterator.
    ///
    /// # Fail
    /// Fails if the pool has not enough descriptors left to bind each binding in `layout`. To prevent that, use the
    /// DynamicPool
    //TODO: also expose a version where the user can set the binding id?
    pub fn new(
        device: &Arc<Device>,
        pool: P,
        layout: impl IntoIterator<Item = Binding<A>>,
        stages: ash::vk::ShaderStageFlags,
    ) -> Result<Self, anyhow::Error> {
        let bindings = layout.into_iter().collect::<Vec<_>>();
        let layout_bindings = bindings
            .iter()
            .enumerate()
            .map(|(idx, b)| *b.into_raw(idx as u32, stages))
            .collect::<Vec<_>>();
        let layout = DescriptorSetLayout::new(device, &layout_bindings)?;

        //allocate descriptorset based on layout
        let set = pool.allocate(&layout.inner)?;

        //setup Self and issue the writes
        let mut s = Self {
            inner: set,
            bindings,
        };

        s.write_all();

        Ok(s)
    }

    ///Updates `binding_id` with `binding`. Fails if the bindings descriptor type does not match the decriptor layouts type at that binding. For instance if you try to bind a buffer to a STORAGE_IMAGE binding.
    ///
    /// Otherwise the old resource bound at that id is returned
    pub fn update_binding(
        &mut self,
        mut binding: Binding<A>,
        binding_id: u32,
    ) -> Result<Binding<A>, BindingError<A>> {
        let binding_id = binding_id as usize;
        if binding_id > self.bindings.len() {
            return Err(BindingError::NoSuchId(binding));
        }

        match (&mut self.bindings[binding_id], &mut binding) {
            (
                Binding::Image { image, layout, ty },
                Binding::Image {
                    image: oimage,
                    layout: olayout,
                    ty: oty,
                },
            ) => {
                //check that type and layout match
                if layout == olayout && ty == oty {
                    //swap
                    std::mem::swap(image, oimage);
                } else {
                    return Err(BindingError::ImageLayoutMissmatch(binding));
                }
            }
            (
                Binding::SampledImage {
                    image,
                    layout,
                    sampler,
                    ty,
                },
                Binding::SampledImage {
                    image: oimage,
                    layout: olayout,
                    sampler: osampler,
                    ty: oty,
                },
            ) => {
                if layout == olayout && ty == oty {
                    //swap
                    std::mem::swap(image, oimage);
                    std::mem::swap(sampler, osampler);
                } else {
                    return Err(BindingError::ImageLayoutMissmatch(binding));
                }
            }
            (
                Binding::Buffer { buffer, ty },
                Binding::Buffer {
                    buffer: obuffer,
                    ty: oty,
                },
            ) => {
                if ty == oty {
                    std::mem::swap(buffer, obuffer);
                } else {
                    return Err(BindingError::ImageLayoutMissmatch(binding));
                }
            }
            _ => return Err(BindingError::DescriptorTypeDoesNotMatch(binding)),
        }

        self.write_binding(binding_id);

        //if everything workt out well we swapped the actual resources above.
        Ok(binding)
    }

    //writes a single binding
    fn write_binding(&mut self, id: usize) {
        if id > self.bindings.len() {
            return;
        }

        match &self.bindings[id] {
            Binding::Image { layout, image, ty } => {
                let imginfo = [ash::vk::DescriptorImageInfo::builder()
                    .image_layout(*layout)
                    .image_view(image.view)
                    .build()];
                let write = ash::vk::WriteDescriptorSet::builder()
                    .image_info(&imginfo)
                    .descriptor_type(*ty)
                    .dst_binding(id as u32)
                    .dst_set(self.inner.inner);

                self.inner.write(write);
            }
            Binding::SampledImage {
                ty,
                layout,
                image,
                sampler,
            } => {
                let imginfo = [ash::vk::DescriptorImageInfo::builder()
                    .image_layout(*layout)
                    .image_view(image.view)
                    .sampler(sampler.inner)
                    .build()];
                let write = ash::vk::WriteDescriptorSet::builder()
                    .image_info(&imginfo)
                    .descriptor_type(*ty)
                    .dst_binding(id as u32)
                    .dst_set(self.inner.inner);

                self.inner.write(write);
            }
            Binding::Buffer { ty, buffer } => {
                let bufferinfo = [ash::vk::DescriptorBufferInfo::builder()
                    .buffer(buffer.inner)
                    .build()];
                let write = ash::vk::WriteDescriptorSet::builder()
                    .buffer_info(&bufferinfo)
                    .descriptor_type(*ty)
                    .dst_binding(id as u32)
                    .dst_set(self.inner.inner);

                self.inner.write(write);
            }
        };
    }

    fn write_all(&mut self) {
        for i in 0..self.bindings.len() {
            self.write_binding(i)
        }
    }

    ///Returns the raw vulkan handle to this descriptor
    pub fn raw(&self) -> &ash::vk::DescriptorSet {
        &self.inner.inner
    }
}
