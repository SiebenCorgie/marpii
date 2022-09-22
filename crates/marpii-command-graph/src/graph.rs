use std::{collections::VecDeque, sync::Arc};

use fxhash::FxHashMap;
use marpii::{
    ash::vk,
    context::{Device, Queue},
    resources::{CommandBufferAllocator, CommandPool},
    sync::{Semaphore, AnonymGuard},
};
use marpii_commands::ManagedCommands;

use crate::graph_builder::GraphBuilder;

///Fence wrapper holding all fences that are related to a particular execution.
pub struct ExecutionGuard {
    pub(crate) guards: Vec<Arc<dyn AnonymGuard + Send + Sync>>,
}

impl ExecutionGuard {
    ///Returns true if all guards have finished
    pub fn has_finished(&self) -> bool {
        for g in self.guards.iter() {
            if !g.has_finished(){
                return false;
            }
        }
        true
    }

    ///Waits for all fences. Panics if any fence fails to signal.
    pub fn wait(self) {
        for f in self.guards {
            f.wait(u64::MAX).unwrap()
        }
    }
}

///Single command buffer submission in this graph.
pub(crate) struct PassSubmit {
    pub(crate) queue: Queue,
    pub(crate) wait_for: Vec<(Arc<Semaphore>, vk::PipelineStageFlags)>,
    pub(crate) command_buffer: ManagedCommands,
    pub(crate) signaling: Option<Arc<Semaphore>>,
    pub(crate) external_signaling: Vec<Arc<Semaphore>>,
}

impl PassSubmit {
    ///Submits. Might fail, for instance if this is a non-resubmittable graph, but it has been
    /// submitted already.
    pub fn submit(
        &mut self,
        device: &Arc<Device>,
    ) -> Result<Arc<dyn AnonymGuard + Send + Sync + 'static>, anyhow::Error> {
        #[cfg(feature = "log_reasoning")]
        log::trace!(
            "Submitting with wait: {:?}, signaling: {:?}",
            self.wait_for,
            self.signaling
        );

        //FIXME: without allocation
        let mut signaling_semaphores = Vec::new();
        if self.external_signaling.len() > 0 {
            signaling_semaphores.append(&mut self.external_signaling.clone());
        }

        if let Some(sem) = &self.signaling {
            signaling_semaphores.push(sem.clone());
        }

        self.command_buffer
            .submit(device, &self.queue, &signaling_semaphores, &self.wait_for)?;
        Ok(self.command_buffer.fence.clone())
    }

    pub fn is_finished(&self) -> bool {
        match self.command_buffer.fence.get_status() {
            Ok(state) => state,
            Err(e) => {
                #[cfg(feature = "logging")]
                log::error!("Failed to get fence state: {},\n considering unfinished. Not this might lead to accumulating memory if a lot of passes fail to end.", e);
                false
            }
        }
    }
}

pub struct Graph {
    ///recycled semaphores that have been submitted.
    recycled_semaphores: VecDeque<Arc<Semaphore>>,

    ///Device the graph is based on
    pub(crate) device: Arc<Device>,
    ///Command pool on a per-queue family basis
    pub(crate) command_pools: FxHashMap<u32, Arc<CommandPool>>,
    ///current inflight submits.
    pub(crate) inlfight_submits: VecDeque<PassSubmit>,
}

impl Graph {
    pub fn new(device: &Arc<Device>) -> Self {
        Graph {
            recycled_semaphores: VecDeque::new(),
            device: device.clone(),
            command_pools: FxHashMap::default(),
            inlfight_submits: VecDeque::new(),
        }
    }

    ///Starts recording a new graph.
    pub fn record<'record, 'passes>(&'record mut self) -> GraphBuilder<'record, 'passes> {
        //We assume that new inflights are always pushed to the front. We therefore can assume that the ones
        //at the back should be finished earlier.
        //
        //For recycling we pop from the back until we found an unfinishe cb.
        //
        //There is an edgecase where a lot of "long" submits are followed by an old one. In that case
        //this strategy is not optimal.
        self.inlfight_submits.retain(|ele| {
            if ele.is_finished() {
                //since is finished, can enqueue semaphore for reuse
                if let Some(sem) = &ele.signaling {
                    #[cfg(feature = "logging")]
                    log::info!("Recycling semaphore of finished submit {:?}", sem.inner);
                    self.recycled_semaphores.push_front(sem.clone());
                }
                //TODO maybe recycle other stuff later.

                false // can be kicked out
            } else {
                true // need to be kept since not finished
            }
        });

        GraphBuilder::new(self)
    }

    ///Returns a unsignaled semaphore
    pub fn alloc_semaphore(&mut self) -> Arc<Semaphore> {
        if let Some(sem) = self.recycled_semaphores.pop_back() {
            sem
        } else {
            Semaphore::new(&self.device, 0).unwrap()
        }
    }

    pub fn alloc_new_command_buffer(
        &mut self,
        queue_family: u32,
    ) -> Result<ManagedCommands, anyhow::Error> {
        let pool = if let Some(pool) = self.command_pools.get_mut(&queue_family) {
            pool.clone()
        } else {
            #[cfg(feature = "logging")]
            log::info!(
                "No command pool for queue_family {} yet, creating one!",
                queue_family
            );
            let command_pool = Arc::new(CommandPool::new(
                &self.device,
                queue_family,
                vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
            )?);
            self.command_pools
                .insert(queue_family, command_pool.clone());
            command_pool
        };

        let buffer = pool.allocate_buffer(vk::CommandBufferLevel::PRIMARY)?;
        ManagedCommands::new(&self.device, buffer)
    }

    pub(crate) fn push_inflight(&mut self, sub: PassSubmit) {
        self.inlfight_submits.push_front(sub);
    }

    /*
        ///Submits the graph. Might fail, for instance if this is a non-resubmittable graph, but it has been
        /// submitted already.
        pub fn submit(&mut self) -> Result<(), anyhow::Error> {
            if self.was_submitted && !self.is_resubmitable {
                anyhow::bail!("Cannot resubmit graph");
            }

            for submit in &mut self.queue_submits {
                #[cfg(feature = "log_reasoning")]
                log::trace!(
                    "Submitting with wait: {:?}, signaling: {:?}",
                    submit.wait_for,
                    submit.signaling
                );

                submit.command_buffer.submit(
                    &self.device,
                    &submit.queue,
                    &submit.signaling,
                    &submit.wait_for,
                )?;
            }
            Ok(())
    }
        */
}
