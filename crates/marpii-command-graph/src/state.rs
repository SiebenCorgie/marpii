use std::{
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
};

use marpii::ash::vk;
use marpii::resources::{Buffer, Image};
use marpii_commands::Recorder;

use crate::{pass::AssumedState, UNDEFINED_QUEUE};

#[derive(Clone, Debug)]
pub struct ImageState {
    pub layout: vk::ImageLayout,
    pub access_mask: vk::AccessFlags,
}

///Stateful image
//NOTE: We intensionaly do not expose the inner state/image handle since we want to prevent that the
//      State is cloned, and then overwritten. In that case the (image,state) pair could become invalid.
#[derive(Clone)]
pub struct StImage {
    //Current image state
    pub(crate) state: Arc<RwLock<ImageState>>,
    //Currently owning queue
    pub(crate) queue: Arc<RwLock<u32>>,
    //actual image
    pub(crate) image: Arc<Image>,
}

impl PartialEq for StImage {
    fn eq(&self, other: &Self) -> bool {
        self.image.inner == other.image.inner
    }
}

impl Eq for StImage {}

impl Hash for StImage {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.image.hash(hasher)
    }
}

impl StImage {
    pub fn unitialized(image: Image) -> Self {
        StImage {
            state: Arc::new(RwLock::new(ImageState {
                access_mask: vk::AccessFlags::empty(),
                layout: vk::ImageLayout::UNDEFINED,
            })),
            queue: Arc::new(RwLock::new(UNDEFINED_QUEUE)),
            image: Arc::new(image),
        }
    }

    ///Creates a shared version of StImage. Assumes that the supplied state information is the current image's state. Otherwise
    /// generated graph might be invalid regarding this image.
    pub fn shared(
        image: Arc<Image>,
        queue: u32,
        access_mask: vk::AccessFlags,
        layout: vk::ImageLayout,
    ) -> Self {
        StImage {
            state: Arc::new(RwLock::new(ImageState {
                access_mask,
                layout,
            })),
            queue: Arc::new(RwLock::new(queue)),
            image,
        }
    }

    pub fn state(&self) -> ImageState {
        self.state.read().unwrap().clone()
    }

    pub fn image(&self) -> &Arc<Image> {
        &self.image
    }

    ///Returns currently owning queue family
    pub fn queue_family(&self) -> u32 {
        *self.queue.read().unwrap()
    }
}

#[derive(Clone, Debug)]
pub struct BufferState {
    pub access_mask: vk::AccessFlags,
}

///Stateful image
#[derive(Clone)]
pub struct StBuffer {
    pub(crate) state: Arc<RwLock<BufferState>>,
    //Currently owning queue
    pub(crate) queue: Arc<RwLock<u32>>,
    pub(crate) buffer: Arc<Buffer>,
}

impl PartialEq for StBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.buffer.inner == other.buffer.inner
    }
}

impl Eq for StBuffer {}

impl Hash for StBuffer {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.buffer.hash(hasher)
    }
}

impl StBuffer {
    pub fn unitialized(buffer: Buffer) -> Self {
        StBuffer {
            state: Arc::new(RwLock::new(BufferState {
                access_mask: vk::AccessFlags::empty(),
            })),
            queue: Arc::new(RwLock::new(UNDEFINED_QUEUE)),
            buffer: Arc::new(buffer),
        }
    }
    pub fn state(&self) -> BufferState {
        self.state.read().unwrap().clone()
    }

    pub fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    ///Returns currently owning queue family
    pub fn queue_family(&self) -> u32 {
        *self.queue.read().unwrap()
    }
}

//Types of transitions
enum Trans {
    //initializes the image to the given layout mask and queue family
    ImgInit {
        img: Arc<Image>,
        mask: vk::AccessFlags,
        layout: vk::ImageLayout,
        queue: u32,
    },
    //Image format transitions, happens if the layout or acccess mask changes
    ImgFmt {
        img: Arc<Image>,
        src_mask: vk::AccessFlags,
        dst_mask: vk::AccessFlags,
        src_layout: vk::ImageLayout,
        dst_layout: vk::ImageLayout,
    },

    //Queuetransfer operation, opertionaly the layout can be transitioned as well
    ImgQueueOp {
        img: Arc<Image>,
        src_queue: u32,
        dst_queue: u32,
        src_mask: vk::AccessFlags,
        dst_mask: vk::AccessFlags,
        src_layout: vk::ImageLayout,
        dst_layout: vk::ImageLayout,
    },

    BufQueueOp {
        buffer: Arc<Buffer>,
        src_mask: vk::AccessFlags,
        dst_mask: vk::AccessFlags,
        src_queue: u32,
        dst_queue: u32,
    }, //TODO add other events.
       //     - Image data-preserving queue acquire/release
       //     - Buffer init
       //     - Buffer format trans
       //     - buffer Queue acquire/release
}

