use std::{fmt::Display, ops::Range};

use ahash::AHashMap;

use crate::{resources::res_states::AnyResKey, track::TrackId, RecordError, Rmg};

use super::TaskRecord;

//Participant in an dependency definition
#[derive(Debug, Clone)]
pub(crate) enum DepPart {
    ///When imported for first use in graph.
    Import,
    ///When it is a scheduled task (the index in the node array), and on which track.
    Scheduled { track: TrackId, node_idx: usize },
}

///Dependency half edge, declaring the *other* participant and the resource that is depended on.
#[derive(Debug, Clone)]
pub(crate) struct Dependency {
    pub(crate) participant: DepPart,
    pub(crate) dep: AnyResKey,
}

//Single task node enumerating dependencies and dependees of this task
pub(crate) struct TaskNode<'t> {
    ///All dependencies needed for this task to execute
    pub(crate) dependencies: Vec<Dependency>,
    ///Dependees that depend on this task, or data from this task
    pub(crate) dependees: Vec<Dependency>,
    pub(crate) task: TaskRecord<'t>,
}

impl<'t> Display for TaskNode<'t> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "| ")?;

        for dep in &self.dependencies {
            let s = match dep.participant {
                DepPart::Import => format!("Imp"),
                DepPart::Scheduled { track, node_idx } => {
                    format!("{:x}:{}", track.0.as_raw(), node_idx)
                }
            };
            write!(f, " {} ", s)?;
        }

        write!(f, "  {}  ", self.task.task.name())?;

        for dep in &self.dependees {
            let s = match dep.participant {
                DepPart::Import => "Imp".to_string(),
                DepPart::Scheduled { track, node_idx } => {
                    format!("{:x}:{}", track.0.as_raw(), node_idx)
                }
            };
            write!(f, " {} ", s)?;
        }

        write!(f, " |")
    }
}

//Marks a single track starting at node `start` of `nodes` and
// including `len` nodes.
//
// A Frame is what is basically translated into one command buffer on the executor.
pub(crate) struct TrackFrame {
    pub(crate) start: usize,
    pub(crate) len: usize,
}

impl TrackFrame {
    pub fn contains_idx(&self, idx: usize) -> bool {
        idx >= self.start && idx < (self.start + self.len)
    }

    ///returns idx when increasing len by 1
    fn next_idx(&self) -> usize {
        self.start + self.len
    }

    ///Pushes the node into the currently active frame.
    ///
    /// # On debug builds: panics if the node idx steps more then one node within the frame.
    fn push_node(&mut self, node_idx: usize) {
        debug_assert!(
            self.next_idx() == node_idx,
            "Node idx={} increases by > 1 step. Current next_idx should be {}",
            node_idx,
            self.next_idx()
        );

        assert!(node_idx >= self.start);

        //we now increase to incoperate `node idx` by diff, in practice we probably could use
        // len+1, but that wouldn't be `push` anymore.
        self.len = node_idx - self.start + 1;
    }

    ///Iterates the indices from start to end, where each indice is included
    /// in the frame
    pub fn iter_indices(&self) -> Range<usize> {
        self.start..(self.start + self.len)
    }
}

impl Display for TrackFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "|F: {}..{}|", self.start, self.start + self.len)
    }
}

///Schedule of a single track
pub(crate) struct TrackSchedule<'t> {
    pub(crate) nodes: Vec<TaskNode<'t>>,
    pub(crate) frames: Vec<TrackFrame>,
}

impl<'t> TrackSchedule<'t> {
    fn is_scheduled(&self, idx: usize) -> bool {
        self.find_frame(idx).is_some()
    }

    fn find_frame(&self, task_node_idx: usize) -> Option<&TrackFrame> {
        self.frames
            .iter()
            .find(|frame| frame.contains_idx(task_node_idx))
    }

    fn has_unscheduled(&self) -> bool {
        //check if last is in last range
        match (self.frames.last(), self.nodes.last()) {
            (Some(last_frame), Some(last_node)) => {
                if !last_frame.contains_idx(self.nodes.len() - 1) {
                    //last node is NOT contained in last node
                    true
                } else {
                    false
                }
            }
            (None, Some(_)) => true,
            (Some(_), None) => {
                #[cfg(feature = "logging")]
                log::error!("Invalid TrackSchedule state, has frames, but no nodes");
                false
            }
            (None, None) => false,
        }
    }

    ///returns the next to be scheduled node, if there is one
    fn next_to_schedule(&self) -> Option<usize> {
        let candidate = if let Some(last) = self.frames.last() {
            last.start + last.len
        } else {
            0
        };

        if candidate < self.nodes.len() {
            Some(candidate)
        } else {
            None
        }
    }

    fn start_next_frame(&mut self) {
        let frame = if let Some(last) = self.frames.last() {
            TrackFrame {
                start: last.start + last.len,
                len: 0,
            }
        } else {
            //starting
            TrackFrame { start: 0, len: 0 }
        };

        self.frames.push(frame);
    }
}

///Only finds out when which task is scheduled. Does not do resource management.
pub struct TaskSchedule<'t> {
    pub(crate) tracks: AHashMap<TrackId, TrackSchedule<'t>>,
    ///Tracks on which track some resource is currently owned.
    pub(crate) resource_residency: AHashMap<AnyResKey, (TrackId, usize)>,
}

