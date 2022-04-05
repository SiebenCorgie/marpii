//! # MarpII-Commands
//!
//! Implements a highlevel command buffer representation. The main part is the extension of [CommandBuffer][marpii::resources::CommandBuffer] with a [Recorder](Recorder)
//!
//! The recorder records commands on this command buffer and caputures all needed resources. After submitting the recorder to a queue all caputured resources are assosiated with a
//! fence that gets signaled when the command buffer has finished its execution. This way the resources have to stay valid for the duration of the command buffer's execution.
//!
//TODO: Do we want to expose "needed" state already, or only on the command-graph crate?

use std::{
    any::Any,
    sync::{Arc, RwLock},
};

use marpii::ash;
use marpii::{
    context::{Device, Queue},
    resources::{CommandBuffer, CommandBufferAllocator},
    sync::{Fence, Semaphore},
};

///Signaled that can be assosiated with a resource.
pub enum SignalState {
    Unused,
    InUse,
}

impl SignalState {
    pub fn is_in_use(&self) -> bool {
        match self {
            SignalState::InUse => true,
            _ => false,
        }
    }
}

///A signal that can be querried for a resources state.
#[derive(Clone)]
pub struct Signal {
    pub state: Arc<RwLock<SignalState>>,
}

impl Signal {
    pub fn is_in_use(&self) -> bool {
        self.state.read().unwrap().is_in_use()
    }
}

///Wrapper around the [CommandBuffer](marpii::resources::CommandBuffer)
/// that tracks used resources lifetimes.
pub struct ManagedCommands<P: CommandBufferAllocator> {
    ///Assosiated command buffer
    pub inner: CommandBuffer<P>,
    ///All resources needed for the current `inner` command buffer to be valid.
    pub resources: Vec<Caputured>,
    ///Assioated fence that represents the `in use` state on the gpu.
    pub fence: Arc<Fence<()>>,
}

impl<P: CommandBufferAllocator> ManagedCommands<P> {
    ///Creates a new ManagedCommands instance from a command buffer. Assumes that the commandbuffer is resetable. Otherwise
    /// [Recorder] creation fails.
    pub fn new(
        device: &Arc<Device>,
        command_buffer: CommandBuffer<P>,
    ) -> Result<Self, anyhow::Error> {
        Ok(ManagedCommands {
            inner: command_buffer,
            resources: Vec::new(),
            fence: Fence::new(device.clone(), true, None)?,
        })
    }

    ///waits for the execution fence to get signaled.
    pub fn wait(&mut self) -> Result<(), ash::vk::Result> {
        self.fence.wait(u64::MAX)
    }

    ///Starts recording a new command buffer. Might block until any execution of this command buffer has finished.
    ///
    /// If you want prevent blocking, use `wait`.
    pub fn start_recording<'a>(&'a mut self) -> Result<Recorder<'a, P>, anyhow::Error> {
        //wait until all execution has finished.
        self.fence.wait(u64::MAX)?;
        //now drop all bound resources
        self.resources.clear();

        //reset the cb then issue a "start recording"
        self.inner.reset(true)?;

        //Issue record begin.
        unsafe {
            self.inner.pool.device().begin_command_buffer(
                self.inner.inner,
                &ash::vk::CommandBufferBeginInfo::builder()
                    .flags(ash::vk::CommandBufferUsageFlags::empty()), //TODO: optimize?
            )?
        };

        Ok(Recorder {
            buffer: self,
            has_finished_recording: false,
        })
    }

    ///Submits commands to a queue.
    ///
    ///`signal_semaphores` will be signaled when the execution has finished.
    pub fn submit(
        &mut self,
        device: &Arc<Device>,
        queue: &Queue,
        signal_semaphores: &[Arc<Semaphore>],
    ) -> Result<(), anyhow::Error> {
        //first of all, make a copy from each semaphore and include them in our captured variables
        for sem in signal_semaphores.iter() {
            self.resources
                .push(Caputured::Unsignaled(Box::new(sem.clone())));
        }

        let local_semaphores = signal_semaphores
            .into_iter()
            .map(|s| s.inner)
            .collect::<Vec<_>>();

        //reset fence for resubmission
        self.fence.reset()?;

        //submit to queue
        if let Err(e) = unsafe {
            device.inner.queue_submit(
                queue.inner,
                &[*ash::vk::SubmitInfo::builder()
                    .command_buffers(&[self.inner.inner])
                    .signal_semaphores(&local_semaphores)],
                self.fence.inner,
            )
        } {
            #[cfg(feature = "logging")]
            log::error!(
                "Failed to submit command buffer to queue {}: {}",
                queue.family_index,
                e
            );
            anyhow::bail!("Failed to execute command buffer on queue: {}", e)
        }

        Ok(())
    }
}

