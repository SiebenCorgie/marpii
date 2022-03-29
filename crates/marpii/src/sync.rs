//! # Synchronization
//!
//! ## Types of synchronization primitives
//!
//! marpii uses a thin wrapper around vulkans native synchronizations primitives. There are mainly
//! - memory barriers: created by the image object or the buffer object, defines memory state transitions and more
//! - fences: to signal to the host/the CPU when something is done on the GPU
//! - semaphores: to synchronize resource access on the GPU between threads/queues
//! - events: to synchronise commands on a single queue or between host and a queue
//!
//! ## When to use what
//!
//!### Events
//!
//! In Marp you'll always get a event provided by commands which support event signaling. You can decide if you want to use it or if you don't want to use it.
//! In general you should collect the events and provide them to a command if you need to be sure that the work of an command has to be finished before some other work can start.
//! For instance, if you want to access a buffer but need to be sure that a buffer to buffer copy has finished before. Just wait for the event of the copy to signal before starting to access the buffer.
//!
//! ### Semaphores
//!
//! Semaphores are usually set by the programmer to sync between two (or more) command buffers on more then one queue. They can potentially let a queue wait for a long time. However, they allow you to do the following for instance:
//! ```ignore
//! Generate a Gbuffer -> Set Barrier -> Compute lightning on graphics queue while you compute SSAO on compute queue async -> set Barrier -> Assemble final image
//! ```
//! Here the barriers are needed first, to make sure that the Gbuffer has been finished. We can't uses a Event since after that we want to operate on two different queues.
//! Second we don't know when both queues have finished and we can use the result, so naturally we wait again for both to signal the semaphore. Therefore we can be sure that the lightning and the SSAO calculations are finished.
//!
//! ### Fences
//!
//! A fence is usually used to signal something between Host and Device. the most common usage is to wait for the GPU to finish some work. However, you could also let the gpu wait
//! for the host to finish some work before it is allowed to access something. Like a resource, or a new frame.
//! An inportant difference to the raww vulkan fence is, that marps's fence can carry a
//! payload. This can be used for instance if resource have to life at leas as long as work is done on the gpu.
//!
//! ### Memory barriers
//!
//! Those are used to control access to a buffer or image sub-region. Since the layout of an image can changes as well as the access mask of both, images and buffers
//! and queue ownership (specifies which queue can use which resource) we can specify when and how this should happen.
//! To wait for a memory barrier you don't supply if to a command buffer command like the other primitives, but execute them via `command_buffer.pipeline_barrier()` This will do all transitions before further commands are executed. Try to pack as many barriers in one call as possible. But also try to use as many immutable images/buffer as possible.
//! Most images won't have to change layout on a per frame basis, similar buffers as well.
//! **The memory barriers are obtained from the buffer/image which is transformed.**
//!
//! ## Important note on fences
//! Since fences are usually used to sync between Device and Host, they will block till they are in the signaled stayed when dropped.

//use crate::context::SubmitInfo;
use crate::context::Device;
use ash;
use std::sync::Arc;

use std::u64;

pub struct Fence<T> {
    pub inner: ash::vk::Fence,
    pub device: Arc<Device>,
    pub payload: Option<T>,
}

//pub type QueueFence = Arc<Fence<Vec<SubmitInfo>>>;

impl<T> Fence<T> {
    pub fn new(
        device: Arc<Device>,
        is_signaled: bool,
        payload: Option<T>,
    ) -> Result<Arc<Self>, ash::vk::Result> {
        let mut create_info = ash::vk::FenceCreateInfo::builder();
        if is_signaled {
            create_info = create_info.flags(ash::vk::FenceCreateFlags::SIGNALED);
        }

        let fence = unsafe {
            match device.inner.create_fence(&create_info, None) {
                Ok(f) => f,
                Err(er) => return Err(er),
            }
        };

        Ok(Arc::new(Fence {
            inner: fence,
            device,
            payload,
        }))
    }

    ///Resets the fence to its initial state. Might be faster then creating a new one.
    pub fn reset(&self) -> Result<(), ash::vk::Result> {
        unsafe {
            match self.device.inner.reset_fences(&[self.inner]) {
                Ok(_) => Ok(()),
                Err(er) => Err(er),
            }
        }
    }

    ///Returns the current status of this fence. The status is encode in the returned `ash::vk::Result`
    pub fn get_status(&self) -> ash::prelude::VkResult<bool> {
        unsafe { self.device.inner.get_fence_status(self.inner) }
    }

