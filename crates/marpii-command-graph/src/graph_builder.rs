use std::{fmt::Debug, sync::Arc};

use fxhash::{FxHashMap, FxHashSet};
use marpii::context::Device;

use crate::{
    pass::{AssumedState, Pass},
    Graph, StBuffer, StImage,
    graph_optimizer::{Submit, OptGraph},
    
};

///Key to a segments in the [GraphBuilder]'s streams. Queue is the `streams` HashMap's queue family key, `index`
///is the index into the `segments` list of [Stream].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SegmentKey {
    pub(crate) index: usize,
    pub(crate) queue: u32,
}

///Some Resource in the graph.
#[derive(Clone, Hash)]
pub enum Resource {
    Image(StImage),
    Buffer(StBuffer),
}

impl PartialEq for Resource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Resource::Image(s), Resource::Image(o)) => s == o,
            (Resource::Buffer(s), Resource::Buffer(o)) => s == o,
            _ => false,
        }
    }
}

impl Eq for Resource {}

impl From<AssumedState> for Resource {
    fn from(f: AssumedState) -> Self {
        match f {
            AssumedState::Buffer { buffer, .. } => Resource::Buffer(buffer),
            AssumedState::Image { image, .. } => Resource::Image(image),
        }
    }
}

pub(crate) enum Dependency {
    ///Used if the resource has not been seen within the graph yet. In that case it is inialized from "UNDEFINED".
    Init(AssumedState),
    ///Queue Transfer dependency. Happens if a resource was used on another queue before.
    QueueTransfer {
        res: AssumedState,
        from_segement: SegmentKey,
        to_segment: SegmentKey,
    },
    ///Simple barrier dependency. In that case the Resource is known, and already owned by the correct queue. In that case
    /// a simple (possibly) layout/access-mask chaning barrier is enqueued.
    Barrier(AssumedState),
}

impl Debug for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dependency::Init(_state) => write!(f, "Init"),
            Dependency::Barrier(_new_state) => write!(f, "Barrier"),
            Dependency::QueueTransfer {
                from_segement,
                to_segment,
                ..
            } => write!(f, "QueueTransfer[{:?} -> {:?}]", from_segement, to_segment),
        }
    }
}

///Dependee information which collect information needed to know where to a resource needs to be released
pub(crate) struct Dependee {
    pub(crate) to_segment: SegmentKey,
    pub(crate) from_segment: SegmentKey,
    pub(crate) resource: AssumedState,
}

///A single pass on a queue's stream. Collects all dependencies it has it self, as well as
/// who depends on it.
pub(crate) struct Segment {
    ///Self's key in teh GraphBuilder. (just for convenience stored here.)
    pub(crate) key_self: SegmentKey,
    ///The user defined pass that is executed.
    pub(crate) pass: Box<dyn Pass + Send>,
    ///Some readable name for debugging.
    pub(crate) name: String,
    ///Dependencies that need to be met before sheduling.
    pub(crate) dependencies: Vec<Dependency>,
    ///Segments that depend on the outcome of this segment.
    pub(crate) dependees: Vec<Dependee>,
}

impl Segment {
    ///Returns all segmens that need to be signaled before this segment can be started.
    pub(crate) fn get_segment_dependencies(&self) -> Vec<SegmentKey> {
        let mut segs = Vec::new();
        for dep in &self.dependencies {
            if let Dependency::QueueTransfer {
                from_segement,
                to_segment,
                ..
            } = dep
            {
                assert!(&self.key_self == to_segment);
                segs.push(*from_segement);
            }
        }

        segs
    }
}

///Stream of [Segment]s. Also tracks ownership of resources while building the dependency graph.
struct Stream {
    owning: FxHashSet<Resource>,
    segments: Vec<Segment>,
}

///Builder-Type that allows for sequencial pass insertions. Resolves inter-pass and inter-queue dependencies of all
///declared resources.
pub struct GraphBuilder {
    device: Arc<Device>,
    ///sequencial streams of segments on a per-queue-family basis.
    streams: FxHashMap<u32, Stream>,
}