impl<P: CommandBufferAllocator> Drop for ManagedCommands<P> {
    fn drop(&mut self) {
        //if not signaled, wait for the fence to end
        if let Ok(false) = self.fence.get_status() {
            #[cfg(feature = "logging")]
            log::trace!("Waiting for fence");

            if let Err(e) = self.fence.wait(u64::MAX) {
                #[cfg(feature = "logging")]
                log::error!("Failed waiting for fence on ManagedBuffer drop: {}", e);
            }
        }
    }
}

///Types of caputured resources.
pub enum Caputured {
    Signaled {
        resource: Box<dyn Any + Send + 'static>,
        signal: Signal,
    },
    Unsignaled(Box<dyn Any + Send + 'static>),
}

pub struct Recorder<'a, P: CommandBufferAllocator> {
    //hosting command buffer,
    buffer: &'a mut ManagedCommands<P>,
    has_finished_recording: bool,
}

impl<'a, P: CommandBufferAllocator> Recorder<'a, P> {
    ///Records a command `cmd`. All resources used on `cmd` have to have a static lifetime, since they will be tracked by
    /// this recorder, and after finishing recording by the parents [ManagedCommands].
    ///
    /// In practice this means a call like this:
    ///```ignore
    ///recorder.record(|device, cmd| device.cmd_push_constants(
    ///    *cmd,
    ///    self.pipeline.layout,
    ///    ash::vk::ShaderStageFlags::COMPUTE,
    ///    0,
    ///	   self.push_constant.content_as_bytes(),
    ///));
    ///```
    ///```ignore
    ///recorder.record({
    ///    let pipe = self.pipeline.clone(); //assuming this is in a arc, if used just in this cb, could be moved as well
    ///    let push_const = self.push_constant.clone(); //same as with the pipeline
    ///    |device, cmd| device.cmd_push_constants(
    ///        *cmd,
    ///        pipe.layout,
    ///        ash::vk::ShaderStageFlags::COMPUTE,
    ///        0,
    ///	       push_const.content_as_bytes(),
    ///    )
    ///});
    ///```
    /// as to be transformed to
    pub fn record<F: Send + 'static>(&mut self, cmd: F)
    where
        F: Fn(&ash::Device, &ash::vk::CommandBuffer),
    {
        //wrap command in a box to catch the resources
        let cmd = Box::new(cmd);

        //record command
        cmd(&self.buffer.inner.pool.device(), &self.buffer.inner.inner);
        //push resources into caputure
        self.buffer
            .resources
            .push(Caputured::Unsignaled(Box::new(cmd)));
    }

    /*
        ///Records a command, returns a signal for `resources` that can be querried at runtime if a resource is in use or not.
        pub fn record_signaled<R: Send + 'static>(&mut self, cmd: impl FnOnce(&ash::Device, &ash::vk::CommandBuffer)) -> Signal{
        let signal = Signal{state: Arc::new(RwLock::new(SignalState::Unused))};

        //record command
        cmd(&resources, &self.buffer.inner.pool.device(), &self.buffer.inner.inner);
        //push resources into caputure
        self.buffer.resources.push(Caputured::Signaled{
            resource: Box::new(resources),
            signal: signal.clone()
        });

        signal
        }
    */
    ///Finishes recording of this buffer.
    pub fn finish_recording(mut self) -> Result<(), anyhow::Error> {
        self.has_finished_recording = true;
        unsafe {
            self.buffer
                .inner
                .pool
                .device()
                .end_command_buffer(self.buffer.inner.inner)?
        };

        Ok(())
    }
}

///Custom implementation of drop. Prevents leafing the command buffer in a recording state.
///This is however most likely creating UB, therefore a log error is issued
impl<'a, P: CommandBufferAllocator> Drop for Recorder<'a, P> {
    fn drop(&mut self) {
        if !self.has_finished_recording {
            #[cfg(feature = "logging")]
            log::error!(
                "Finish recording on drop. This is most likely UB on command buffer recording!"
            );
            if let Err(e) = unsafe {
                self.buffer
                    .inner
                    .pool
                    .device()
                    .end_command_buffer(self.buffer.inner.inner)
            } {
                #[cfg(feature = "logging")]
                log::error!("Failed to end recording of command buffer in Recorder's drop implementation: {}", e);
            }
        }
    }
}
