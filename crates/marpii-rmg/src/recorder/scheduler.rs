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

impl<'rmg> CmdFrame<'rmg>  {
    fn new() -> Self{
        CmdFrame { acquire: Vec::new(), init: Vec::new(), release: Vec::new(), tasks: Vec::new() }
    }
}


///Represents all frames for this specific track.
struct TrackRecord<'rmg>{
    frames: Vec<CmdFrame<'rmg>>
}

impl<'rmg> TrackRecord<'rmg>{
    fn current_frame(&self) -> usize{
        self.frames.len() - 1
    }
    fn finish_frame(&mut self){
        self.frames.push(CmdFrame::new())
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
            frames: vec![CmdFrame::new()]
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
            let new_loc = self.request_res(rmg, track_id, res)?;
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
    fn request_res<'a>(&mut self, rmg: &'a mut Rmg, track: TrackId, res: AnyResKey) -> Result<ResLocation, RecordError>{
        //Check if we know where the res is at. If so, arrange release/acquire. Otherwise we have to either import the res,
        // or if the res was just created, init it

        let dst_loc = ResLocation{track, frame: self.tracks.get(&track).unwrap().current_frame()};

        if let Some(src_loc) = self.known_res.remove(&res){
            //found, release it from current location to new one.
            //
            // if the frame we release from is the *current*, we also end the frame.
            self.tracks.get_mut(&src_loc.track).unwrap().frames[src_loc.frame].release.push(Release {
                to: dst_loc,
                res
            });

            self.tracks.get_mut(&dst_loc.track).unwrap().frames[dst_loc.frame].acquire.push(Acquire { from: src_loc, res });

            //if we where on the same frame, finish
            if src_loc.frame == self.tracks.get(&src_loc.track).unwrap().current_frame(){
                self.tracks.get_mut(&src_loc.track).unwrap().finish_frame();
                debug_assert!(self.tracks.get(&src_loc.track).unwrap().current_frame() == src_loc.frame + 1);
            }else{
                debug_assert!(self.tracks.get(&src_loc.track).unwrap().current_frame() > src_loc.frame)
            }

        }else{
            //check if the resource was initialised yet. If not we init on this track/frame. Otherwise we add a release op on the header
            // of the currently owning track and a acquire for us.
            if res.is_initialised(rmg){
                #[cfg(feature="logging")]
                log::trace!("Import res={:?}", res);

                //Note, we try to import from origin track. If there is none this a state less object like a sampler. In that case we ignore ownership
                // transfer all together
                if let Some(origin_track) = res.current_owner(rmg){
                    //Note that we release from the current owner by pushing the release to the firs track
                    #[cfg(feature="logging")]
                    log::trace!("Importing outside from {:?} to {:?} for res={:?}", origin_track, track, res);


                    self.tracks.get_mut(&origin_track).unwrap().frames[0].release.push(Release { to: dst_loc, res});
                    self.tracks.get_mut(&dst_loc.track).unwrap().frames[dst_loc.frame].acquire.push(Acquire { from: ResLocation { track: origin_track, frame: 0 }, res })
                }else{
                    #[cfg(feature="logging")]
                    log::trace!("Ignoring ownership transfer for res={:?}", res);
                }

            }else{
                //add as init
                #[cfg(feature="logging")]
                log::trace!("Init res={:?}, seeing for first time", res);

                self.tracks.get_mut(&dst_loc.track).unwrap().frames[dst_loc.frame].init.push(Init { res });
            }
        }


        //now update inner tracker. Note that the key was removed in the Some case, or never added at all in the none case above.
        self.known_res.insert(res, dst_loc);

        Ok(dst_loc)
    }
}