impl Debug for GraphBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GraphBuilder:\n")?;
        for (k, streams) in &self.streams {
            write!(f, "    On Queue = {}:\n", k)?;
            for s in streams.segments.iter() {
                write!(
                    f,
                    "        {}, dependencies: {:?}, dependees: {:?}\n",
                    s.name,
                    s.dependencies,
                    s.dependees
                        .iter()
                        .map(|dep| dep.to_segment)
                        .collect::<Vec<_>>()
                )?;
            }
        }

        Ok(())
    }
}

impl GraphBuilder {
    pub fn new(device: &Arc<Device>) -> Self {
        GraphBuilder {
            device: device.clone(),
            streams: FxHashMap::default(),
        }
    }

    ///Returns true if no segment on no queue has been submitted yet.
    pub fn is_empty(&self) -> bool {
        if !self.streams.is_empty() {
            for (_, s) in &self.streams {
                if s.segments.len() > 0 {
                    return false;
                }
            }
        }

        true
    }

    ///Calculates the dependency for `resource` if it was to be used on `target`.
    fn dependency_for(&mut self, resource: &AssumedState, target: SegmentKey) -> Dependency {
        let res: Resource = resource.clone().into();
        let owning_family = if let Some((owning_family, _owning_stream)) = self
            .streams
            .iter()
            .find(|(_family, stream)| stream.owning.contains(&res))
        {
            if *owning_family == target.queue {
                //is a simple barrier since the family is already owning
                return Dependency::Barrier(resource.clone());
            } else {
                *owning_family
            }
        } else {
            //If it was not found, it has not been registered yet, therefore do a init operation
            self.streams
                .get_mut(&target.queue)
                .unwrap()
                .owning
                .insert(res.clone());
            return Dependency::Init(resource.clone());
        };

        //if we are here, this is an queue transfer
        let dependency_segment = SegmentKey {
            index: self.streams.get_mut(&owning_family).unwrap().segments.len() - 1,
            queue: owning_family,
        };

        //remove from owning, and add dependee information
        assert!(self
            .streams
            .get_mut(&owning_family)
            .unwrap()
            .owning
            .remove(&res));
        let dependee = Dependee {
            from_segment: dependency_segment,
            to_segment: target,
            resource: resource.clone(),
        };
        self.streams.get_mut(&owning_family).unwrap().segments[dependency_segment.index]
            .dependees
            .push(dependee);

        //add to new owner
        assert!(self
            .streams
            .get_mut(&target.queue)
            .unwrap()
            .owning
            .insert(res.clone()));

        Dependency::QueueTransfer {
            res: resource.clone(),
            from_segement: dependency_segment,
            to_segment: target,
        }
    }

    ///Assures that a segment on `queue` exists
    fn assure_stream(&mut self, queue: u32) {
        if !self.streams.contains_key(&queue) {
            self.streams.insert(
                queue,
                Stream {
                    segments: Vec::new(),
                    owning: FxHashSet::default(),
                },
            );
        }
    }

