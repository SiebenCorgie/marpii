use std::{
    any::Any,
    sync::{Arc, RwLock},
};

use marpii::{ash::{self, vk}, resources::CommandPool};
use marpii::{
    context::{Device, Queue},
    resources::{CommandBuffer, CommandBufferAllocator},
    sync::{Fence, Semaphore},
};

///Signaled that can be associated with a resource.
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
///
/// Note that this opinionated on the command buffer pool type.
pub struct ManagedCommands {
    ///Assosiated command buffer
    pub inner: CommandBuffer<Arc<CommandPool>>,
    ///All resources needed for the current `inner` command buffer to be valid.
    pub resources: Vec<Caputured>,

    ///Inner semaphore that is used for the execution state of this buffer.
    exec_semaphore: Arc<Semaphore>,
    ///The `exec_semaphore` value that is reached after the current version has finished its execution.
    next_finish: u64,
}

impl ManagedCommands {
    ///Creates a new ManagedCommands instance from a command buffer. Assumes that the commandbuffer is resetable. Otherwise
    /// [Recorder] creation fails.
    pub fn new(
        device: &Arc<Device>,
        command_buffer: CommandBuffer<Arc<CommandPool>>,
    ) -> Result<Self, anyhow::Error> {
        Ok(ManagedCommands {
            inner: command_buffer,
            resources: Vec::new(),
            exec_semaphore: Semaphore::new(device, 0).unwrap(),
            next_finish: 0,
        })
    }

    ///waits for the execution fence to get signaled.
    pub fn wait(&mut self) {
        self.exec_semaphore.wait(self.next_finish, u64::MAX)
    }

    ///Starts recording a new command buffer. Might block until any execution of this command buffer has finished.
    ///
    /// If you want prevent blocking, use `wait`.
    pub fn start_recording<'a>(&'a mut self) -> Result<Recorder<'a>, anyhow::Error> {
        //wait until all execution has finished.
        self.wait();
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
    ///`signal_semaphores` will be signalled when the execution has finished to the given value.
    /// `wait_semaphores` is a list of semaphores that need to be signalled to the given value before starting execution. Each semaphore
    /// must supply the pipeline stage on which is waited.
    pub fn submit(
        &mut self,
        device: &Arc<Device>,
        queue: &Queue,
        signal_semaphores: &[(Arc<Semaphore>, u64)],
        wait_semaphores: &[(Arc<Semaphore>, ash::vk::PipelineStageFlags, u64)],
    ) -> Result<(), anyhow::Error> {
        //first of all, make a copy from each semaphore and include them in our captured variables
        for sem in signal_semaphores
            .iter().map(|(sem, src_val)| sem)
            .chain(wait_semaphores.iter().map(|(s, _stage, _target)| s))
        {
            self.resources
                .push(Caputured::Unsignaled(Box::new(sem.clone())));
        }

        let local_signal_semaphores = signal_semaphores
            .into_iter()
            .map(|s| s.inner)
            .collect::<Vec<_>>();

        let (local_wait_semaphores, local_wait_stages) = wait_semaphores.into_iter().fold(
            (Vec::new(), Vec::new()),
            |(mut vec_sem, mut vec_stage), (sem, stage)| {
                vec_sem.push(sem.inner);
                vec_stage.push(*stage);
                (vec_sem, vec_stage)
            },
        );

        compile_error!("unimplmented");
        //submit to queue
        if let Err(e) = unsafe {
            let queue_lock = queue.inner();
            device.inner.queue_submit(
                *queue_lock,
                &[*ash::vk::SubmitInfo::builder()
                    .command_buffers(&[self.inner.inner])
                    .wait_semaphores(&local_wait_semaphores)
                    .wait_dst_stage_mask(&local_wait_stages)
                    .signal_semaphores(&local_signal_semaphores)],
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

impl Drop for ManagedCommands {
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

pub struct Recorder<'a> {
    //hosting command buffer,
    buffer: &'a mut ManagedCommands,
    has_finished_recording: bool,
}

impl<'a> Recorder<'a> {
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
    /// as to be transformed to
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
    ///
    /// In essence you dont want to reference anyhting outside the closure, like `self` or any reference that does not has
    /// a `'static` lifetime (which most references won't have).
    ///
    ///
    /// In practice this can usually be done by either moving data into the closure (if they are used once), or, if you
    /// need to keep the reference, wrapping it into `Arc<T>` / `Arc<Mutex<T>>`.
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

///Custom implementation of drop. Prevents leaving the command buffer in a recording state.
///This is however most likely creating UB, therefore a log error is issued
impl<'a> Drop for Recorder<'a> {
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
