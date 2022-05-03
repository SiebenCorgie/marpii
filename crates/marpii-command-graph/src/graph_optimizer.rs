use std::sync::Arc;

use fxhash::FxHashMap;
use marpii::{ash::vk, context::Device, sync::Semaphore};

use crate::{
    graph::PassSubmit,
    pass::AssumedState,
    state::Transitions,
    Graph, Resource,
    graph_builder::{Dependency, Dependee, Segment, SegmentKey},
};

pub struct Submit {
    pub(crate) queue: u32,
    pub(crate) wait_for: Vec<Arc<Semaphore>>,
    pub(crate) order: Vec<Segment>,
    //The submits "own" signal that might be created while building the graph
    pub(crate) signaling: Option<Arc<Semaphore>>,
    //possible external signals that are submitted regardless
    pub(crate) external_signals: Vec<Arc<Semaphore>>,
}

impl Submit {
    pub(crate) fn get_seamphore(&mut self, device: &Arc<Device>) -> Arc<Semaphore> {
        if let Some(sem) = &self.signaling {
            sem.clone()
        } else {
            self.signaling = Some(Semaphore::new(device).unwrap());
            self.signaling.as_ref().unwrap().clone()
        }
    }
}

///Optimizer graph build from a submission list. Each [Submit](crate::Submit) will later represent a command buffer as well as its signal
///and waiting information.
///
///A submission however can be optimized as long as each *segment* dependencies are respected.
pub struct OptGraph {
    pub(crate) device: Arc<Device>,
    pub(crate) submits: Vec<Submit>,

    ///True if the graph is resubmittable
    pub(crate) is_resubmitable: bool,
}

impl OptGraph {
    pub fn new(device: &Arc<Device>) -> Self {
        OptGraph {
            device: device.clone(),
            submits: Vec::new(),
            is_resubmitable: false,
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
                return Some(sub.get_seamphore(&self.device));
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

    ///Adds additional steps to the graph in order to keep `data` valid in between graph submission.
    ///
    /// # Implementation
    ///
    /// While [make_resubmitable][OptGraph::make_resubmitable] makes the graph resubmittable it can not gurantee that
    /// data can be kept between submissions.
    pub fn keep(self, _data: Resource) -> Self {
        unimplemented!();
    }

    ///Makes the whole graph resubmitable. This adds data dependencies in order to make resubmissions of the same command buffer valid.
    ///
    /// # Implementation
    ///
    ///In practise the final layout of each resource is analyzed. An additional "Initialization" Segment is introduced
    /// that transforms each resource to the "final layout" before executing the graph for the first time.
    ///
    /// On first submit the graph will then transform the resources to the "final layout" first, then start "normal" execution.
    /// Since the final and initial layout are the same now the graph becomes resubmittable.
    pub fn make_resubmitable(self) -> Self {
        unimplemented!();
    }

    ///Uses the current submit state to record all command buffers.
    pub fn finish(self) -> Result<Graph, anyhow::Error> {
        let mut graph = Graph {
            device: self.device.clone(),
            command_pools: FxHashMap::default(),
            queue_submits: Vec::with_capacity(self.submits.len()),
            is_resubmitable: self.is_resubmitable,
            was_submitted: false,
        };

        //We now take each submit, allocate a command buffer for it, record the command buffer based on the submits conditions. This mostly means
        //adding all queue/acquires at the start, then recording for each segment:
        //    Barriers
        //    the pass
        //
        //and finaly after recording all segments of a submit, enqueue possible queue-release operations based on the dependees field.
        //
        //The final command buffers are then put into the final graphs "submission order". Which can be executed once ore multiple times.
        for submit in self.submits.into_iter() {
            let mut cb = graph.alloc_new_command_buffer(&self.device, submit.queue)?;
            let mut recorder = cb.start_recording()?;
            for mut segment in submit.order.into_iter() {
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

				    trans.add_into_assumed(init_state, submit.queue);				    
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
                                AssumedState::Image { image, .. } => {
                                    trans.release_image(
					&image,
					from_segment.queue,
					to_segment.queue
				    )
                                }
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

            //build sinal submit and push it
            let graph_submit = PassSubmit {
                queue: self
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
                signaling: submit
                    .signaling
                    .into_iter()
                    .chain(submit.external_signals.into_iter())
                    .collect(), //merge internal and external signals
                wait_for: submit
                    .wait_for
                    .into_iter()
                    .map(|sem| (sem, vk::PipelineStageFlags::ALL_COMMANDS))
                    .collect(), //TODO optimize based on passes context information.
            };

            graph.queue_submits.push(graph_submit);
        }

        Ok(graph)
    }
}
