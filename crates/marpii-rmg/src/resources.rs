//! # Resources
//!
//! User facing resource definitions.
//!
//! TODO: Examples once this stabilises.
//!

use std::{marker::PhantomData, sync::Arc, time::Instant};
use fxhash::FxHashMap;
use marpii::{resources::{Buffer, Image, Sampler, BufDesc, ImgDesc}, ash::vk, context::Ctx, gpu_allocator::vulkan::Allocator, allocator::MemoryUsage};
use slotmap::SlotMap;
use thiserror::Error;

use crate::{task::Attachment, TrackId, Track, Tracks, RmgError};


pub(crate) mod handle;


slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `BufferHdl<T>` and `ImageHdl` for user facing API.
    pub struct BufferKey;
);

impl<T: 'static> From<BufferHdl<T>> for BufferKey{
    fn from(key: BufferHdl<T>) -> Self {
        key.hdl
    }
}

slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `BufferHdl<T>` and `ImageHdl` for user facing API.
    pub struct ImageKey;
);


impl From<ImageHdl> for ImageKey{
    fn from(key: ImageHdl) -> Self {
        key.hdl
    }
}

slotmap::new_key_type!(
    ///exposed keys used to reference internal data from externally. Try to use `SamplerHdl` user facing API.
    pub struct SamplerKey;
);

impl From<SamplerHdl> for SamplerKey{
    fn from(key: SamplerHdl) -> Self {
        key.hdl
    }
}



#[derive(Debug, Error)]
pub enum ResourceError {
    #[error("anyhow")]
    Any(#[from] anyhow::Error),
}

#[derive(Clone, Copy)]
pub struct BufferHdl<T: 'static>{
    pub(crate) hdl: BufferKey,
    ty: PhantomData<T>
}

#[derive(Clone, Copy)]
pub struct ImageHdl{
    pub(crate) hdl: ImageKey,
}

#[derive(Clone, Copy)]
pub struct SamplerHdl{
    pub(crate) hdl: SamplerKey,
}


//TODO: We might be able to remove the Arc's around the resources...
///Combined state of a single image.
pub(crate) struct ResImage{
    pub(crate) image: Arc<Image>,
    pub(crate) sampler: Option<SamplerHdl>,
    pub(crate) owning_family: Option<u32>,
    pub(crate) mask: vk::AccessFlags2,
    pub(crate) layout: vk::ImageLayout,

    ///Some if the image is currently guarded by some execution.
    pub(crate) guard: Option<Guard>,
}


///Combined state of a single buffer, type tagged. Note that it is valid to use a `u8` as type, which turns this buffer into a simple byte-address-buffer.
pub(crate) struct ResBuffer{
    pub(crate) buffer: Arc<Buffer>,
    pub(crate) owning_family: Option<u32>,
    pub(crate) mask: vk::AccessFlags2,

    ///Some if the image is currently guarded by some execution.
    pub(crate) guard: Option<Guard>,
}

pub(crate) struct ResSampler{
    sampler: Arc<Sampler>,
}

///Guard for some execution. Can be polled if the execution has finished or not.
pub(crate) struct Guard{
    ///The track that currently guards
    pub(crate) track: TrackId,
    ///The target semaphore value that needs to be reached on the track to free the guarded value.
    pub(crate) target_val: u64,
}



struct AttachmentReg{
    last_use: Instant,
    key: ImageKey,

}

#[derive(Debug)]
enum AnyResource{
    Image(ImageKey),
    Buffer(BufferKey),
    Sampler(SamplerKey)
}

///Represents any temporary resource and it's last activation epoch.
struct TemporaryResource{
    res: AnyResource,
    last_touch: u64,
}

///Resource handler. TODO Document.
// FIXME: Use [Synchronization2](https://www.khronos.org/blog/vulkan-sdk-offers-developers-a-smooth-transition-path-to-synchronization2)
//        For Scheduling
pub(crate) struct Resources{
    images: SlotMap<ImageKey, ResImage>,
    buffers: SlotMap<BufferKey, ResBuffer>,
    sampler: SlotMap<SamplerKey, ResSampler>,

    ///Latest resource epoch. Used to identify when a resource was last used.
    epoch: u64,

    temporary_resources: Vec<TemporaryResource>,
}


impl Resources {

    ///Maximum number of epochs until a temporary resource is dropped.
    const MAX_INACTIVE_EPOCHS: u64 = 10;

    pub(crate) fn new() -> Self{
        Resources {
            images: SlotMap::with_key(),
            buffers: SlotMap::with_key(),
            sampler: SlotMap::with_key(),

            epoch: 0,
            temporary_resources: Vec::new(),
        }
    }

    pub(crate) fn new_buffer<T: 'static>(&mut self, buffer: Arc<Buffer>) -> BufferHdl<T>{

        let buffer = ResBuffer{
            buffer,
            owning_family: None,
            mask: vk::AccessFlags2::empty(),
            guard: None,
        };

        let key = self.buffers.insert(buffer);

