use std::sync::Arc;

use marpii::{ash::vk, sync::Semaphore};

use crate::{
    graph::{ExecutionFence, PassSubmit},
    graph_builder::{Dependee, Dependency, Segment, SegmentKey},
    pass::AssumedState,
    state::Transitions,
    Graph,
};

///Single command buffer submit. Queueing multiple segments. Synchronises itself via wait_for and signaling semaphores.
///Borrows internal passes for `passes`
pub struct Submit<'passes> {
    pub(crate) queue: u32,
    pub(crate) wait_for: Vec<Arc<Semaphore>>,
    pub(crate) order: Vec<Segment<'passes>>,
    //The submits "own" signal that might be created while building the graph
    pub(crate) signaling: Option<Arc<Semaphore>>,
    //possible external signals that are submitted regardless
    pub(crate) external_signals: Vec<Arc<Semaphore>>,
}

impl<'passes> Submit<'passes> {
    ///Returns its signal semaphore. Might create one if there wasn't already. Assumes that the semaphore is waited uppon at some point.
    pub(crate) fn get_seamphore(&mut self, graph: &mut Graph) -> Arc<Semaphore> {
        if let Some(sem) = &self.signaling {
            sem.clone()
        } else {
            self.signaling = Some(graph.alloc_semaphore());
            self.signaling.as_ref().unwrap().clone()
        }
    }
}

///Optimizer graph build from a submission list. Each [Submit](crate::Submit) will later represent a command buffer as well as its signal
///and waiting information.
///
///A submission however can be optimized as long as each *segment* dependencies are respected.
pub struct OptGraph<'graph, 'passes> {
    pub(crate) graph: &'graph mut Graph,
    pub(crate) submits: Vec<Submit<'passes>>,
}

impl<'graph, 'passes> OptGraph<'graph, 'passes> {
    pub fn new(graph: &'graph mut Graph) -> Self {
        OptGraph {
            graph,
            submits: Vec::new(),
        }
    }

    ///Returns the signal for a given segment or none if the segment is in no submit
    pub(crate) fn signal_for_segment(&mut self, segment: SegmentKey) -> Option<Arc<Semaphore>> {
        for sub in &mut self.submits {
            let mut has_key = false;
            for seg in &sub.order {
                if seg.key_self == segment {
                    has_key = true;
                    break;
                }
            }

            if has_key {
                return Some(sub.get_seamphore(self.graph));
            }
        }

        None
    }

    #[allow(dead_code)]
    fn print(&self) {
        for sub in &self.submits {
            println!(
                "Submit: q={}, num={}, signaling: {:?}, waiting: {:?}",
                sub.queue,
                sub.order.len(),
                sub.signaling,
                sub.wait_for
            );
            for seg in &sub.order {
                print!("  {}: dep[", seg.name);
                for dep in &seg.dependencies {
                    print!(" {:?}", dep);
                }
                println!(" ]");
            }
        }
    }

    ///Tries to execute all barriers as early as possible
    pub fn early_barriers(self) -> Self {
        unimplemented!();
    }

    ///Tries to acquire resources for the queue as early as possible
    pub fn early_acquire(self) -> Self {
        unimplemented!();
    }

    ///Tries to release resources for another queue as early as possible
    pub fn early_release(self) -> Self {
        unimplemented!();
    }

    ///Groups as many barriers together as possible. Might conflict with [OptGraph::early_barriers](OptGraph::early_barriers).
    pub fn group_barriers(self) -> Self {
        unimplemented!();
    }

