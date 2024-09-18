use crossbeam_channel::{Receiver, Sender};
use marpii::{
    ash::vk,
    context::Device,
    resources::{
        BufDesc, Buffer, BufferMapError, Image, ImgDesc, PipelineLayout, SafeImageView, Sampler,
    },
    MarpiiError, OoS,
};
use slotmap::SlotMap;
use std::{marker::PhantomData, sync::Arc};
use thiserror::Error;

use crate::{
    resources::{
        descriptor::{Bindless, ResourceHandle},
        res_states::{
            BufferKey, ImageKey, QueueOwnership, ResBuffer, ResImage, ResSampler, SamplerKey,
        },
    },
    track::Tracks,
    BufferHandle, ImageHandle, SamplerHandle,
};

use self::{handle::AnyHandle, res_states::AnyResKey};

pub(crate) mod descriptor;
pub(crate) mod handle;
pub(crate) mod res_states;

#[derive(Debug, Error)]
pub enum ResourceError {
    #[error("vulkan error")]
    VkError(#[from] vk::Result),

    #[error("Marpii internal error: {0}")]
    MarpiiError(#[from] MarpiiError),

    #[error("Resource already existed")]
    ResourceExists(AnyHandle),

    #[error("Resource {0:?} was already bound to {1:?}")]
    AlreadyBound(AnyHandle, ResourceHandle),

    #[error("Image has both, SAMPLED and STORAGE flags set")]
    ImageIntersectingUsageFlags,

    #[error("Image has none of SAMPLED and STORAGE flags set. Can't decide which to use")]
    ImageNoUsageFlags,

    #[error("Resource has no usage flag set that allows access of type {0:?}")]
    InvalidAccess(vk::AccessFlags2),

    #[error("Binding a resource failed")]
    BindingFailed,

    #[error("Failed to get new swapchain image")]
    SwapchainError,

    #[error("There is no Track for queue family {0}")]
    NoTrackForQueueFamily(u32),

    #[error("Resource was already requested for the registry.")]
    ResourceAlreadyRequested,

    #[error("Buffer mapping error while accessing resource: {0}")]
    BufferMapError(BufferMapError),
}

///Rmg's resource management. This bundles all state that outlifes a single frame. Meaning Images, buffers and samplers.
/// It also caches the bindless resources and takes care that only resources for which no handle exists anymore are retired.
pub struct Resources {
    pub(crate) bindless: Bindless,
    pub(crate) bindless_layout: Arc<PipelineLayout>,

    pub(crate) images: SlotMap<ImageKey, ResImage>,
    pub(crate) buffer: SlotMap<BufferKey, ResBuffer>,
    pub(crate) sampler: SlotMap<SamplerKey, ResSampler>,
    ///Channel used by the handles to signal their drop.
    #[allow(dead_code)]
    pub(crate) handle_drop_channel: (Sender<AnyResKey>, Receiver<AnyResKey>),
}

impl Resources {
    pub fn new(device: &Arc<Device>) -> Result<Self, ResourceError> {
        let bindless = Bindless::new_default(device)?;
        let bindless_layout = Arc::new(bindless.new_pipeline_layout(&[]));
        let handle_drop_channel = crossbeam_channel::unbounded();

        Ok(Resources {
            bindless,
            bindless_layout,
            buffer: SlotMap::with_key(),
            images: SlotMap::with_key(),
            sampler: SlotMap::with_key(),
            handle_drop_channel,
        })
    }

    pub fn bindless_layout(&self) -> Arc<PipelineLayout> {
        self.bindless_layout.clone()
    }

    ///Binds the resource for use on the gpu.
    fn bind(&mut self, res: impl Into<AnyResKey>) -> Result<ResourceHandle, ResourceError> {
        let res = res.into();
        match res {
            AnyResKey::Buffer(buf) => {
                let buffer = self.buffer.get_mut(buf).unwrap();
                if let Some(hdl) = &buffer.descriptor_handle {
                    return Err(ResourceError::AlreadyBound(res.into(), *hdl));
                }
                buffer.descriptor_handle = Some(
                    self.bindless
                        .bind_storage_buffer(buffer.buffer.clone())
                        .map_err(|_| ResourceError::BindingFailed)?,
                );
                Ok(buffer.descriptor_handle.unwrap())
            }
            AnyResKey::Image(img) => {
                let image = self.images.get_mut(img).unwrap();
                if let Some(hdl) = &image.descriptor_handle {
                    return Err(ResourceError::AlreadyBound(res.into(), *hdl));
                }

                match (image.is_sampled_image(), image.is_storage_image()) {
                    (true, true) => {
                        #[cfg(feature = "logging")]
                        log::info!("Binding image to both, sampled, and storage descriptor set");
                        image.descriptor_handle = Some(
                            self.bindless
                                .bind_sampled_storage_image(image.view.clone())
                                .map_err(|_| ResourceError::BindingFailed)?,
                        );
                    }
                    (true, false) => {
                        image.descriptor_handle = Some(
                            self.bindless
                                .bind_sampled_image(image.view.clone())
                                .map_err(|_| ResourceError::BindingFailed)?,
                        );
                    }
                    (false, true) => {
                        image.descriptor_handle = Some(
                            self.bindless
                                .bind_storage_image(image.view.clone())
                                .map_err(|_| ResourceError::BindingFailed)?,
                        );
                    }
                    (false, false) => {
                        #[cfg(feature = "logging")]
                        log::error!("Failed to bind image, neither STORAGE, nor SAMPLED bit set");
                        return Err(ResourceError::BindingFailed);
                    }
                }

                Ok(image.descriptor_handle.unwrap())
            }
            AnyResKey::Sampler(sam) => {
                let sampler = self.sampler.get_mut(sam).unwrap();
                if let Some(hdl) = &sampler.descriptor_handle {
                    return Err(ResourceError::AlreadyBound(res.into(), *hdl));
                }
                sampler.descriptor_handle = Some(
                    self.bindless
                        .bind_sampler(sampler.sampler.clone())
                        .map_err(|_| ResourceError::BindingFailed)?,
                );
                Ok(sampler.descriptor_handle.unwrap())
            }
        }
    }

    ///Adds an image, assuming it is uninitialised. If the image is initialised, owned by a queue or similar,
    /// use the [import](crate::Rmg::new_image_uninitialized) function instead.
    pub fn add_image(
        &mut self,
        image: impl Into<OoS<Image>>,
    ) -> Result<ImageHandle, ResourceError> {
        //always convert into arc since rmg is using arcs everywhere incl.
        // the garbage collector
        let mut image: OoS<Image> = image.into();
        let image_arc: Arc<Image> = image.share().shared_arc();
        let image_view_desc = image.view_all();
        let image_view = Arc::new(
            image
                .view(image_view_desc)
                .map_err(|e| ResourceError::MarpiiError(e.into()))?,
        );

        let key = self.images.insert(ResImage {
            image: image_arc.clone(),
            view: image_view,
            ownership: QueueOwnership::Uninitialized,
            mask: vk::AccessFlags2::empty(),
            layout: vk::ImageLayout::UNDEFINED,
            guard: None,
            descriptor_handle: None,
        });

        Ok(ImageHandle {
            key,
            imgref: image_arc,
        })
    }

    pub fn add_sampler(&mut self, sampler: Arc<Sampler>) -> Result<SamplerHandle, ResourceError> {
        let key = self.sampler.insert(ResSampler {
            descriptor_handle: None,
            sampler: sampler.clone(),
        });

        Ok(SamplerHandle {
            key,
            samref: sampler,
        })
    }

    ///Adds an buffer, assuming it is uninitialised. If the buffer is initialised, owned by a queue or similar,
    /// use the [import](crate::Rmg::import_buffer) function instead.
    pub fn add_buffer<T: 'static>(
        &mut self,
        buffer: Arc<Buffer>,
    ) -> Result<BufferHandle<T>, ResourceError> {
        let key = self.buffer.insert(ResBuffer {
            buffer: buffer.clone(),
            ownership: QueueOwnership::Uninitialized,
            mask: vk::AccessFlags2::empty(),
            guard: None,
            descriptor_handle: None,
        });

        Ok(BufferHandle {
            key,
            bufref: buffer,
            data_type: PhantomData,
        })
    }

    ///Imports the buffer with the given state. Returns an error if a given `queue_family` index has no internal `TrackId`.
    pub(crate) fn import_buffer<T: 'static>(
        &mut self,
        tracks: &Tracks,
        buffer: Arc<Buffer>,
        queue_family: Option<u32>,
        access_flags: Option<vk::AccessFlags2>,
    ) -> Result<BufferHandle<T>, ResourceError> {
        let owner = if let Some(fam) = queue_family {
            let track = tracks.0.iter().find_map(|(track_id, track)| {
                if track.queue_idx == fam {
                    Some(track_id)
                } else {
                    None
                }
            });
            if let Some(_t) = track {
                QueueOwnership::Owned(fam)
            } else {
                return Err(ResourceError::NoTrackForQueueFamily(fam));
            }
        } else {
            QueueOwnership::Uninitialized
        };

        let access = access_flags.unwrap_or(vk::AccessFlags2::NONE);

        let key = self.buffer.insert(ResBuffer {
            buffer: buffer.clone(),
            ownership: owner,
            mask: access,
            guard: None,
            descriptor_handle: None,
        });

        Ok(BufferHandle {
            key,
            bufref: buffer,
            data_type: PhantomData,
        })
    }

