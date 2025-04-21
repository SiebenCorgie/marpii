//! Abstraction for content-identified batches.

use iced::Rectangle;
use iced_marpii_shared::ResourceHandle;
use marpii::{ash::vk, bytemuck::Pod};
use marpii_rmg::{BufferHandle, Rmg};
use marpii_rmg_tasks::UploadBuffer;

//Generic content-identified batch.
pub type Batch<CmdTy> = Vec<CmdTy>;

///Buffer-State of such a batch.
pub(crate) enum BufferState<CmdTy: Pod + 'static> {
    Uploading {
        was_enqueued: bool,
        upload: UploadBuffer<CmdTy>,
    },
    Residing(BufferHandle<CmdTy>),
}

impl<CmdTy: Pod + 'static> BufferState<CmdTy> {
    pub fn is_residing(&self) -> bool {
        if let Self::Residing(_) = self {
            true
        } else {
            false
        }
    }

    pub fn unwrap_handle(&self) -> BufferHandle<CmdTy> {
        if let Self::Residing(hdl) = self {
            hdl.clone()
        } else {
            panic!("Handle not yet residing")
        }
    }
}

///A cached batch command
pub(crate) struct CachedBatch<CmdTy: Pod + 'static> {
    ///A flag that is incremented whenever the batch was not used in a frame.
    ///Allows us to delete buffers that where not used for a set of frames.
    pub last_use: usize,
    pub buffer: BufferState<CmdTy>,
    pub batch_size: usize,
    //The bound this batch is drawn in
    pub bound: Rectangle,
}

impl<CmdTy: Pod + 'static> CachedBatch<CmdTy> {
    pub fn new(rmg: &mut Rmg, batch: &Batch<CmdTy>, bound: Rectangle) -> Self {
        let size = batch.len();
        let upload = UploadBuffer::new(rmg, batch.as_slice()).unwrap();
        CachedBatch {
            last_use: 0,
            buffer: BufferState::Uploading {
                was_enqueued: false,
                upload,
            },
            batch_size: size,
            bound,
        }
    }
}

///All data needed to enqueue a batch into a pipeline
pub(crate) struct BatchCall<CmdTy: 'static> {
    pub buffer: BufferHandle<CmdTy>,
    pub resource_handle: Option<ResourceHandle>,
    pub count: usize,
    pub bound: vk::Rect2D,
    pub layer_depth: f32,
}

///Idientifies a batch by its content-hash, and its layer depth
pub(crate) enum BatchId {
    Solid { id: u64, layer_depth: f32 },
    Gradient { id: u64, layer_depth: f32 },
}
