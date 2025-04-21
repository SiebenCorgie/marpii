use std::{
    f32,
    hash::{Hash, Hasher},
};

use ahash::AHashMap;
use gradient::QuadGradientPass;
use iced::Rectangle;
use iced_graphics::Settings;
use iced_marpii_shared::{CmdQuad, CmdQuadGradient};
use marpii::ash::vk;
use marpii_rmg::{ImageHandle, MetaTask, Rmg};
use solid::QuadPass;

use crate::batch_cache::{Batch, BatchCall, BatchId, BufferState, CachedBatch};

pub(crate) mod gradient;
pub(crate) mod solid;

///The vertex/index-buffer less quad renderer.
///
/// We use DynamicRendering + PushConstants to setup
/// the quad renderer.
pub struct QuadRenderer {
    ///Identifies a batch by its content's hash.
    solid_batch_cache: AHashMap<u64, CachedBatch<CmdQuad>>,
    ///Identifies a batch by its content's hash.
    gradient_batch_cache: AHashMap<u64, CachedBatch<CmdQuadGradient>>,

    ///Order of batches to render, and their layer depth
    order: Vec<BatchId>,

    solid_pass: QuadPass,
    gradient_pass: QuadGradientPass,
}

impl QuadRenderer {
    ///How many frames a buffer can be unused before being deleted.
    const MAX_NO_USE: usize = 10;

    pub fn new(
        rmg: &mut Rmg,
        settings: &Settings,
        color_buffer: ImageHandle,
        depth_buffer: ImageHandle,
    ) -> Self {
        let solid_pass = QuadPass::new(rmg, settings, color_buffer.clone(), depth_buffer.clone());
        let gradient_pass = QuadGradientPass::new(rmg, settings, color_buffer, depth_buffer);

        Self {
            solid_batch_cache: AHashMap::default(),
            gradient_batch_cache: AHashMap::default(),
            order: Vec::new(),
            solid_pass,
            gradient_pass,
        }
    }

    pub fn set_clear_color(&mut self, color: Option<[f32; 4]>) {
        self.solid_pass.clear_color = color;
    }

    pub fn notify_resize(&mut self, color_buffer: ImageHandle, depth_buffer: ImageHandle) {
        self.solid_pass
            .resize(color_buffer.clone(), depth_buffer.clone());
        self.gradient_pass.resize(color_buffer, depth_buffer);
    }

    pub fn push_solid_batch(
        &mut self,
        rmg: &mut Rmg,
        batch: &mut Batch<CmdQuad>,
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

        let id = BatchId::Solid {
            id: hash,
            layer_depth,
        };

        if let Some(cached) = self.solid_batch_cache.get_mut(&hash) {
            log::trace!("Reusing quad-batch {hash}");
            //note: must be at least one, otherwise we'd try to reuse a batch twice.
            assert!(cached.last_use != 0, "batch was already reused");
            cached.last_use = 0;
            //overwrite bound
            cached.bound = bound;
            self.order.push(id)
        } else {
            self.solid_batch_cache
                .insert(hash, CachedBatch::new(rmg, batch, bound));
            self.order.push(id)
        }
    }
    pub fn push_gradient_batch(
        &mut self,
        rmg: &mut Rmg,
        batch: &mut Batch<CmdQuadGradient>,
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
                item.colors_0 = crate::util::gamma_correct(item.colors_0);
                item.colors_1 = crate::util::gamma_correct(item.colors_1);
                item.colors_2 = crate::util::gamma_correct(item.colors_2);
                item.colors_3 = crate::util::gamma_correct(item.colors_3);
                item.colors_4 = crate::util::gamma_correct(item.colors_4);
                item.colors_5 = crate::util::gamma_correct(item.colors_5);
                item.colors_6 = crate::util::gamma_correct(item.colors_6);
                item.colors_7 = crate::util::gamma_correct(item.colors_7);
            }
        }

        let mut hasher = ahash::AHasher::default();
        batch.hash(&mut hasher);
        let hash = hasher.finish();

        let id = BatchId::Gradient {
            id: hash,
            layer_depth,
        };

        if let Some(cached) = self.gradient_batch_cache.get_mut(&hash) {
            log::trace!("Reusing quad-batch {hash}");
            //note: must be at least one, otherwise we'd try to reuse a batch twice.
            assert!(cached.last_use != 0, "batch was already reused");
            cached.last_use = 0;
            //overwrite bound
            cached.bound = bound;
            self.order.push(id)
        } else {
            self.gradient_batch_cache
                .insert(hash, CachedBatch::new(rmg, batch, bound));
            self.order.push(id)
        }
    }

    pub fn begin_new_frame(&mut self, viewport: &iced_graphics::Viewport) {
        //setup _general_transform_
        self.solid_pass.push.get_content_mut().transform = viewport.projection().into();
        self.solid_pass.push.get_content_mut().scale = viewport.scale_factor() as f32;
        self.gradient_pass.push.get_content_mut().transform = viewport.projection().into();
        self.gradient_pass.push.get_content_mut().scale = viewport.scale_factor() as f32;

        //last-use flag update
        for batch in self.solid_batch_cache.values_mut() {
            batch.last_use += 1;
        }
        for batch in self.gradient_batch_cache.values_mut() {
            batch.last_use += 1;
        }
        //clear order
        self.order.clear();
    }

    pub fn prepare_data(&mut self, rmg: &mut Rmg) {
        let mut upload_recorder = rmg.record();

        //Add an upload task, if a cached item is not yet uploaded, but will be used.
        for batch in self.solid_batch_cache.values_mut() {
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

        for batch in self.gradient_batch_cache.values_mut() {
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
        for batch in self.solid_batch_cache.values_mut() {
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
        for batch in self.gradient_batch_cache.values_mut() {
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
        self.solid_batch_cache
            .retain(|_k, v| v.last_use < Self::MAX_NO_USE);
        self.gradient_batch_cache
            .retain(|_k, v| v.last_use < Self::MAX_NO_USE);
    }
}

impl MetaTask for QuadRenderer {
    fn record<'a>(
        &'a mut self,
        recorder: marpii_rmg::Recorder<'a>,
    ) -> Result<marpii_rmg::Recorder<'a>, marpii_rmg::RecordError> {
        //Clear the old data
        self.solid_pass.batches.clear();
        self.gradient_pass.batches.clear();

        //transform batchen, in-order, into batch calls
        for batch_id in self.order.iter() {
            match batch_id {
                BatchId::Solid { id, layer_depth } => {
                    let batch = self.solid_batch_cache.get(&id).unwrap();
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
                    self.solid_pass.batches.push(batch_call);
                }
                BatchId::Gradient { id, layer_depth } => {
                    let batch = self.gradient_batch_cache.get(&id).unwrap();
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
                    self.gradient_pass.batches.push(batch_call);
                }
            }
        }

        recorder
            .add_task(&mut self.solid_pass)
            .unwrap()
            .add_task(&mut self.gradient_pass)
    }
}