    ///Imports the image with the given state. Returns an error if a given `queue_family` index has no internal `TrackId`.
    pub(crate) fn import_image(
        &mut self,
        tracks: &Tracks,
        image: impl Into<OoS<Image>>,
        queue_family: Option<u32>,
        layout: Option<vk::ImageLayout>,
        access_flags: Option<vk::AccessFlags2>,
    ) -> Result<ImageHandle, ResourceError> {
        let owner = if let Some(fam) = queue_family {
            let track = tracks.0.iter().find_map(|(track_id, track)| {
                if track.queue_idx == fam {
                    Some(track_id)
                } else {
                    None
                }
            });
            if let Some(_t) = track {
                QueueOwnership::Owned(fam)
            } else {
                return Err(ResourceError::NoTrackForQueueFamily(fam));
            }
        } else {
            QueueOwnership::Uninitialized
        };

        let access = access_flags.unwrap_or(vk::AccessFlags2::NONE);
        let layout = layout.unwrap_or(vk::ImageLayout::UNDEFINED);

        let mut image = image.into();
        let image_arc = image.share().shared_arc();
        let image_view_desc = image.share().view_all();
        let view = Arc::new(
            image
                .view(image_view_desc)
                .map_err(|e| ResourceError::MarpiiError(e.into()))?,
        );

        let key = self.images.insert(ResImage {
            image: image_arc.clone(),
            view,
            ownership: owner,
            mask: access,
            guard: None,
            layout,
            descriptor_handle: None,
        });

        Ok(ImageHandle {
            key,
            imgref: image_arc,
        })
    }

