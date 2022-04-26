use std::sync::Arc;

use fxhash::FxHashMap;
use marpii::{
    ash::vk,
    context::{Device, Queue},
    resources::{CommandBufferAllocator, CommandPool},
    sync::Semaphore,
};
use marpii_commands::ManagedCommands;

use crate::graph_builder::GraphBuilder;

///Single command buffer submission in this graph.
pub(crate) struct PassSubmit {
    pub(crate) queue: Queue,
    pub(crate) wait_for: Vec<(Arc<Semaphore>, vk::PipelineStageFlags)>,
    pub(crate) command_buffer: ManagedCommands,
    pub(crate) signaling: Vec<Arc<Semaphore>>,
}

pub struct Graph {
    pub(crate) device: Arc<Device>,
    pub(crate) command_pools: FxHashMap<u32, Arc<CommandPool>>,
    pub(crate) queue_submits: Vec<PassSubmit>,
    ///True if this graph can be submitted more than once.
    pub(crate) is_resubmitable: bool,
    ///True if this graph has been submitted.
    pub(crate) was_submitted: bool,
}

impl Graph {
    pub fn new(device: &Arc<Device>) -> GraphBuilder {
        GraphBuilder::new(device)
    }

    pub fn alloc_new_command_buffer(
        &mut self,
        device: &Arc<Device>,
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
                device,
                queue_family,
                vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
            )?);
            self.command_pools
                .insert(queue_family, command_pool.clone());
            command_pool
        };

        let buffer = pool.allocate_buffer(vk::CommandBufferLevel::PRIMARY)?;
        ManagedCommands::new(device, buffer)
    }

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
}