impl<'t> TaskSchedule<'t> {
    pub fn new_from_tasks(
        rmg: &mut Rmg,
        records: Vec<TaskRecord<'t>>,
    ) -> Result<Self, RecordError> {
        let tracks = rmg
            .tracks
            .0
            .iter()
            .map(|(id, track)| {
                (
                    *id,
                    TrackSchedule {
                        nodes: Vec::with_capacity(10),
                        frames: Vec::new(),
                    },
                )
            })
            .collect();

        let mut schedule = TaskSchedule {
            tracks,
            resource_residency: AHashMap::default(),
        };

        //add all tasks, which will (implicitly) add inter-task dependencies wherever needed.
        for record in records {
            schedule.add_task(rmg, record)?;
        }

        //now figure out *frames*. A frame is a set of tasks on one track, that can be executed without having to signal a semaphore or wait for another dependency
        //
        // In practice we can move all tasks into one thread, that only depend on already scheduled tasks.
        schedule.build_frames()?;

        Ok(schedule)
    }

    ///True if there are tasks that are not part of a frame on any track.
    fn unscheduled_tasks(&self) -> bool {
        for track in self.tracks.values() {
            if track.has_unscheduled() {
                return true;
            }
        }

        false
    }

    fn add_task(&mut self, rmg: &mut Rmg, task: TaskRecord<'t>) -> Result<(), RecordError> {
        //allocate node
        let node_track = rmg
            .tracks
            .track_for_usage(task.task.queue_flags().into())
            .ok_or(RecordError::NoFittingTrack(task.task.queue_flags()))?;
        let node_idx = self
            .tracks
            .get_mut(&node_track)
            .ok_or_else(|| RecordError::NoFittingTrack(task.task.queue_flags()))?
            .nodes
            .len();
        let mut node = TaskNode {
            task,
            dependees: Vec::new(),
            dependencies: Vec::new(),
        };

        //resolve dependencies
        for res in node.task.registry.any_res_iter() {
            let dep = if let Some(residency) = self.resource_residency.get_mut(&res) {
                let to_add = Dependency {
                    participant: DepPart::Scheduled {
                        track: residency.0,
                        node_idx: residency.1,
                    },
                    dep: res,
                };

                //signal as dependee to the task we take it from
                self.tracks.get_mut(&residency.0).unwrap().nodes[residency.1]
                    .dependees
                    .push(Dependency {
                        participant: DepPart::Scheduled {
                            track: node_track,
                            node_idx: node_idx,
                        },
                        dep: res,
                    });

                //and move resource ownership
                *residency = (node_track, node_idx);

                to_add
            } else {
                //Mark as import
                let dep = Dependency {
                    participant: DepPart::Import,
                    dep: res,
                };
                //add to residency tracker, since seen for the first time
                self.resource_residency.insert(res, (node_track, node_idx));
                dep
            };
            node.dependencies.push(dep);
        }
        self.tracks.get_mut(&node_track).unwrap().nodes.push(node);
        Ok(())
    }

    ///Checks if the node on this track is scheduleable
    fn is_scheduleable(&self, track: &TrackId, node_idx: usize) -> bool {
        for dep in self.tracks.get(track).unwrap().nodes[node_idx]
            .dependencies
            .iter()
        {
            //NOTE: We resolve imports always as *ok*, since the frame is necessarily already in flight when this
            // frame is build.
            match &dep.participant {
                DepPart::Import => {}
                DepPart::Scheduled {
                    track: dep_track,
                    node_idx,
                } => {
                    //TODO: We probably can ignore if the track is our track and the idx is smaller...
                    //      For a first implementation this is however clearer.

                    //Check if it is scheduled, otherwise end early.
                    if !self.tracks.get(dep_track).unwrap().is_scheduled(*node_idx) {
                        return false;
                    }
                }
            }
        }

        true
    }

    ///Builds frames for the current config
    fn build_frames(&mut self) -> Result<(), RecordError> {
        //We go through each track and try to schedule as many tasks as possible.
        // We end scheduling whenever a task has an unscheduled dependency.
        let all_tracks = self.tracks.keys().cloned().collect::<Vec<_>>();
        while self.unscheduled_tasks() {
            //tracks if we scheduled anything, If we end the loop and this is still
            // false, we have a dead lock :<.
            let mut any_scheduled = false;

            //one pass tries to build a frame per track, but at least one overall
            for track_id in &all_tracks {
                let mut is_first_scheduled = false;

                while let Some(next) = self.tracks.get(&track_id).unwrap().next_to_schedule() {
                    //check dependencies
                    if self.is_scheduleable(track_id, next) {
                        //if first node, add new frame, otherwise just push to last
                        if !is_first_scheduled {
                            self.tracks.get_mut(track_id).unwrap().start_next_frame();
                            is_first_scheduled = true;
                        }
                        //add node to current track
                        //Safety: Both unwraps are valid the id comes from the iter, the frame must be there, since at least
                        // one has been created before.
                        self.tracks
                            .get_mut(track_id)
                            .unwrap()
                            .frames
                            .last_mut()
                            .unwrap()
                            .push_node(next);
                    } else {
                        //break from the scheduling loop of the track if we found something unscheduleable.
                        break;
                    }
                }

                //overwrite any flag if we scheduled at least one.
                if is_first_scheduled {
                    any_scheduled = true;
                }
            }

            if !any_scheduled {
                return Err(RecordError::DeadLock);
            }
        }

        Ok(())
    }
}

impl<'t> Display for TaskSchedule<'t> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Task schedule:\n")?;

        for (id, track) in &self.tracks {
            //header
            write!(f, "{:x} :", id.0.as_raw())?;
            for task in &track.nodes {
                write!(f, "----{}----", task)?;
            }
            writeln!(f, "")?;
        }
        writeln!(f, "")?;

        writeln!(f, "Frames: ")?;
        for (id, track) in &self.tracks {
            //header
            write!(f, "{:x} :", id.0.as_raw())?;
            for frame in &track.frames {
                write!(f, "----{}----", frame)?;
            }
            writeln!(f, "")?;
        }

        writeln!(f, "")
    }
}
