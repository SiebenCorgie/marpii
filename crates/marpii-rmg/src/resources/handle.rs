use std::sync::Arc;
use crossbeam_channel::Sender;
use crate::{ImageKey, AnyResKey};


pub struct ImgHandle{
    //reference to the key. The arc signals the garbage collector when we
    // dropped
    key_ref: Arc<ImageKey>,
    drop_signal: Sender<AnyResKey>,
}
