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

///Different states of queue ownership of resources.
pub enum QueueState {
    ///No queue has taken ownership yet
    Uninitialized,
    ///Owned by the given queue family.
    Owned(u32),
    ///Queue has been released `from` queue `to` queu
    Released { from: u32, to: u32 },
}

impl QueueState {
    ///Returns the currently owning queue family. Returns [UNDEFINED_QUEUE](crate::UNDEFINED_QUEUE) if no family
    /// is currently owning the resource
    pub fn queue_family(&self) -> u32 {
        match self {
            QueueState::Owned(i) => *i,
            _ => UNDEFINED_QUEUE,
        }
    }

    pub fn acquire_to(&mut self, src: u32, dst: u32) {
        #[cfg(feature = "logging")]
        match &self {
            QueueState::Released { from, to } => {
                log::trace!("Acquiring image from {} to {}!", from, to);
                if src != *from || dst != *to {
                    log::error!("Release information does not match acquire information: Release[from={}, to={}], Acquire[from={}, to={}]", from, to, src, dst);
                }
            }
            QueueState::Owned(_i) => log::warn!("Cannot Acquire owned image"),
            QueueState::Uninitialized => log::trace!("Initializing to queue {}", dst),
        }

        *self = QueueState::Owned(dst);
    }

    pub fn release_to(&mut self, src: u32, dst: u32) {
        #[cfg(feature = "logging")]
        match &self {
            QueueState::Uninitialized => log::warn!("Cannot release uninitialized image!"),
            QueueState::Released { .. } => log::warn!("Cannot release already released image!"),
            QueueState::Owned(_i) => log::trace!("Releasing image from {} to {}", src, dst),
        }

        *self = QueueState::Released { from: src, to: dst };
    }

    ///Returns true if the resource is currently owned by this `family`
    pub fn is_owned(&self, family: u32) -> bool {
        if let QueueState::Owned(f) = &self {
            *f == family
        } else {
            false
        }
    }

    pub fn is_released(&self, from_queue: u32, to_queue: u32) -> bool {
        match self {
            QueueState::Released { from, to } => *from == from_queue && *to == to_queue,
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub fn is_uninitialized(&self) -> bool {
        if let QueueState::Uninitialized = self {
            true
        } else {
            false
        }
    }
}

///Stateful image. Tracks an internal state that is used to calculate layout and queue transitions within a
/// graph.
//NOTE: We intensionally do not expose the inner state/image handle since we want to prevent that the
//      State is cloned, and then overwritten. In that case the (image,state) pair could become invalid.
#[derive(Clone)]
pub struct StImage {
    //Current image state
    pub(crate) state: Arc<RwLock<ImageState>>,
    //Currently owning queue
    pub(crate) queue: Arc<RwLock<QueueState>>,
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
            queue: Arc::new(RwLock::new(QueueState::Uninitialized)),
            image: Arc::new(image),
        }
    }

