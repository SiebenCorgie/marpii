use marpii::{resources::{BufDesc, SharingMode}, ash::vk};
use marpii_rmg::{Rmg, BufferKey, RmgError, Task};
use shared::SimObj;

use crate::OBJECT_COUNT;

pub struct Simulation{
    ///Simulation buffer where one is *src* and the other is *dst*
    /// with alternating keys.
    sim_buffer: [BufferKey; 2],
    ///Points to the current *src* buffer. Switches after each execution.
    current: usize,

    is_init: bool,
}

impl Simulation{
    fn new(rmg: &mut Rmg) -> Result<Self, RmgError>{
        Ok(Simulation {
            sim_buffer: [
                rmg.new_buffer::<SimObj>(OBJECT_COUNT, Some("SimBuffer 1"))?,
                rmg.new_buffer::<SimObj>(OBJECT_COUNT, Some("SimBuffer 2"))?,
            ],
            current: 0,
            is_init: false
        })
    }

    fn src_buffer(&self) -> BufferKey{
        self.sim_buffer[self.current % 2]
    }

    fn dst_buffer(&self) -> BufferKey{
        self.sim_buffer[(self.current + 1) % 2]
    }

    fn switch(&mut self){
        self.current = (self.current + 1) % 2;
    }
}


impl Task for Simulation {
    fn name(&self) -> &'static str {
        "Simulation"
    }

    fn post_execution(&mut self, _resources: &mut marpii_rmg::Resources) -> Result<(), marpii_rmg::RecordError> {
        self.switch();
        Ok(())
    }

    fn queue_flags(&self) -> vk::QueueFlags {
        vk::QueueFlags::COMPUTE
    }

    fn register(&self, registry: &mut marpii_rmg::ResourceRegistry) {
        registry.request_buffer(self.dst_buffer());
        registry.request_buffer(self.src_buffer());
    }

    fn record(
        &mut self,
        device: &std::sync::Arc<marpii::context::Device>,
        command_buffer: &vk::CommandBuffer,
        resources: &marpii_rmg::Resources,
    ) {
        println!("Record!")
    }
}