impl Trans {
    fn is_queue_transition(&self) -> bool {
        if let Trans::BufQueueOp { .. } | Trans::ImgQueueOp { .. } = &self {
            true
        } else {
            false
        }
    }
}

///Tracker of transitions that can calculate the optimal barriers
pub struct Transitions {
    transitions: Vec<Trans>,
}

impl Transitions {
    pub fn empty() -> Self {
        Transitions {
            transitions: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }

    ///Adds a image release operation which releases `image` from `queue`.
    pub fn release_image(&mut self, image: StImage, src_queue: u32, dst_queue: u32) {
        let current_state = image.state.read().unwrap().clone();
        self.transitions.push(Trans::ImgQueueOp {
            img: image.image.clone(),
            src_queue,
            dst_queue,
            src_mask: current_state.access_mask,
            dst_mask: current_state.access_mask,
            src_layout: current_state.layout,
            dst_layout: current_state.layout,
        });

        *image.queue.write().unwrap() = UNDEFINED_QUEUE;
    }

    ///Adds a image acquire operation which acquires `image` for `queue`.
    pub fn acquire_image(&mut self, image: StImage, src_queue: u32, dst_queue: u32) {
        let current_state = image.state.read().unwrap().clone();
        self.transitions.push(Trans::ImgQueueOp {
            img: image.image.clone(),
            src_queue,
            dst_queue,
            src_mask: current_state.access_mask,
            dst_mask: current_state.access_mask,
            src_layout: current_state.layout,
            dst_layout: current_state.layout,
        });

        *image.queue.write().unwrap() = dst_queue;
    }

    pub fn release_buffer(&mut self, buffer: StBuffer, src_queue: u32, dst_queue: u32) {
        let current_state = buffer.state.read().unwrap().clone();
        self.transitions.push(Trans::BufQueueOp {
            buffer: buffer.buffer.clone(),
            dst_mask: current_state.access_mask,
            src_mask: current_state.access_mask,
            src_queue,
            dst_queue,
        });
    }

    pub fn acquire_buffer(&mut self, buffer: StBuffer, src_queue: u32, dst_queue: u32) {
        let current_state = buffer.state.read().unwrap().clone();
        self.transitions.push(Trans::BufQueueOp {
            buffer: buffer.buffer.clone(),
            dst_mask: current_state.access_mask,
            src_mask: current_state.access_mask,
            src_queue,
            dst_queue,
        });
    }

    ///Adds a transition state transforms `src` into the assumed state on `queue`.
    pub fn add_into_assumed(&mut self, src: AssumedState, queue_family: u32) {
        match &src {
            AssumedState::Image { image, state } => {
                //There are two events that can occure.
                // 1. Image is not initialized. In that case simple transition from UNDEFINED to this queue and layout.
                // 2. Is initialized with some stat, in that case simple transfer.

                let imgstate = image.state();

                if imgstate.layout == vk::ImageLayout::UNDEFINED {
                    #[cfg(feature = "log_reasoning")]
                    log::trace!(
                        "Found uninitialized image {:?}, moving to state {:?}",
                        image.image().inner,
                        &state
                    );

                    self.transitions.push(Trans::ImgInit {
                        img: image.image().clone(),
                        mask: state.access_mask,
                        layout: state.layout,
                        queue: queue_family,
                    });
                } else {
                    //is transition. For sanity reasons, assert that the queue family matches the current tracked one. Otherwise
                    //panic for now. Later, when queue transitions are implemented, this should be handled by the graph by not calling
                    // `into_assumed` but acquire/release instead.
                    //TODO Re-Add assert!(queue_family == image.queue_family(), "Queue families for transition operation do not match: {} != {}", queue_family, image.queue_family());
                    self.transitions.push(Trans::ImgFmt {
                        img: image.image.clone(),
                        src_mask: imgstate.access_mask,
                        dst_mask: state.access_mask,
                        src_layout: imgstate.layout,
                        dst_layout: state.layout,
                    });
                }
            }
            AssumedState::Buffer { .. } => panic!("Buffer transitions not implemented!"),
        }

        //Signal state change internally
        src.apply_state();
    }

    pub fn record(self, cmd: &mut Recorder) {
        let Transitions { transitions } = self;
        //Note we need to move the images/buffers to the command buffers's scope. This is the reason for this wonky stuff below.

        let (queue_transitions, other) = transitions.into_iter().fold(
            (Vec::new(), Vec::new()),
            |(mut trans, mut other), this| {
                if this.is_queue_transition() {
                    trans.push(this);
                    (trans, other)
                } else {
                    other.push(this);
                    (trans, other)
                }
            },
        );

        //First shedule queue transitions
        cmd.record(move |dev, cmd| {
            let (buffer_barriers, image_barriers) = queue_transitions.iter().fold(
                (Vec::new(), Vec::new()),
                |(mut bufbar, mut imbar), trans| match trans {
                    Trans::BufQueueOp {
                        buffer,
                        src_queue,
                        dst_queue,
                        dst_mask,
                        src_mask,
                    } => {
                        bufbar.push(vk::BufferMemoryBarrier {
                            buffer: buffer.inner,
                            src_access_mask: *src_mask,
                            dst_access_mask: *dst_mask,
                            src_queue_family_index: *src_queue,
                            dst_queue_family_index: *dst_queue,
                            ..Default::default()
                        });

                        (bufbar, imbar)
                    }
                    Trans::ImgQueueOp {
                        img,
                        src_queue,
                        dst_queue,
                        src_layout,
                        dst_layout,
                        dst_mask,
                        src_mask,
                    } => {
                        imbar.push(vk::ImageMemoryBarrier {
                            image: img.inner,
                            src_access_mask: *src_mask,
                            dst_access_mask: *dst_mask,
                            old_layout: *src_layout,
                            new_layout: *dst_layout,
                            src_queue_family_index: *src_queue,
                            dst_queue_family_index: *dst_queue,
                            subresource_range: img.subresource_all(),
                            ..Default::default() //TODO ferify that data is kept
                        });

                        (bufbar, imbar)
                    }
                    _ => panic!("Found non queue transfer op in transfer iterator"),
                },
            );

            #[cfg(feature = "log_reasoning")]
            log::trace!(
                "Sheduling queue transitions: \nBuffer={:#?}\nImages={:#?}!",
                buffer_barriers,
                image_barriers
            );

            //Emit actual transition
            if !buffer_barriers.is_empty() || !image_barriers.is_empty() {
                unsafe {
                    dev.cmd_pipeline_barrier(
                        *cmd,
                        vk::PipelineStageFlags::ALL_COMMANDS, //TODO read from context, or split based on barrier types.
                        vk::PipelineStageFlags::ALL_COMMANDS, //TODO read from context, or split based on barrier types.
                        vk::DependencyFlags::empty(),
                        &[],
                        buffer_barriers.as_slice(),
                        image_barriers.as_slice(),
                    )
                }
            }
        });

        //Now shedule all normal dependencies
        cmd.record(move |dev, cmd| {
            let (buffer_barriers, image_barriers): (
                Vec<vk::BufferMemoryBarrier>,
                Vec<vk::ImageMemoryBarrier>,
            ) = other
                .iter()
                .fold((Vec::new(), Vec::new()), |(bufbar, mut imgbar), trans| {
                    match trans {
                        Trans::ImgInit {
                            img,
                            mask,
                            layout,
                            queue,
                        } => imgbar.push(vk::ImageMemoryBarrier {
                            image: img.inner,
                            src_access_mask: vk::AccessFlags::NONE,
                            dst_access_mask: *mask,
                            old_layout: vk::ImageLayout::UNDEFINED,
                            new_layout: *layout,
                            subresource_range: img.subresource_all(),
                            src_queue_family_index: *queue,
                            dst_queue_family_index: *queue,
                            ..Default::default()
                        }),
                        Trans::ImgFmt {
                            img,
                            src_mask,
                            dst_mask,
                            src_layout,
                            dst_layout,
                        } => imgbar.push(vk::ImageMemoryBarrier {
                            image: img.inner,
                            src_access_mask: *src_mask,
                            dst_access_mask: *dst_mask,
                            old_layout: *src_layout,
                            new_layout: *dst_layout,
                            subresource_range: img.subresource_all(),
                            ..Default::default()
                        }),
                        _ => todo!("Transition unimplemented!"),
                    }

                    (bufbar, imgbar)
                });

            #[cfg(feature = "log_reasoning")]
            log::trace!(
                "Sheduling barriers: \nBuffer={:#?}\nImages={:#?}!",
                buffer_barriers,
                image_barriers
            );
            if !buffer_barriers.is_empty() || !image_barriers.is_empty() {
                unsafe {
                    dev.cmd_pipeline_barrier(
                        *cmd,
                        vk::PipelineStageFlags::ALL_COMMANDS, //TODO read from context, or split based on barrier types.
                        vk::PipelineStageFlags::ALL_COMMANDS, //TODO read from context, or split based on barrier types.
                        vk::DependencyFlags::empty(),
                        &[],
                        buffer_barriers.as_slice(),
                        image_barriers.as_slice(),
                    )
                }
            }
        });
    }
}
