use fxhash::FxHashMap;

use crate::{AnyResKey, ResourceError, recorder::task::AttachmentDescription, ImageKey};


struct RuntimeInfo{
    last_use: u64,
    timeout: u64,
}

impl RuntimeInfo{
    fn timeout_epoch(&self) -> u64{
        self.last_use + self.timeout
    }
}


///Temporary resource manager. Keeps track of resources that where created temporarily,
/// and when they where used last.
pub struct TempResources{
    ///Current epoch we are at
    epoch: u64,

    res_map: FxHashMap<AnyResKey, RuntimeInfo>,

    remove_buffer: Vec<AnyResKey>,
}


impl TempResources {

    pub const DEFAULT_TIMEOUT: u64 = 31;

    pub fn new() -> Self{
        TempResources {
            epoch: 0,
            res_map: FxHashMap::default(),
            remove_buffer: Vec::new()
        }
    }

    pub fn register(&mut self, res: AnyResKey, timeout: u64) -> Result<(), ResourceError>{
        if let Some(old) = self.res_map.insert(res, RuntimeInfo { last_use: self.epoch, timeout }){
            Err(ResourceError::ResourceExists(res))
        }else{
            Ok(())
        }
    }

    pub fn get_image(&mut self, des: AttachmentDescription) -> Option<ImageKey>{
        todo!();
        None
    }

    ///Ticks the tracker, adds all resources that can be dropped to the list.
    pub fn tick(&mut self, drop_list: &mut Vec<AnyResKey>){
        self.epoch += 1;
        self.remove_buffer.clear();
        for (reskey, info) in self.res_map.iter(){
            if info.timeout_epoch() > self.epoch{

                #[cfg(feature="logging")]
                log::trace!("Scheduling removing of temporary resource {:?}", reskey);
                self.remove_buffer.push(*reskey);
            }
        }

        //remove all from tracking
        for rem in &self.remove_buffer{
            assert!(self.res_map.remove(rem).is_some());
        }
    }


}
