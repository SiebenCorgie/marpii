use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct Sem {
    sem: String,
}

impl From<&str> for Sem {
    fn from(s: &str) -> Self {
        Sem { sem: s.to_string() }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct Resource {
    state: String,
}

impl From<&str> for Resource {
    fn from(s: &str) -> Self {
        Resource {
            state: s.to_string(),
        }
    }
}

#[derive(Debug)]
struct Pass {
    name: String,
    family: u32,
    res: Vec<Resource>,
}

#[derive(Debug)]
enum Dependency {
    Init(Resource),
    QueueTransfer {
        res: Resource,
        from_segement: SegmentKey,
        to_segment: SegmentKey,
    },
    Barrier(Resource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SegmentKey {
    queue: u32,
    index: usize,
}

#[derive(Debug)]
struct Segment {
    key_self: SegmentKey,
    pass: Pass,
    ///Dependencies that need to be met before sheduling.
    dependencies: Vec<Dependency>,
    ///Segments that depend on the outcome of this segment.
    dependees: Vec<SegmentKey>,

    signal: Option<Sem>,
}

impl Segment {
    ///Returns all segmens that need to be signaled before this segment can be started.
    fn get_segment_dependencies(&self) -> Vec<SegmentKey> {
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

#[derive(Debug)]
struct Stream {
    //Currently owned resources
    owning: HashSet<Resource>,
    //operation stream
    ops: Vec<Segment>,
}

#[derive(Debug)]
struct GraphBuilder {
    streams: HashMap<u32, Stream>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        GraphBuilder {
            streams: HashMap::new(),
        }
    }

    fn is_empty(&self) -> bool {
        if !self.streams.is_empty() {
            for (_, s) in &self.streams {
                if s.ops.len() > 0 {
                    return false;
                }
            }
        }

        true
    }

    ///Calculates the dependency for `resource` if it was to be used on `target`.
    fn dependency_for(&mut self, resource: Resource, target: SegmentKey) -> Dependency {
        let owning_family = if let Some((owning_family, _owning_stream)) = self
            .streams
            .iter()
            .find(|(_family, stream)| stream.owning.contains(&resource))
        {
            if *owning_family == target.queue {
                //is a simple barrier since the family is already owning
                return Dependency::Barrier(resource);
            } else {
                *owning_family
            }
        } else {
            //If it was not found, it has not been registered yet, therefore do a init operation
            self.streams
                .get_mut(&target.queue)
                .unwrap()
                .owning
                .insert(resource.clone());
            return Dependency::Init(resource);
        };

        //if we are here, this is an queue transfer

        let dependency_segment = SegmentKey {
            index: self.streams.get_mut(&owning_family).unwrap().ops.len() - 1,
            queue: owning_family,
        };

        //remove from owning, and add dependee information
        assert!(self
            .streams
            .get_mut(&owning_family)
            .unwrap()
            .owning
            .remove(&resource));
        self.streams.get_mut(&owning_family).unwrap().ops[dependency_segment.index]
            .dependees
            .push(target);

        //add to new owner
        assert!(self
            .streams
            .get_mut(&target.queue)
            .unwrap()
            .owning
            .insert(resource.clone()));

        Dependency::QueueTransfer {
            res: resource,
            from_segement: dependency_segment,
            to_segment: target,
        }
    }

    pub fn insert_pass(mut self, pass: Pass) -> Self {
        let trg_fam = pass.family;
        //Make sure the stream exists
        if !self.streams.contains_key(&trg_fam) {
            self.streams.insert(
                pass.family,
                Stream {
                    ops: Vec::new(),
                    owning: HashSet::new(),
                },
            );
        }

        let segment_key = SegmentKey {
            index: self.streams.get_mut(&trg_fam).unwrap().ops.len(),
            queue: pass.family,
        };

        let new_segment = Segment {
            key_self: segment_key,
            dependees: Vec::new(),
            dependencies: pass
                .res
                .iter()
                .map(|res| self.dependency_for(res.clone(), segment_key))
                .collect(),
            pass,
            signal: None,
        };

        self.streams
            .get_mut(&trg_fam)
            .unwrap()
            .ops
            .push(new_segment);

        self
    }

    ///Takes the current builder state and creates the queue submit list with correctly setup inter-queue semaphores.
    pub fn build(mut self) -> Graph {
        let mut graph = Graph {
            submits: Vec::new(),
        };

        //Collects already submitted segements
        let mut submitted_segments: HashSet<SegmentKey> = HashSet::new();

        //Scheduling works by searching each queue for a submitable segment.
        //A segment becomes submittable if every queue-transfer it depends on has been submitted before (to not create deadlocks).
        //
        // If a segment is found we record all segments on that queue until we find a segment that is beeing depndet on, but where the dependy has not been submitted yet.

        'search_loop: while !self.is_empty() {
            //Collects all segments that are submitted in this search-loop iteration.
            //We don't do it before, since otherwise weakly parallized submissions will wait on potentually parallizable work.
            //TODO: find nice example...
            let mut new_segments = Vec::new();

            //On each queue, build a submit list of as many segments as possible until we find a segment that depends on something that has not been submitted yet.
            for (queue, stream) in self.streams.iter_mut() {
                //check for the streams first if it is submittable

                let mut submission = Submit {
                    order: Vec::new(),
                    queue: *queue,
                    signaling: Sem {
                        sem: format!("Sem[{}, {}]", queue, submitted_segments.len()),
                    },
                    wait_for: Vec::new(),
                };

                //Push segments into the submission until we find a segments that depends on a segment that has not been submitted yet.
                'segment_pusher: for _i in 0..stream.ops.len() {
                    //Note since we always pushed the first out of the vec into the submission we are using the first here. The
                    //for loop however makes sure we are not using anything out of bounds.

                    //A segment is submittable if all dependencies have been submitted already.
                    let mut is_submitable = true;
                    'dep_test: for dep in &stream.ops[0].dependencies {
                        match dep {
                            Dependency::QueueTransfer { from_segement, .. } => {
                                if !submitted_segments.contains(from_segement) {
                                    is_submitable = false;
                                    println!(
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
                        let segment = stream.ops.remove(0);
                        println!("Can Submit {:?}", segment.key_self);

                        //push segment into tracker, then enque segment into current submission
                        new_segments.push(segment.key_self);

                        //resolve inter-queue segment dependencies to actual
                        //semaphores
                        for seg_dep in segment.get_segment_dependencies() {
                            let sem = graph.signal_for_segment(seg_dep).unwrap();
                            submission.wait_for.push(sem);
                        }

                        let has_dependees = segment.dependees.len() > 0;

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
                    graph.submits.push(submission);
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

        graph
    }
}

#[derive(Debug)]
struct Submit {
    queue: u32,
    wait_for: Vec<Sem>,
    order: Vec<Segment>,
    signaling: Sem,
}

#[derive(Debug)]
struct Graph {
    submits: Vec<Submit>,
}

impl Graph {
    ///Returns the signal for a given segmentm or none if the segment is in no submit
    fn signal_for_segment(&self, segment: SegmentKey) -> Option<Sem> {
        for sub in &self.submits {
            let mut has_key = false;
            for seg in &sub.order {
                if seg.key_self == segment {
                    has_key = true;
                    break;
                }
            }

            if has_key {
                return Some(sub.signaling.clone());
            }
        }

        None
    }

    fn print(&self) {
        for (idx, submit) in self.submits.iter().enumerate() {
            println!(
                "Submit[{} on {}], waiting for [{:?}]:",
                idx, submit.queue, submit.wait_for
            );
            for op in &submit.order {
                println!("   {}", op.pass.name)
            }
            println!("   signaling {:?}", submit.signaling);
        }
    }
}

//TODO: Merge sync and release/acquire into one, later derive merged sync points

fn main() {
    let graph_builder = GraphBuilder::new()
        .insert_pass(Pass {
            name: String::from("Shadow"),
            family: 1,
            res: vec!["Shadow".into()],
        })
        .insert_pass(Pass {
            name: String::from("Physics"),
            family: 1,
            res: vec!["Physics1".into(), "Physics2".into()],
        })
        .insert_pass(Pass {
            name: String::from("Gbuffer"),
            family: 0,
            res: vec!["Albedo".into(), "Nrm".into(), "Depth".into()],
        })
        .insert_pass(Pass {
            name: String::from("Light"),
            family: 0,
            res: vec![
                "Albedo".into(),
                "Nrm".into(),
                "Depth".into(),
                "Shadow".into(),
                "LBuffer".into(),
            ],
        })
        .insert_pass(Pass {
            name: String::from("GiUpdate"),
            family: 1,
            res: vec!["LBuffer".into()],
        })
        .insert_pass(Pass {
            name: String::from("PostProgress"),
            family: 0,
            res: vec!["LBuffer".into(), "PostPass".into()],
        })
        .insert_pass(Pass {
            name: String::from("Swapchain"),
            family: 0,
            res: vec!["PostPass".into(), "SwImagePresent".into()],
        });

    println!("Builder: {:#?}", graph_builder);

    println!("\n\n\n");
    let graph = graph_builder.build();

    graph.print();
}
