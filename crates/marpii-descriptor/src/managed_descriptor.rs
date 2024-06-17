use std::{
    fmt::{Debug, Display},
    sync::Arc,
};

use marpii::{
    ash::{self, vk::DescriptorType},
    context::Device,
    resources::{
        Buffer, DescriptorAllocator, DescriptorPool, DescriptorSet, DescriptorSetLayout, Image,
        ImageView, SafeImageView, Sampler,
    },
    DescriptorError, OoS,
};

pub enum BindingError {
    ///If the binding type did not match
    DescriptorTypeDoesNotMatch(Binding),
    ///If binding failed while calling vulkan
    BindFailed {
        error: ash::vk::Result,
        binding: Binding,
    },
    ///If the DescriptorType matched, but the image's layout dit not
    ImageLayoutMissmatch(Binding),
    ///If the binding_id was not found
    NoSuchId(Binding),
}

impl Debug for BindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindingError::DescriptorTypeDoesNotMatch(_) => {
                f.write_str("Descriptor type does not match")
            }
            BindingError::BindFailed { error, binding: _ } => {
                f.write_str(&format!("Binding failed with: {}", error))
            }
            BindingError::ImageLayoutMissmatch(_) => f.write_str("Image layout missmatch"),
            BindingError::NoSuchId(_) => f.write_str("No such id"),
        }
    }
}

impl Display for BindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindingError::DescriptorTypeDoesNotMatch(_) => {
                f.write_str("Descriptor type does not match")
            }
            BindingError::BindFailed { error, binding: _ } => {
                f.write_str(&format!("Binding failed with: {}", error))
            }
            BindingError::ImageLayoutMissmatch(_) => f.write_str("Image layout missmatch"),
            BindingError::NoSuchId(_) => f.write_str("No such id"),
        }
    }
}

impl std::error::Error for BindingError {}

///Represent a single binding in the descriptor set.
pub enum Binding {
    Image {
        ///Descriptor type. By default STORAGE_IMAGE
        ty: ash::vk::DescriptorType,
        layout: ash::vk::ImageLayout,
        image: Arc<ImageView>,
    },
    SampledImage {
        ///Descriptor type. By default SAMPLED_IMAGE
        ty: ash::vk::DescriptorType,
        layout: ash::vk::ImageLayout,
        image: Arc<ImageView>,
        sampler: Arc<Sampler>,
    },
    //TODO: expose offset and range?
    Buffer {
        ///Descriptor type. By default STORAGE_BUFFER
        ty: ash::vk::DescriptorType,
        buffer: Arc<Buffer>,
    },
    Sampler {
        sampler: Arc<Sampler>,
    }, //TODO implement array versions as well
}

impl Binding {
    pub fn new_image(image: Arc<ImageView>, layout: ash::vk::ImageLayout) -> Self {
        Binding::Image {
            layout,
            image,
            ty: ash::vk::DescriptorType::STORAGE_IMAGE,
        }
    }

    pub fn new_whole_image(image: OoS<Image>, layout: ash::vk::ImageLayout) -> Self {
        let view_info = image.view_all();
        let view = image.view(view_info).unwrap();
        Self::new_image(Arc::new(view), layout)
    }

    pub fn new_sampled_image(
        image: Arc<ImageView>,
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

    pub fn new_whole_sampled_image(
        image: OoS<Image>,
        layout: ash::vk::ImageLayout,
        sampler: Arc<Sampler>,
    ) -> Self {
        let view_info = image.view_all();
        let view = image.view(view_info).unwrap();
        Self::new_sampled_image(Arc::new(view), layout, sampler)
    }

    pub fn new_buffer(buffer: Arc<Buffer>) -> Self {
        Binding::Buffer {
            buffer,
            ty: ash::vk::DescriptorType::STORAGE_BUFFER,
        }
    }

    pub fn new_sampler(sampler: Arc<Sampler>) -> Self {
        Binding::Sampler { sampler }
    }

    pub fn into_raw<'a>(
        &'a self,
        binding_id: u32,
        stage_flags: ash::vk::ShaderStageFlags,
    ) -> ash::vk::DescriptorSetLayoutBinding<'a> {
        match self {
            Binding::Image { ty, .. } => ash::vk::DescriptorSetLayoutBinding::default()
                .binding(binding_id)
                .stage_flags(stage_flags)
                .descriptor_count(1)
                .descriptor_type(*ty),
            Binding::SampledImage { ty, .. } => ash::vk::DescriptorSetLayoutBinding::default()
                .binding(binding_id)
                .stage_flags(stage_flags)
                .descriptor_count(1)
                .descriptor_type(*ty),
            Binding::Buffer { ty, .. } => ash::vk::DescriptorSetLayoutBinding::default()
                .binding(binding_id)
                .stage_flags(stage_flags)
                .descriptor_count(1)
                .descriptor_type(*ty),
            Binding::Sampler { .. } => ash::vk::DescriptorSetLayoutBinding::default()
                .binding(binding_id)
                .stage_flags(stage_flags)
                .descriptor_count(1)
                .descriptor_type(DescriptorType::SAMPLER),
        }
    }
}