    ///Tries to get the resource's bindless handle. If not already bound, tries to bind the resource
    pub fn resource_handle_or_bind(
        &mut self,
        res: impl Into<AnyHandle> + Clone,
    ) -> Result<ResourceHandle, ResourceError> {
        if let Some(hdl) = self.try_resource_handle(res.clone()) {
            Ok(hdl)
        } else {
            let hdl = res.into();
            //try to bind, try that
            Ok(self.bind(hdl.key)?)
        }
    }

    /// Tries to find a resource for the handle. Returns None if the resource is not bound yet.
    pub fn try_resource_handle(&self, res: impl Into<AnyHandle>) -> Option<ResourceHandle> {
        let hdl = res.into();
        //NOTE: All available handles must be valid. Therefore the unwrap should not panic. If it does,
        //      it is a bug.
        let hdl = match hdl.key {
            AnyResKey::Buffer(buf) => self.buffer.get(buf).unwrap().descriptor_handle,
            AnyResKey::Image(img) => self.images.get(img).unwrap().descriptor_handle,
            AnyResKey::Sampler(sam) => self.sampler.get(sam).unwrap().descriptor_handle,
        };
        hdl
    }

    ///tick the resource manager that a new frame has started
    //TODO: Currently we use the rendering frame to do all the cleanup. In a perfect world we'd use
    //      another thread for that to not stall the recording process
    pub(crate) fn tick_record(&mut self, tracks: &Tracks) {
        self.images.retain(|#[allow(unused_variables)] key, img| {
            if img.is_orphaned() && img.guard.map_or(true, |g| g.expired(tracks)) {
                #[cfg(feature = "logging")]
                log::info!("Dropping {:?}", key);

                if let Some(hdl) = img.descriptor_handle {
                    match (img.is_sampled_image(), img.is_storage_image()){
                        (true, true) => {
                            self.bindless.remove_sampled_image(hdl);
                            self.bindless.remove_storage_image(hdl);
                        },
                        (true, false) => {

                            self.bindless.remove_sampled_image(hdl);
                        },
                        (false, true) => {
                            self.bindless.remove_storage_image(hdl);
                        },
                        (false, false) => {
                            #[cfg(feature = "logging")]
                            log::error!("Cannot drop image resource thats not STORAGE nor SAMPLED tagged: {:?}", key);
                        }
                    }
                }
                false
            } else {
                true
            }
        });

