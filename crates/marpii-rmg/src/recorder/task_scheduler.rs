use std::fmt::Display;

use ahash::AHashMap;

use crate::{track::TrackId, resources::res_states::AnyResKey, Rmg, RecordError};

use super::{TrackEvent, TaskRecord, scheduler::Schedule};

///There are currently two types of dependencies.
/// 1. Data dependency: Means A needs data that is used/produced by B
/// 2. TaskOrder: Means B needs to be executed before A.
enum DependecyTy{
    Data(AnyResKey),
    TaskOrder
}


//Participant in an dependency definition
enum DepPart{
    ///When imported for first use in graph.
    Import,
    ///When it is a scheduled task
    Scheduled{
        track: TrackId,
        task_idx: usize,
    }
}

struct Dependency{
    participant: DepPart,
    dep: DependecyTy
}

//Single task node enumerating dependencies and dependees of this task
struct TaskNode<'t>{
    ///All dependencies needed for this task to execute
    dependencies: Vec<Dependency>,
    ///Dependees that depend on this task, or data from this task
    dependees: Vec<Dependency>,
    task: TaskRecord<'t>,
}

impl<'t> Display for TaskNode<'t>{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "| ")?;

        for dep in &self.dependencies{
            let s = match dep.participant{
                DepPart::Import => format!("Imp"),
                DepPart::Scheduled { track, task_idx } => format!("{:x}:{}", track.0.as_raw(), task_idx),
            };
            write!(f, " {} ", s)?;
        }

        write!(f, "  {}  ", self.task.task.name())?;


        for dep in &self.dependees{
            let s = match dep.participant{
                DepPart::Import => "Imp".to_string(),
                DepPart::Scheduled { track, task_idx } => format!("{:x}:{}", track.0.as_raw(), task_idx),
            };
            write!(f, " {} ", s)?;
        }

        write!(f, " |")
    }
}

///Schedule of a single track
struct TrackSchedule<'t>{
    nodes: Vec<TaskNode<'t>>,
    //offset to be added to a nodes index to get the semaphore value this task works on.
    semaphore_offset: u64,
}

impl<'t> TrackSchedule<'t>{

}

///Only finds out when which task is scheduled. Does not do resource management.
pub struct TaskSchedule<'t>{
    tracks: AHashMap<TrackId, TrackSchedule<'t>>,
    ///Tracks on which track some resource is currently owned.
    resource_residency: AHashMap<AnyResKey, (TrackId, usize)>,
}

impl<'t> TaskSchedule<'t>{
    pub fn new_from_tasks(
        rmg: &mut Rmg,
        records: Vec<TaskRecord<'t>>
    ) -> Result<Self, RecordError>{
        let tracks = rmg.tracks.0.iter().map(|(id, track)| (*id, TrackSchedule{
            nodes: Vec::with_capacity(10),
            semaphore_offset: track.latest_signaled_value
        })).collect();

        let mut schedule = TaskSchedule { tracks, resource_residency: AHashMap::default() };

        //add all tasks, which will (implicitly) add inter-task dependencies wherever needed.
        for record in records{
            schedule.add_task(rmg, record)?;
        }

        //now figure out

        Ok(schedule)
    }

    fn add_task(
        &mut self,
        rmg: &mut Rmg,
        task: TaskRecord<'t>
    ) -> Result<(), RecordError>{
        //allocate node
        let node_track = rmg.tracks.track_for_usage(task.task.queue_flags().into()).ok_or(RecordError::NoFittingTrack(task.task.queue_flags()))?;
        let node_idx = self.tracks.get_mut(&node_track).ok_or_else(|| RecordError::NoFittingTrack(task.task.queue_flags()))?.nodes.len();
        let mut node = TaskNode{
            task,
            dependees: Vec::new(),
            dependencies: Vec::new(),
        };

        //resolve dependencies
        for res in node.task.registry.any_res_iter(){
            let dep = if let Some(residency) = self.resource_residency.get_mut(&res){
                let to_add = Dependency{
                    participant: DepPart::Scheduled { track: residency.0, task_idx: residency.1 },
                    dep: DependecyTy::Data(res)
                };

                //signal as dependee to the task we take it from
                self.tracks.get_mut(&residency.0).unwrap().nodes[residency.1].dependees.push(Dependency{
                    participant: DepPart::Scheduled { track: node_track, task_idx: node_idx },
                    dep: DependecyTy::Data(res)
                });

                //and move resource ownership
                *residency = (node_track, node_idx);

                to_add
            }else{
                //Mark as import
                let dep = Dependency{
                    participant: DepPart::Import,
                    dep: DependecyTy::Data(res)
                };
                //add to residency tracker
                self.resource_residency.insert(res, (node_track, node_idx));
                dep
            };
            node.dependencies.push(dep);
        }
        self.tracks.get_mut(&node_track).unwrap().nodes.push(node);
        Ok(())
    }
}


impl<'t> Display for TaskSchedule<'t>{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Task schedule:\n")?;
        for (id, track) in &self.tracks{
            //header
            write!(f, "{:x} :", id.0.as_raw())?;

            for task in &track.nodes{
                write!(f, "----{}----", task)?;
            }
            writeln!(f, "")?;
        }

        writeln!(f, "")
    }
}