    /// Waits for this single fence. If you want to wait for several fences to
    /// signal finished, use `marp::sync::wait_for_fences()` instead since it
    /// uses the native vulkan wait for fence function with the whole array of fences you supply.
    pub fn wait(&self, timeout: u64) -> Result<(), ash::vk::Result> {
        unsafe {
            match self
                .device
                .inner
                .wait_for_fences(&[self.inner], true, timeout)
            {
                Ok(_) => Ok(()),
                Err(er) => {
                    #[cfg(feature = "logging")]
                    log::error!("Fence wait error: {}", er);
                    Err(er)
                }
            }
        }
    }

    ///Returns the cpu local payload which is embedded.
    pub fn get_payload(&self) -> &Option<T> {
        &self.payload
    }
}

///An abstract way of defining any fence. Can be used if the payload does not have to be acquired at any point and for storing fences in an `Arc<AbstractFence>`
pub trait AbstractFence {
    fn reset(&self) -> Result<(), ash::vk::Result>;
    fn get_status(&self) -> ash::prelude::VkResult<bool>;
    fn wait(&self, timeout: u64) -> Result<(), ash::vk::Result>;
    fn inner(&self) -> &ash::vk::Fence;
}

impl<T> AbstractFence for Fence<T> {
    fn reset(&self) -> Result<(), ash::vk::Result> {
        self.reset()
    }
    fn get_status(&self) -> ash::prelude::VkResult<bool> {
        self.get_status()
    }
    fn wait(&self, timeout: u64) -> Result<(), ash::vk::Result> {
        self.wait(timeout)
    }
    fn inner(&self) -> &ash::vk::Fence {
        &self.inner
    }
}

///Waits for all fences supplied. If you only need one of those activated, use `wait_all: false`. It will then return when one of the fences supplied has finished.
///TODO check if the lifetimes are okay.
pub fn wait_for_fences(
    device: Arc<Device>,
    fences: Vec<Arc<dyn AbstractFence>>,
    wait_all: bool,
    timeout: u64,
) -> Result<(), ash::vk::Result> {
    let inner_fences: Vec<ash::vk::Fence> = fences.into_iter().map(|f| *f.inner()).collect();
    unsafe {
        match device
            .inner
            .wait_for_fences(inner_fences.as_slice(), wait_all, timeout)
        {
            Ok(_) => Ok(()),
            Err(er) => Err(er),
        }
    }
}

impl<T> Drop for Fence<T> {
    fn drop(&mut self) {
        match self.wait(u64::MAX) {
            Ok(_) => {}
            Err(er) => {
                #[cfg(feature = "logging")]
                log::error!("Failed to wait for fence while dropping: {}", er);
            }
        }
        unsafe {
            self.device.inner.destroy_fence(self.inner, None);
        }
    }
}

pub struct Semaphore {
    pub inner: ash::vk::Semaphore,
    pub device: Arc<Device>,
}

impl Semaphore {
    pub fn new(device: &Arc<Device>) -> Result<Arc<Self>, ash::vk::Result> {
        let semaphore = unsafe {
            match device
                .inner
                .create_semaphore(&ash::vk::SemaphoreCreateInfo::builder(), None)
            {
                Ok(s) => s,
                Err(er) => return Err(er),
            }
        };

        Ok(Arc::new(Semaphore {
            inner: semaphore,
            device: device.clone(),
        }))
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_semaphore(self.inner, None) }
    }
}

#[derive(Debug)]
pub enum EventError {
    SetEventStatusError(ash::vk::Result),
    GetEventStatusError(ash::vk::Result),
    ResetEventError(ash::vk::Result),
    StatusReadError(ash::vk::Result),
}

pub struct Event {
    pub device: Arc<Device>,
    pub event: ash::vk::Event,
}

impl Event {
    pub fn new(device: Arc<Device>) -> Result<Arc<Self>, ash::vk::Result> {
        let ci = ash::vk::EventCreateInfo::builder();

        let event = unsafe {
            match device.inner.create_event(&ci, None) {
                Ok(ok) => ok,
                Err(er) => return Err(er),
            }
        };

        Ok(Arc::new(Event { event, device }))
    }
    ///Sets the event into the "waiting" status.
    pub fn set_event(&self) -> Result<(), EventError> {
        unsafe {
            match self.device.inner.set_event(self.event) {
                Ok(_) => Ok(()),
                Err(er) => Err(EventError::SetEventStatusError(er)),
            }
        }
    }

    pub fn reset_event(&self) -> Result<(), EventError> {
        unsafe {
            match self.device.inner.reset_event(self.event) {
                Ok(_) => Ok(()),
                Err(er) => Err(EventError::ResetEventError(er)),
            }
        }
    }

    pub fn status(&self) -> Result<bool, EventError> {
        unsafe {
            match self.device.inner.get_event_status(self.event) {
                Ok(b) => Ok(b),
                Err(er) => Err(EventError::StatusReadError(er)),
            }
        }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_event(self.event, None);
        }
    }
}