    ///Uses the current state to record the final command buffer and submits all of it to the GPU.
    pub fn execute(self) -> Result<ExecutionFence, anyhow::Error> {
        //We now take each submit, allocate a command buffer for it, record the command buffer based on the submits conditions. This mostly means
        //adding all queue/acquires at the start, then recording for each segment:
        //    Barriers
        //    the pass
        //
        //and finaly after recording all segments of a submit, enqueue possible queue-release operations based on the dependees field.
        //
        //The final command buffers are then put into the final graphs "submission order". Which can be executed once ore multiple times.
        let mut execution_fence = ExecutionFence {
            fences: Vec::with_capacity(self.submits.len()),
        };

        for submit in self.submits.into_iter() {
            let mut cb = self.graph.alloc_new_command_buffer(submit.queue)?;
            let mut recorder = cb.start_recording()?;
            for segment in submit.order.into_iter() {
                //Check if there is one or more queue transfers. In that case, schedule the queue transfer barrier,
                // and collect the assosiated assumed states for a later transitioning barrier.
                let transitions = segment.dependencies.into_iter().fold(
                    Transitions::empty(),
                    |mut trans, dep| {
                        match dep {
                            Dependency::Barrier(new_state) => {
                                trans.add_into_assumed(new_state, submit.queue);

                                //Update tracked state.
                            }
                            Dependency::Init(init_state) => {
				//Check if the resource already lives on our queue.
				// If not we have to transition it. Depending on the if it was released or not
				// we either acquire (which keeps the data) or have to initialize from UNDEFINED.
				if init_state.current_queue() != submit.queue{
				    #[cfg(feature="log_reasoning")]
				    log::trace!("Initing resource to\n {:#?}\n coming from different queue = {}", init_state, init_state.current_queue());
				    //If we are here, we assume that the image might not be on our queue,
				    //we therefore schedule a queue acquire and transform into the correct
				    // layout
				    match &init_state{
					AssumedState::Buffer { buffer, state } => trans.init_buffer(buffer, submit.queue, state),
					AssumedState::Image { image, state } => trans.init_image(image, submit.queue, state),
				    }
				}else{
				    #[cfg(feature="log_reasoning")]
				    log::trace!("Initing resource {:?} on same queue via transition", init_state);
                                    trans.add_into_assumed(init_state, submit.queue);
				}
                            }
                            Dependency::QueueTransfer {
                                res,
                                from_segement,
                                to_segment,
                            } => {
                                assert!(to_segment.queue == submit.queue);
                                //Otherwise something went wrong in graph building.
                                assert!(to_segment.queue != from_segement.queue);

                                //NOTE: Since this is in the dependency part of the segment this is a queue acquire operation.
                                //Add the acquire operation, as well as the into_assumed transition
                                match &res {
                                    AssumedState::Image { image, .. } => {
                                        trans.acquire_image(
                                            image,
                                            from_segement.queue,
                                            to_segment.queue,
                                        );
                                    }
                                    AssumedState::Buffer { buffer, .. } => {
                                        trans.acquire_buffer(
                                            buffer,
                                            from_segement.queue,
                                            to_segment.queue,
                                        );
                                    }
                                }
                                trans.add_into_assumed(res, to_segment.queue);
                            }
                        }

                        trans
                    },
                );

                transitions.record(&mut recorder);

                //Since all resources are in the current state, record the actual pass.
                segment.pass.record(&mut recorder)?;

                //Now check if a dependee is set. In that case, enqueue a release operation.
                let transitions =
                    segment
                        .dependees
                        .into_iter()
                        .fold(Transitions::empty(), |mut trans, dep| {
                            //if we got a dependee, add a queue release transition
                            let Dependee {
                                to_segment,
                                from_segment,
                                resource,
                            } = dep;
                            match resource {
                                AssumedState::Image { image, .. } => trans.release_image(
                                    &image,
                                    from_segment.queue,
                                    to_segment.queue,
                                ),
                                AssumedState::Buffer { buffer, .. } => trans.release_buffer(
                                    &buffer,
                                    from_segment.queue,
                                    to_segment.queue,
                                ),
                            }

                            trans
                        });

                if !transitions.is_empty() {
                    transitions.record(&mut recorder);
                }
            }

            recorder.finish_recording()?;

            //build sinal submit, execute and push it.
            //NOTE: Currently using the first queue for the given family.
            let mut graph_submit = PassSubmit {
                queue: self
                    .graph
                    .device
                    .get_first_queue_for_family(submit.queue)
                    .map(|q| q.clone())
                    .ok_or_else(|| {
                        anyhow::format_err!(
                            "Could not find queue for queue_family = {}",
                            submit.queue
                        )
                    })?, //FIXME: Currently not scheduling for multiple queues on same queue family.
                command_buffer: cb,
                signaling: submit.signaling, //merge internal and external signals
                external_signaling: submit.external_signals,
                wait_for: submit
                    .wait_for
                    .into_iter()
                    .map(|sem| (sem, vk::PipelineStageFlags::ALL_COMMANDS))
                    .collect(), //TODO optimize based on passes context information.
            };

            //Submit command buffer to gpu
            let fence = graph_submit.submit(&self.graph.device)?;
            execution_fence.fences.push(fence);
            //Now push to signal that it is executing. Allows us later to recycle submit related data.
            self.graph.push_inflight(graph_submit);
        }

        //Finally drop all borrows
        Ok(execution_fence)
    }
}