///Wrapps the standard [DescriptorSet](marpii::resources::DescriptorSet) and keeps all resources that have
/// been written to the set alive.
pub struct ManagedDescriptorSet {
    #[allow(dead_code)]
    inner: DescriptorSet,
    ///bound resources
    #[allow(dead_code)]
    bindings: Vec<Binding>,
    ///needs to be kept alive for valid updates later on
    #[allow(dead_code)]
    layout: DescriptorSetLayout,
}

impl ManagedDescriptorSet {
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
        pool: OoS<DescriptorPool>,
        layout: impl IntoIterator<Item = Binding>,
        stages: ash::vk::ShaderStageFlags,
    ) -> Result<Self, DescriptorError> {
        let bindings = layout.into_iter().collect::<Vec<_>>();
        let layout_bindings = bindings
            .iter()
            .enumerate()
            .map(|(idx, b)| b.into_raw(idx as u32, stages))
            .collect::<Vec<_>>();
        let layout = DescriptorSetLayout::new(device, &layout_bindings)?;

        //allocate descriptorset based on layout
        let set = pool.allocate(&layout.inner)?;

        //setup Self and issue the writes
        let mut s = Self {
            inner: set,
            bindings,
            layout,
        };

        s.write_all();

        Ok(s)
    }

    ///Updates `binding_id` with `binding`. Fails if the bindings descriptor type does not match the decriptor layouts type at that binding. For instance if you try to bind a buffer to a STORAGE_IMAGE binding.
    ///
    /// Otherwise the old resource bound at that id is returned
    pub fn update_binding(
        &mut self,
        mut binding: Binding,
        binding_id: u32,
    ) -> Result<Binding, BindingError> {
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
            (Binding::Sampler { sampler: s_old }, Binding::Sampler { sampler: s_new }) => {
                std::mem::swap(s_old, s_new);
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
                let imginfo = [ash::vk::DescriptorImageInfo::default()
                    .image_layout(*layout)
                    .image_view(image.view)];
                let write = ash::vk::WriteDescriptorSet::default()
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
                let imginfo = [ash::vk::DescriptorImageInfo::default()
                    .image_layout(*layout)
                    .image_view(image.view)
                    .sampler(sampler.inner)];
                let write = ash::vk::WriteDescriptorSet::default()
                    .image_info(&imginfo)
                    .descriptor_type(*ty)
                    .dst_binding(id as u32)
                    .dst_set(self.inner.inner);

                self.inner.write(write);
            }
            Binding::Buffer { ty, buffer } => {
                let bufferinfo = [ash::vk::DescriptorBufferInfo::default()
                    .buffer(buffer.inner)
                    .offset(0)
                    .range(ash::vk::WHOLE_SIZE)];
                let write = ash::vk::WriteDescriptorSet::default()
                    .buffer_info(&bufferinfo)
                    .descriptor_type(*ty)
                    .dst_binding(id as u32)
                    .dst_set(self.inner.inner);

                self.inner.write(write);
            }
            Binding::Sampler { sampler } => {
                let imginfo = [ash::vk::DescriptorImageInfo::default().sampler(sampler.inner)];
                let write = ash::vk::WriteDescriptorSet::default()
                    .image_info(&imginfo)
                    .descriptor_type(ash::vk::DescriptorType::SAMPLER)
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
