//! # Synchronisation
//!
//! ## Types of synchronisation primitives
//!
//! marpii uses a thin wrapper around vulkans native synchronisations primitives. There are mainly
//! - memory barriers: created by the image object or the buffer object, defines memory state transitions and more
//! - semaphores: to synchronise resource access on the GPU between threads/queues and the CPU. Those are usually called "TimelineSemaphores". We do not expose Fences.
//! - events: to synchronise commands on a single queue or between host and a queue
//!
//! ## Semaphores
//!
//! Semaphores are usually set by the programmer to sync between two (or more) command buffers on more than one queue. They can potentially let a queue wait for a long time. However, they allow you to do the following for instance:
//! ```ignore
//! Generate a Gbuffer -> Set Barrier -> Compute lightning on graphics queue while you compute SSAO on compute queue async -> set Barrier -> Assemble final image
//! ```
//! Here the barriers are needed first, to make sure that the Gbuffer has been finished. We can't uses a Event since we want to operate on two different queues.
//! Second we don't know when both queues have finished and we can use the result, so naturally we wait again for both to signal the semaphore. Therefore we can be sure that the lightning and the SSAO calculations are finished.
//!
//! ### Fences
//!
//! Vulkan 1.0 exposes a Fence primitive that allows synchronisation between the device and the host. This capability however was merged into TimelineSemaphores starting with Vulkan 1.2.
//! Since MarpII requires the newest Vulkan version anyways, the fence primitive is not wrapped at all. As always you are free to create Fences through Ash though.
//!
//! ### Memory barriers
//!
//! Those are used to control access to a buffer or image sub-region. Since the layout of an image can changes as well as the access mask of both, images and buffers
//! and queue ownership (specifies which queue can use which resource) we can specify when and how this should happen.
//! To wait for a memory barrier you don't supply if to a command buffer command like the other primitives, but execute them via `command_buffer.pipeline_barrier()` This will do all transitions before further commands are executed. Try to pack as many barriers in one call as possible. But also try to use as many immutable images/buffer as possible.
//! Most images won't have to change layout on a per frame basis, similar buffers as well.
//! **The memory barriers are obtained from the buffer/image which is transformed.**
//!
//! ## Important note on `GuardSemaphore`s
//! Since `GuardSemaphore` are usually used to sync between Device and Host, they will block till they are in the signaled state when dropped.

//use crate::context::SubmitInfo;
use crate::context::Device;
use ash;
use std::fmt::Debug;
use std::sync::Arc;

use std::u64;


///Single [TimelineSemaphore](https://www.khronos.org/blog/vulkan-timeline-semaphores).
pub struct Semaphore {
    pub inner: ash::vk::Semaphore,
    pub device: Arc<Device>,
}

impl Semaphore {
    pub fn new(device: &Arc<Device>, initial_value: u64) -> Result<Arc<Self>, ash::vk::Result> {

        let mut timeline_ci = ash::vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(ash::vk::SemaphoreType::TIMELINE)
            .initial_value(initial_value);

        let semaphore = unsafe {

            let ci = ash::vk::SemaphoreCreateInfo::builder()
                .push_next(&mut timeline_ci);

            device
                .inner
                .create_semaphore(&ci, None)?
        };

        Ok(Arc::new(Semaphore {
            inner: semaphore,
            device: device.clone(),
        }))
    }

    ///Returns the current value of the semaphore. Note that this can change at any time if the semaphore is in use on
    /// the device.
    ///
    /// # Safety
    ///
    /// Under normal conditions this can't fail if used on a Semaphore created via the [new](Semaphore::new) function. There are however edgecases. If those occur `u64::MAX` is returned instead.
    pub fn get_value(&self) -> u64{

        //Safety: in 99% of all cases the semaphore is created via `new`. In this case the code below is safe.
        //        Otherwise "semaphore must have been created with a VkSemaphoreType of VK_SEMAPHORE_TYPE_TIMELINE"
        //        and "semaphore must have been created, allocated, or retrieved from device" are not guaranteed. In that case the unwrap_or
        //        comes into play.
        unsafe{
            self.device.inner.get_semaphore_counter_value(self.inner).unwrap_or(u64::MAX)
        }
    }

