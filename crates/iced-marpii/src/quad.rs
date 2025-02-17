use std::{
    f32,
    hash::{Hash, Hasher},
};

use ahash::AHashMap;
use iced::Rectangle;
use iced_graphics::Settings;
use iced_marpii_shared::{CmdQuad, ResourceHandle};
use marpii::ash::vk;
use marpii_rmg::{BufferHandle, ImageHandle, MetaTask, Rmg};
use marpii_rmg_tasks::UploadBuffer;
use solid::QuadPass;

mod gradient;
mod solid;

pub type Batch = Vec<CmdQuad>;

enum BufferState {
    Uploading {
        was_enqueued: bool,
        upload: UploadBuffer<CmdQuad>,
    },
    Residing(BufferHandle<CmdQuad>),
}

impl BufferState {
    pub fn is_residing(&self) -> bool {
        if let Self::Residing(_) = self {
            true
        } else {
            false
        }
    }

    pub fn unwrap_handle(&self) -> BufferHandle<CmdQuad> {
        if let Self::Residing(hdl) = self {
            hdl.clone()
        } else {
            panic!("Handle not yet residing")
        }
    }
}

///A cached quad-draw batch.
struct CachedBatch {
    ///A flag that is incremented whenever the batch was not used in a frame.
    ///Allows us to delete buffers that where not used for a set of frames.
    last_use: usize,
    buffer: BufferState,
    batch_size: usize,
    //The bound this batch is drawn in
    bound: Rectangle,
}

impl CachedBatch {
    ///How many frames a buffer can be unused before being deleted.
    const MAX_NO_USE: usize = 10;

    pub fn new(rmg: &mut Rmg, batch: &Batch, bound: Rectangle) -> Self {
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

///The quad calls that are enqueued.
pub(crate) struct BatchCall {
    buffer: BufferHandle<CmdQuad>,
    resource_handle: Option<ResourceHandle>,
    count: usize,
    bound: vk::Rect2D,
    layer_depth: f32,
}

///The vertex/index-buffer less quad renderer.
///
/// We use DynamicRendering + PushConstants to setup
/// the quad renderer.
pub struct QuadRenderer {
    ///Identifies a batch by its content's hash.
    batch_cache: AHashMap<u64, CachedBatch>,
    ///Order of batches to render, and their layer depth
    order: Vec<(u64, f32)>,

    pass: QuadPass,
}

impl QuadRenderer {
    pub fn new(
        rmg: &mut Rmg,
        settings: &Settings,
        color_buffer: ImageHandle,
        depth_buffer: ImageHandle,
    ) -> Self {
        let pass = QuadPass::new(rmg, settings, color_buffer, depth_buffer);

        Self {
            batch_cache: AHashMap::default(),
            order: Vec::new(),
            pass,
        }
    }

    pub fn set_clear_color(&mut self, color: Option<[f32; 4]>) {
        self.pass.clear_color = color;
    }

    pub fn notify_resize(&mut self, color_buffer: ImageHandle, depth_buffer: ImageHandle) {
        self.pass.resize(color_buffer, depth_buffer);
    }

    pub fn push_batch(
        &mut self,
        rmg: &mut Rmg,
        batch: &mut Batch,
        bound: Rectangle,
        layer_depth: f32,
        gamma_correct: bool,
    ) {
        //Do not push batches, that are empty
        if batch.len() == 0 {
            return;
        }

        if gamma_correct {
            for item in batch.iter_mut() {
                item.border_color = crate::util::gamma_correct(item.border_color);
                item.shadow_color = crate::util::gamma_correct(item.shadow_color);
                item.color = crate::util::gamma_correct(item.color);
            }
        }

        let mut hasher = ahash::AHasher::default();
        batch.hash(&mut hasher);
        let hash = hasher.finish();
        if let Some(cached) = self.batch_cache.get_mut(&hash) {
            log::trace!("Reusing quad-batch {hash}");
            //note: must be at least one, otherwise we'd try to reuse a batch twice.
            assert!(cached.last_use != 0, "batch was alredy reused");
            cached.last_use = 0;
            //overwrite bound
            cached.bound = bound;
            self.order.push((hash, layer_depth))
        } else {
            self.batch_cache
                .insert(hash, CachedBatch::new(rmg, batch, bound));
            self.order.push((hash, layer_depth))
        }
    }

    pub fn begin_new_frame(&mut self, viewport: &iced_graphics::Viewport) {
        //setup _general_transform_
        self.pass.push.get_content_mut().transform = viewport.projection().into();
        self.pass.push.get_content_mut().scale = viewport.scale_factor() as f32;

        //last-use flag update
        for batch in self.batch_cache.values_mut() {
            batch.last_use += 1;
        }
        //clear order
        self.order.clear();
    }

    pub fn prepare_data(&mut self, rmg: &mut Rmg) {
        let mut upload_recorder = rmg.record();

        for batch in self.batch_cache.values_mut() {
            if batch.last_use != 0 {
                continue;
            }

            match &mut batch.buffer {
                BufferState::Uploading {
                    was_enqueued,
                    upload,
                } => {
                    if !*was_enqueued {
                        *was_enqueued = true;
                    } else {
                        //ignore, if already enqueued
                        continue;
                    };
                    upload_recorder = upload_recorder.add_task(upload).unwrap();
                }
                BufferState::Residing(_) => {}
            }
        }

        //now upload all batches
        upload_recorder.execute().unwrap();

        //finally transition all to residing
        for batch in self.batch_cache.values_mut() {
            match &mut batch.buffer {
                BufferState::Residing(_buf) => {
                    //if already residing, don't do anything
                }
                BufferState::Uploading {
                    was_enqueued,
                    upload,
                } => {
                    if !*was_enqueued {
                        panic!("quad upload failed");
                    } else {
                        //was already enqueued, so we can transition to residing
                        batch.buffer = BufferState::Residing(upload.buffer.clone());
                    }
                }
            }
        }
    }

    pub fn end_frame(&mut self) {
        //Remove all cached buffer, where the last-use is too long ago
        self.batch_cache
            .retain(|_k, v| v.last_use < CachedBatch::MAX_NO_USE);
    }
}

impl MetaTask for QuadRenderer {
    fn record<'a>(
        &'a mut self,
        recorder: marpii_rmg::Recorder<'a>,
    ) -> Result<marpii_rmg::Recorder<'a>, marpii_rmg::RecordError> {
        self.pass.batches.clear();
        //transform batchen, in-order, into batch calls
        for (batch_id, layer_depth) in self.order.iter() {
            let batch = self.batch_cache.get(batch_id).unwrap();
            assert!(batch.buffer.is_residing());

            let batch_call = BatchCall {
                bound: vk::Rect2D {
                    offset: vk::Offset2D {
                        x: batch.bound.x as i32,
                        y: batch.bound.y as i32,
                    },
                    extent: vk::Extent2D {
                        width: batch.bound.width as u32,
                        height: batch.bound.height as u32,
                    },
                },
                buffer: batch.buffer.unwrap_handle(),
                resource_handle: None,
                count: batch.batch_size,
                layer_depth: *layer_depth,
            };
            self.pass.batches.push(batch_call);
        }

        recorder.add_task(&mut self.pass)
    }
}
