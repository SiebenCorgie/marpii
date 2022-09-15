use fxhash::FxHashMap;
use std::sync::Arc;
use marpii::sync::Semaphore;

use crate::{Rmg, RecordError, track::TrackId, resources::res_states::AnyResKey};

use super::TaskRecord;



struct Acquire{
    //The track, and frame index this aquires from
    from: ResLocation,
    res: AnyResKey
}


struct Init{
    res: AnyResKey
}

struct Release{
    to: ResLocation,
    res: AnyResKey
}

///A frame is a set of tasks on a certain Track that can be executed after each other without having to synchronise via
/// Semaphores in between.
struct CmdFrame<'rmg>{
    acquire: Vec<Acquire>,
    init: Vec<Init>,
    release: Vec<Release>,


    tasks: Vec<TaskRecord<'rmg>>
}

///Represents all frames for this specific track.
struct TrackRecord<'rmg>{
    frames: Vec<CmdFrame<'rmg>>
}

impl<'rmg> TrackRecord<'rmg>{
    fn current_frame(&self) -> usize{
        self.frames.len() - 1
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct ResLocation{
    track: TrackId,
    frame: usize
}

pub struct Schedule<'rmg>{
    known_res: FxHashMap<AnyResKey, ResLocation>,
    tracks: FxHashMap<TrackId, TrackRecord<'rmg>>
}

impl<'rmg> Schedule<'rmg> {
    pub(crate) fn from_tasks(rmg: &'rmg mut Rmg, records: Vec<TaskRecord<'rmg>>) -> Result<Self, RecordError>{

        //setup at least one frame per track.
        let tracks = rmg.tracks.0.iter().map(|(id, _track)| (*id, TrackRecord{
            frames: vec![CmdFrame{
                acquire: Vec::new(),
                init: Vec::new(),
                release: Vec::new(),
                tasks: Vec::new()
            }]
        })).collect();

        let mut schedule = Schedule{
            known_res: FxHashMap::default(),
            tracks
        };

        for task in records{
            schedule.schedule_task(rmg, task)?;
        }


        Ok(schedule)
    }


    fn schedule_task<'a>(&mut self, rmg: &'a mut Rmg, task: TaskRecord<'rmg>) -> Result<(), RecordError>{
        let track_id = rmg.tracks.track_for_usage(task.task.queue_flags()).ok_or(RecordError::NoFittingTrack(task.task.queue_flags()))?;
        let frame_index = self.tracks.get(&track_id).unwrap().current_frame();

        let record_location = ResLocation{track: track_id, frame: frame_index};

        //now move all resources to this track and add to the newest frame on this track
        for res in task.registry.any_res_iter(){
            let new_loc = self.request_res(track_id, res)?;
            assert!(new_loc == record_location); //sanity check
            debug_assert!(self.known_res.get(&res).unwrap() == &new_loc);
        }

        //Finally push frame to this index
        debug_assert!(self.tracks.get(&record_location.track).unwrap().current_frame() == record_location.frame);
        self.tracks.get_mut(&record_location.track).unwrap().frames[record_location.frame].tasks.push(task);

        Ok(())
    }

    ///Requests the resource on the given track. Note that this will always be written to the *latest* frame.
    /// Returns the new location if successful.
    fn request_res(&mut self, track: TrackId, res: AnyResKey) -> Result<ResLocation, RecordError>{
        todo!()
    }
}