    ///Creates a shared version of StImage. Assumes that the supplied state information is the current image's state. Otherwise
    /// generated graph might be invalid regarding this image.
    ///
    /// # Note
    ///
    /// You can use [UNDEFINED_QUEUE](crate::UNDEFINED_QUEUE) if the image has not been acquired yet.
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
            queue: Arc::new(RwLock::new(if queue == UNDEFINED_QUEUE {
                QueueState::Uninitialized
            } else {
                QueueState::Owned(queue)
            })),
            image,
        }
    }

    pub fn state(&self) -> ImageState {
        self.state.read().unwrap().clone()
    }

    ///Returns a reference to the current image.
    ///
    /// # Safety
    ///
    /// Do not transform the state (layout, access mask, queue ownership), since this would break assumptions made
    /// while generating graphs.
    pub fn image(&self) -> &Arc<Image> {
        &self.image
    }

    ///Returns currently owning queue family
    pub fn queue_family(&self) -> u32 {
        self.queue.read().unwrap().queue_family()
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
    pub(crate) queue: Arc<RwLock<QueueState>>,
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
            queue: Arc::new(RwLock::new(QueueState::Uninitialized)),
            buffer: Arc::new(buffer),
        }
    }
    pub fn state(&self) -> BufferState {
        self.state.read().unwrap().clone()
    }

    pub fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    ///Creates a shared version of StBuffer. Assumes that the supplied state information is the current buffers's state. Otherwise
    /// generated graph might be invalid regarding this buffer.
    ///
    /// # Note
    ///
    /// You can use [UNDEFINED_QUEUE](crate::UNDEFINED_QUEUE) if the buffer has not been acquired yet.
    pub fn shared(buffer: Arc<Buffer>, queue: u32, access_mask: vk::AccessFlags) -> Self {
        StBuffer {
            state: Arc::new(RwLock::new(BufferState { access_mask })),
            queue: Arc::new(RwLock::new(if queue == UNDEFINED_QUEUE {
                QueueState::Uninitialized
            } else {
                QueueState::Owned(queue)
            })),
            buffer,
        }
    }

    ///Returns currently owning queue family
    pub fn queue_family(&self) -> u32 {
        self.queue.read().unwrap().queue_family()
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

    ///Inits the buffer to the given mask and queue ownership.
    BufInit {
        buf: Arc<Buffer>,
        mask: vk::AccessFlags,
        queue: u32,
    },

    ///Moves the access mask of a buffer.
    BufFmt {
        buf: Arc<Buffer>,
        src_mask: vk::AccessFlags,
        dst_mask: vk::AccessFlags,
    },

    ///Transitions buffer from srcqueue to dstqueue. Might also change access mask.
    BufQueueOp {
        buf: Arc<Buffer>,
        src_mask: vk::AccessFlags,
        dst_mask: vk::AccessFlags,
        src_queue: u32,
        dst_queue: u32,
    },
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
    pub fn release_image(&mut self, image: &StImage, src_queue: u32, dst_queue: u32) {
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

        debug_assert!(
            image.queue.read().unwrap().is_owned(src_queue),
            "Released image = {:?} on queue = {}, but was not owned by this queue!",
            image.image().inner,
            src_queue
        );
        image
            .queue
            .write()
            .unwrap()
            .release_to(src_queue, dst_queue);
    }

    ///Initializes image ignoring data that might be written to it.
    pub fn init_image(&mut self, image: &StImage, queue: u32, state: &ImageState) {
        //While just initing to another queue works, the validation layers don't like this.
        //we throw a warning so the user could change the graph.

        match *image.queue.read().unwrap() {
            QueueState::Uninitialized => {
                #[cfg(feature = "log_reasoning")]
                log::trace!(
                    "Init image {:?} from uninit to queue {}",
                    image.image().inner,
                    queue
                );

                //Emmit simple init
                self.transitions.push(Trans::ImgInit {
                    img: image.image().clone(),
                    mask: state.access_mask,
                    layout: state.layout,
                    queue,
                });
            }
            QueueState::Released { from, to } => {
                #[cfg(feature = "log_reasoning")]
                log::trace!(
                    "Init image {:?} from released from: {} to: {}",
                    image.image().inner,
                    from,
                    to
                );

                let before_state = image.state();
                //was releaed, can acquire
                self.acquire_image(image, from, to);
                //then transition to state
                self.transitions.push(Trans::ImgFmt {
                    img: image.image().clone(),
                    src_mask: before_state.access_mask,
                    dst_mask: state.access_mask,
                    src_layout: before_state.layout,
                    dst_layout: state.layout,
                });
            }
            QueueState::Owned(q) => {
                //This is the case where the image was not released elsewhere. So we have to discrad. Warn, then ignore and
                //trans anyways
                #[cfg(feature = "logging")]
                if q != queue {
                    log::warn!("Image {:?} is owned by a different queue, but graph is initializing. Consider adding an explicit release pass for this image to the graph that was executed before, to allow the pass on queue_family={} to schedule a acquire operation instead.", image.image().inner, queue);
                } else {
                    #[cfg(feature = "log_reasoning")]
                    log::trace!("Init image {:?} from owned via init", image.image().inner);

                    //Is already owned. Therefore we can transition.
                    self.transitions.push(Trans::ImgInit {
                        img: image.image().clone(),
                        mask: state.access_mask,
                        layout: state.layout,
                        queue,
                    });
                }
            }
        }

        //and move image to this state
        *image.state.write().unwrap() = state.clone();
        *image.queue.write().unwrap() = QueueState::Owned(queue);
    }

    ///Initializes buffer ignoring data that might be written to it.
    pub fn init_buffer(&mut self, buffer: &StBuffer, queue: u32, state: &BufferState) {
        //While just initing to another queue works, the validation layers don't like this.
        //we throw a warning so the user could change the graph.

        match *buffer.queue.read().unwrap() {
            QueueState::Uninitialized => {
                //Emmit simple init
                self.transitions.push(Trans::BufInit {
                    buf: buffer.buffer().clone(),
                    mask: state.access_mask,
                    queue,
                });
            }
            QueueState::Released { from, to } => {
                let before_state = buffer.state();
                //was releaed, can acquire
                self.acquire_buffer(buffer, from, to);
                //then transition to state
                self.transitions.push(Trans::BufFmt {
                    buf: buffer.buffer().clone(),
                    src_mask: before_state.access_mask,
                    dst_mask: state.access_mask,
                });
            }
            QueueState::Owned(q) => {
                //This is the case where the image was not released elsewhere. So we have to discrad. Warn, then ignore and
                //trans anyways
                #[cfg(feature = "logging")]
                if q != queue {
                    log::warn!("Buffer {:?} is owned by a different queue, but graph is initializing. Consider adding an explicit release pass for this Buffer to the graph that was executed before, to allow the pass on queue_family={} to schedule a acquire operation instead.", buffer.buffer().inner, queue);
                } else {
                    //Is already owned. Therefore we can transition.
                    self.transitions.push(Trans::BufInit {
                        buf: buffer.buffer().clone(),
                        mask: state.access_mask,
                        queue,
                    });
                }
            }
        }

        //and move image to this state
        *buffer.state.write().unwrap() = state.clone();
        *buffer.queue.write().unwrap() = QueueState::Owned(queue);
    }

    ///Adds a image acquire operation which acquires `image` for `queue`.
    pub fn acquire_image(&mut self, image: &StImage, src_queue: u32, dst_queue: u32) {
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

        debug_assert!(
            image
                .queue
                .read()
                .unwrap()
                .is_released(src_queue, dst_queue),
            "Acquired image {:?}, but queue was defined!",
            image.image().inner
        );
        image
            .queue
            .write()
            .unwrap()
            .acquire_to(src_queue, dst_queue);
    }

    pub fn release_buffer(&mut self, buffer: &StBuffer, src_queue: u32, dst_queue: u32) {
        let current_state = buffer.state.read().unwrap().clone();
        self.transitions.push(Trans::BufQueueOp {
            buf: buffer.buffer.clone(),
            dst_mask: current_state.access_mask,
            src_mask: current_state.access_mask,
            src_queue,
            dst_queue,
        });

        debug_assert!(
            buffer.queue.read().unwrap().is_owned(src_queue),
            "Released buffer = {:?} on queue = {}, but was not owned by this queue!",
            buffer.buffer().inner,
            src_queue
        );
        buffer
            .queue
            .write()
            .unwrap()
            .release_to(src_queue, dst_queue);
    }

    pub fn acquire_buffer(&mut self, buffer: &StBuffer, src_queue: u32, dst_queue: u32) {
        let current_state = buffer.state.read().unwrap().clone();
        self.transitions.push(Trans::BufQueueOp {
            buf: buffer.buffer.clone(),
            dst_mask: current_state.access_mask,
            src_mask: current_state.access_mask,
            src_queue,
            dst_queue,
        });

        debug_assert!(
            buffer
                .queue
                .read()
                .unwrap()
                .is_released(src_queue, dst_queue),
            "Acquired buffer {:?}, but queue was defined!",
            buffer.buffer().inner
        );
        buffer
            .queue
            .write()
            .unwrap()
            .acquire_to(src_queue, dst_queue);
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
                    log::warn!(
                        "Found uninitialized image {:?}, that was not catched by the graph. Moving to state {:?}",
                        image.image().inner,
                        &state
                    );

                    self.init_image(image, queue_family, state)
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
            AssumedState::Buffer { buffer, state } => {
                let buffer_state = buffer.state.read().unwrap().clone();

                if buffer.queue_family() == UNDEFINED_QUEUE {
                    #[cfg(feature = "log_reasoning")]
                    log::trace!(
                        "Found uninitialized buffer {:?}, that was not catched by the graph. Moving to state {:?}",
                        buffer.buffer().inner,
                        &state
                    );

                    self.init_buffer(buffer, queue_family, state)
                } else {
                    //Simple format transform since queue is alright
                    self.transitions.push(Trans::BufFmt {
                        buf: buffer.buffer.clone(),
                        src_mask: buffer_state.access_mask,
                        dst_mask: state.access_mask,
                    });
                }
            }
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
                        buf,
                        src_queue,
                        dst_queue,
                        dst_mask,
                        src_mask,
                    } => {
                        bufbar.push(vk::BufferMemoryBarrier {
                            buffer: buf.inner,
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

            //Emit actual transition
            if !buffer_barriers.is_empty() || !image_barriers.is_empty() {
                #[cfg(feature = "log_reasoning")]
                log::trace!(
                    "Sheduling queue transitions: \nBuffer={:#?}\nImages={:#?}!",
                    buffer_barriers,
                    image_barriers
                );
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
            ) = other.iter().fold(
                (Vec::new(), Vec::new()),
                |(mut bufbar, mut imgbar), trans| {
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
                        Trans::BufInit { buf, mask, queue } => {
                            bufbar.push(vk::BufferMemoryBarrier {
                                buffer: buf.inner,
                                src_access_mask: vk::AccessFlags::NONE,
                                dst_access_mask: *mask,
                                src_queue_family_index: *queue,
                                dst_queue_family_index: *queue,
                                offset: 0,
                                size: vk::WHOLE_SIZE,
                                ..Default::default()
                            })
                        }
                        Trans::BufFmt {
                            buf,
                            src_mask,
                            dst_mask,
                        } => bufbar.push(vk::BufferMemoryBarrier {
                            buffer: buf.inner,
                            src_access_mask: *src_mask,
                            dst_access_mask: *dst_mask,
                            offset: 0,
                            size: vk::WHOLE_SIZE,
                            ..Default::default()
                        }),
                        _ => panic!("Found non init or fmt operation in buffer/image barriers"),
                    }

                    (bufbar, imgbar)
                },
            );

            if !buffer_barriers.is_empty() || !image_barriers.is_empty() {
                #[cfg(feature = "log_reasoning")]
                log::trace!(
                    "Sheduling barriers: \nBuffer={:#?}\nImages={:#?}!",
                    buffer_barriers,
                    image_barriers
                );
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