    ///Inserts `pass` with the given `name` as the next step.
    //TODO remove `queue` argument in favor of runtime decission based on set flags
    pub fn insert_pass<P: Pass + Send + 'static>(
        mut self,
        name: impl Into<String>,
        pass: P,
        queue_family: u32,
    ) -> Self {
        //Make sure the stream exists
        self.assure_stream(queue_family);

        let segment_key = SegmentKey {
            index: self.streams.get_mut(&queue_family).unwrap().segments.len(),
            queue: queue_family,
        };

        let new_segment = Segment {
            key_self: segment_key,
            name: name.into(),
            dependees: Vec::new(),
            dependencies: pass
                .assumed_states()
                .iter()
                .map(
                    //This calculates the dependencies we have at the time of insertion
                    |res| self.dependency_for(res, segment_key),
                )
                .collect(),
            pass: Box::new(pass),
        };

        self.streams
            .get_mut(&queue_family)
            .unwrap()
            .segments
            .push(new_segment);

        self
    }

    ///Builds the final graph structure for this builder. Returns a [OptGraph] that can be optimized. Use [build](GraphBuilder::build) to skip the optimization use use the graph directly.
    pub fn finish(mut self) -> OptGraph {
        let mut tmp_graph = OptGraph::new(&self.device);

        //Collects already submitted segements
        let mut submitted_segments: FxHashSet<SegmentKey> = FxHashSet::default();

        //Scheduling works by searching each queue for a submitable segment.
        //A segment becomes submittable if every queue-transfer it depends on has been submitted before (to not create deadlocks).
        //
        // If a segment is found we record all segments on that queue until we find a segment that is beeing depended on, but where the dependy has not been submitted yet.

        while !self.is_empty() {
            //Collects all segments that are submitted in this search-loop iteration.
            //We don't do it before, since otherwise weakly parallized submissions will wait on potentually parallizable work.
            let mut new_segments = Vec::new();

            //On each queue, build a submit list of as many segments as possible until we find a segment that depends on something that has not been submitted yet.
            for (queue, stream) in self.streams.iter_mut() {
                //check for the streams first if it is submittable

                let mut submission = Submit {
                    order: Vec::new(),
                    queue: *queue,
                    signaling: None,
                    wait_for: Vec::new(),
                    external_signals: Vec::new(),
                };

                //Push segments into the submission until we find a segments that depends on a segment that has not been submitted yet.
                'segment_pusher: for _i in 0..stream.segments.len() {
                    //Note since we always pushed the first out of the vec into the submission we are using the first here. The
                    //for loop however makes sure we are not using anything out of bounds.

                    //A segment is submittable if all dependencies have been submitted already.
                    let mut is_submitable = true;
                    'dep_test: for dep in &stream.segments[0].dependencies {
                        match dep {
                            Dependency::QueueTransfer { from_segement, .. } => {
                                if !submitted_segments.contains(from_segement) {
                                    is_submitable = false;
                                    #[cfg(feature = "log_reasoning")]
                                    log::trace!(
                                        "Not submittable, since {:?} is not submitted yet",
                                        from_segement
                                    );
                                    break 'dep_test;
                                }
                            }
                            _ => { /*TODO check barriers as well?*/ }
                        }
                    }

                    if is_submitable {
                        let segment = stream.segments.remove(0);
                        #[cfg(feature = "log_reasoning")]
                        log::trace!("Can Submit {:?}", segment.key_self);

                        //push segment into tracker, then enque segment into current submission
                        new_segments.push(segment.key_self);

                        //resolve inter-queue segment dependencies to actual
                        //semaphores
                        for seg_dep in segment.get_segment_dependencies() {
                            let sem = tmp_graph.signal_for_segment(seg_dep).unwrap();
                            submission.wait_for.push(sem);
                        }

                        let has_dependees = segment.dependees.len() > 0;

                        //Post push extern signals into dependencies. This includes external wait for semaphores as
                        // well as external signals that can be declared by the segment
                        for sig in segment.pass.signals_external() {
                            #[cfg(feature = "log_reasoning")]
                            log::trace!("{} declared external signal: {:?}", segment.name, sig);

                            submission.external_signals.push(sig.clone());
                        }

                        for waitsig in segment.pass.waits_for_external() {
                            #[cfg(feature = "log_reasoning")]
                            log::trace!("{} declared external wait: {:?}", segment.name, waitsig);

                            submission.wait_for.push(waitsig.clone());
                        }

                        //Finally move segment in
                        submission.order.push(segment);

                        //If other async segment depend on us (the list of dependees is not empty) we have to break.
                        if has_dependees {
                            break 'segment_pusher;
                        }
                    } else {
                        //if not submitable, stop searching for segments
                        break 'segment_pusher;
                    }
                }

                if submission.order.len() > 0 {
                    //add all segment keys to the tracker, then push the segment into the graph
                    tmp_graph.submits.push(submission);
                }
            }

            if new_segments.is_empty() && !self.is_empty() {
                panic!("Could not submit any, but was not empty!");
            }

            //enqueue new segments for next search run
            for seg in new_segments {
                assert!(submitted_segments.insert(seg));
            }
        }

        tmp_graph
    }

    ///Builds the graph without optimizing submits for anything. Note that all data written within the graph
    /// might become undefined after the graph is executed. If you want to keep data between frames, use [finish][GraphBuilder::finish] instead and optimitze for [resubmitable][OptGraph::make_resubmitable].
    pub fn build(self) -> Result<Graph, anyhow::Error> {
        self.finish().finish()
    }
}