    ///Sets the semaphore value. Note that it [has to be](https://registry.khronos.org/vulkan/specs/1.2-extensions/html/chap7.html#VUID-VkSemaphoreSignalInfo-value-03258) greater then the current value.
    ///
    /// For more information, have a look [here](https://registry.khronos.org/vulkan/specs/1.2-extensions/html/chap7.html#vkSignalSemaphoreKHR).
    ///
    /// # Error
    ///
    /// Returns an error if the value was not greater. The value returned in this case is the current value.
    pub fn set_value(&self, value: u64) -> Result<(), u64>{
        let signal_info = ash::vk::SemaphoreSignalInfo::builder()
            .semaphore(self.inner)
            .value(value);

        if let Err(_) = unsafe{
            self.device.inner.signal_semaphore(&signal_info)
        }{
            Err(self.get_value())
        }else{
            Ok(())
        }
    }

    ///Waits for multiple semaphores of the form `(Semaphore, target_value)`. Additionally a timeout for the whole wait operation can be given.
    ///
    /// # Performance
    ///
    /// Note that this function allocates two vectors, if you are only waiting for a single Semaphore, use the associated [wait](Semaphore::wait).
    pub fn wait_for(waits: &[(&Semaphore, u64)], timeout: u64) -> Result<(), ash::vk::Result>{
        if waits.len() == 0{
            return Ok(());
        }

        let (sems, values): (Vec<ash::vk::Semaphore>, Vec<u64>) = waits.iter().fold(
            (Vec::with_capacity(waits.len()), Vec::with_capacity(waits.len())), //FIXME: Oh no, allocation :/
            |(mut semvec, mut valvec),(sem, val)| {
                semvec.push(sem.inner);
                valvec.push(*val);

                (semvec, valvec)
            });

        let wait = ash::vk::SemaphoreWaitInfo::builder()
            .semaphores(&sems)
            .values(&values);

        unsafe{
            waits[0].0.device.inner.wait_semaphores(&wait, timeout)
        }
    }

    ///Blocks until `self` reaches `value`, or the `timeout` is reached. When having to wait for multiple semaphores, consider using [wait_for](Semaphore::wait_for).
    pub fn wait(&self, value: u64, timeout: u64) -> Result<(), ash::vk::Result>{
        let sem = [self.inner];
        let val = [value];
        let wait = ash::vk::SemaphoreWaitInfo::builder()
            .semaphores(&sem)
            .values(&val);

        unsafe{
            self.device.inner.wait_semaphores(&wait, timeout)
        }
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_semaphore(self.inner, None) }
    }
}

impl Debug for Semaphore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}


///A semaphore that guard a value `T` until a certain target state is reached.
pub struct GuardSemaphore<T>{
    sem: Arc<Semaphore>,
    target: u64,
    #[allow(dead_code)]
    value: T
}

impl<T> GuardSemaphore<T> {
    ///Creates a guard that won't drop `T` until `target` is reached as the semaphores value.
    ///
    /// # Safety
    ///
    /// Note that this can lead to deadlocks if `target` is illdefined.
    pub fn guard(semaphore: Arc<Semaphore>, target: u64, guarded: T) -> GuardSemaphore<T>{
        GuardSemaphore { sem: semaphore, target, value: guarded }
    }

    ///Tries to drop self, returns `Self` as an error if the target value wasn't reached yet.
    pub fn try_drop(self) -> Result<(), Self>{
        if self.sem.get_value() >= self.target{
            Ok(())
        }else {
            Err(self)
        }
    }
}

impl<T> Drop for GuardSemaphore<T> {
    fn drop(&mut self) {
        if self.sem.get_value() < self.target{
            #[cfg(feature="logging")]
            log::warn!("Dropping Guard with unfulfilled target, blocking in drop implementation!");

            //wait
            if let Err(e) = self.sem.wait(self.target, u64::MAX){
                #[cfg(feature="logging")]
                log::error!("Failed to wait for GuardSemaphore: {}", e);
            }
        }
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

impl Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.event.fmt(f)
    }
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