        self.buffer
            .retain(|#[allow(unused_variables)] key, buffer| {
                if buffer.is_orphaned() && buffer.guard.map_or(true, |g| g.expired(tracks)) {
                    #[cfg(feature = "logging")]
                    log::info!("Dropping {:?}", key);

                    if let Some(hdl) = buffer.descriptor_handle {
                        self.bindless.remove_storage_buffer(hdl);
                    }
                    false
                } else {
                    true
                }
            });

        self.sampler
            .retain(|#[allow(unused_variables)] key, sampler| {
                if sampler.is_orphaned() {
                    #[cfg(feature = "logging")]
                    log::info!("Dropping {:?}", key);

                    if let Some(hdl) = sampler.descriptor_handle {
                        self.bindless.remove_sampler(hdl);
                    }
                    false
                } else {
                    true
                }
            });
    }

    pub fn get_image_desc(&self, hdl: &ImageHandle) -> &ImgDesc {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        &self
            .images
            .get(hdl.key)
            .as_ref()
            .expect("Used invalid image handle")
            .image
            .desc
    }

    pub fn get_buffer_desc<T: 'static>(&self, hdl: &BufferHandle<T>) -> &BufDesc {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        &self
            .buffer
            .get(hdl.key)
            .as_ref()
            .expect("Used invalid buffer handle")
            .buffer
            .desc
    }

    ///Returns the current state of the given image.
    ///
    /// # Safety
    /// If a the state gets changed in a command buffer, make sure that the final state is the
    /// same as the initial state reported by this function. Otherwise scheduling might produce a
    /// wrong value.
    pub fn get_image_state(&self, hdl: &ImageHandle) -> &ResImage {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        self.images
            .get(hdl.key)
            .as_ref()
            .expect("Used invalid ImageHandle")
    }

    ///Returns the current state of the given buffer.
    ///
    /// # Safety
    /// If a the state gets changed in a command buffer, make sure that the final state is the
    /// same as the initial state reported by this function. Otherwise scheduling might produce a
    /// wrong value.
    pub fn get_buffer_state<T: 'static>(&self, hdl: &BufferHandle<T>) -> &ResBuffer {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        self.buffer
            .get(hdl.key)
            .as_ref()
            .unwrap_or_else(|| panic!("Used invalid BufferHandle {:?}", hdl.key))
    }
    ///Returns the current state of the given sampler.
    ///
    /// # Safety
    /// If a the state gets changed in a command buffer, make sure that the final state is the
    /// same as the initial state reported by this function. Otherwise scheduling might produce a
    /// wrong value.
    pub fn get_sampler_state(&self, hdl: &SamplerHandle) -> &ResSampler {
        //Safety: expect is ok since we controll handle creation, and based on that resource
        //        destruction. In theory it is not possible to own a handle to an destroyed
        //        resource.
        self.sampler
            .get(hdl.key)
            .as_ref()
            .expect("Used invalid Sampler Handle")
    }

    ///Returns the current owning queue (if the res exists and is NOT a sampler).
    pub(crate) fn get_current_owner(&self, res: impl Into<AnyResKey>) -> Option<u32> {
        match &res.into() {
            AnyResKey::Buffer(k) => self.buffer.get(*k).and_then(|buf| buf.ownership.owner()),
            AnyResKey::Image(k) => self.images.get(*k).and_then(|img| img.ownership.owner()),
            AnyResKey::Sampler(_) => None,
        }
    }
}