        BufferHdl { hdl: key, ty: PhantomData }
    }


    pub(crate) fn new_image(&mut self, image: Arc<Image>, sampler: Option<SamplerHdl>) -> ImageHdl{

        let image = ResImage{
            image,
            sampler,
            owning_family: None,
            mask: vk::AccessFlags2::empty(),
            layout: vk::ImageLayout::UNDEFINED,
            guard: None,
        };

        let key = self.images.insert(image);

        ImageHdl { hdl: key }
    }

    pub(crate) fn new_sampler(&mut self, sampler: Arc<Sampler>) -> SamplerHdl{
        let sampler = ResSampler{
            sampler,
        };

        let key = self.sampler.insert(sampler);
        SamplerHdl{hdl: key}
    }

    pub(crate) fn get_image(&self, image: ImageKey) -> Option<&ResImage> {
        self.images.get(image)
    }

    pub(crate) fn get_buffer(&self, buffer: BufferKey) -> Option<&ResBuffer> {
        self.buffers.get(buffer)
    }

    pub(crate) fn get_sampler(&self, sampler: SamplerKey) -> Option<&ResSampler> {
        self.sampler.get(sampler)
    }

    pub(crate) fn get_image_mut(&mut self, image: ImageKey) -> Option<&mut ResImage> {
        self.images.get_mut(image)
    }

    pub(crate) fn get_buffer_mut(&mut self, buffer: BufferKey) -> Option<&mut ResBuffer> {
        self.buffers.get_mut(buffer)
    }

    pub(crate) fn get_sampler_mut(&mut self, sampler: SamplerKey) -> Option<&mut ResSampler> {
        self.sampler.get_mut(sampler)
    }

    pub(crate) fn remove_image(&mut self, image: ImageKey){
        let _ = self.images.remove(image);
    }

    pub(crate) fn remove_buffer(&mut self, buffer: BufferKey){
        let _ = self.buffers.remove(buffer);
    }

    pub(crate) fn remove_sampler(&mut self, sampler: SamplerKey){
        let _ = self.sampler.remove(sampler);
    }

    ///Retrieves a short lived image from the cache.
    pub(crate) fn tmp_image(&mut self, desc: ImgDesc, ctx: &Ctx<Allocator>, tracks: &Tracks) -> Result<ImageKey, ResourceError>{
        //check cache for an unused image that has the correct properties, otherwise create one.
        for res in &mut self.temporary_resources{
            if let AnyResource::Image(img_key) = res.res{
                //If image is no guarded by running command buffer, and description matches, use it
                if let Some(img) = self.images.get_mut(img_key){
                    //We test the description first, since getting the semaphore value might take a while.
                    if img.image.desc != desc{
                        continue;
                    }

                    if let Some(guard) = &img.guard{
                        if tracks.guard_finished(guard){
                            #[cfg(feature="logging")]
                            log::info!("Found suitable cached image {:?}!", img_key);

                            //Yay found one, touch it and return.
                            res.last_touch = self.epoch;
                            img.guard = None;
                            return Ok(img_key);
                        }
                    }
                }
            }
        }

        //If we reached this we did not find such an image, therfore create one
        let image = Image::new(&ctx.device, &ctx.allocator, desc, MemoryUsage::GpuOnly, None, None).map_err(|e| ResourceError::Any(e))?;
        let new = self.new_image(Arc::new(image), None);

        #[cfg(feature="logging")]
        log::info!("Created new temporary image {:?}!", new.hdl);

        self.temporary_resources.push(TemporaryResource { res: AnyResource::Image(new.hdl), last_touch: self.epoch });
        Ok(new.hdl)
    }

    ///Notifies that a new frame is started. We can therefore try and recycle temporary resources into e new *epoch* if needed.
    pub(crate) fn notify_new_frame(&mut self){

        //TODO: - iterate over long untouched resources and retire if possible.
        //      - handle wrapping add by resetting all counters
        //      -

        match self.epoch.checked_add(1){
            Some(new) => self.epoch = new,
            None => {
                //reset epoch counter and set all temporary resources to 0.
                self.epoch = 1;
                for res in &mut self.temporary_resources{
                    res.last_touch = 0;
                }
            }
        }
        //remove all that are too old
        // FIXME: Make faster :)
        let mut tmp = Vec::new();
        core::mem::swap(&mut tmp, &mut self.temporary_resources);

        let (removed, retained) = tmp
                                      .into_iter()
            .fold((Vec::new(), Vec::new()), |(mut removed, mut retained), res| {
                if (self.epoch - res.last_touch) > Self::MAX_INACTIVE_EPOCHS{
                    removed.push(res);
                }else {
                    retained.push(res);
                }
                (removed, retained)
            });

        //Update list and remove old ones
        self.temporary_resources = retained;
        for rem in removed{
            #[cfg(feature="logging")]
            log::info!("retiring {:?}", rem.res);

            match rem.res {
                AnyResource::Buffer(buf) => self.remove_buffer(buf),
                AnyResource::Image(img) => self.remove_image(img),
                AnyResource::Sampler(sam) => self.remove_sampler(sam)
            }
        }
    }
}
