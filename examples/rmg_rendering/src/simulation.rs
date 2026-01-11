use marpii::{ash::vk, resources::ImgDesc};
use marpii_rmg::{
    helper::{computepass::GenericComputePass, BufferUsage, ImageUsage},
    BufferHandle, ImageHandle, Rmg, RmgError,
};
use shared::SimObj;

use crate::OBJECT_COUNT;
const SHADER_COMP: &[u8] = include_bytes!("../resources/simulation.spv");

pub struct Simulation {
    ///Simulation buffer
    pub sim_buffer: BufferHandle<SimObj>,
    pub feedback_image: ImageHandle,
    is_init: bool,
    pass: GenericComputePass<shared::SimPush>,
}

impl Simulation {
    pub const SUBGROUP_COUNT: u32 = 64;

    fn dispatch_count() -> u32 {
        ((OBJECT_COUNT as f32) / Self::SUBGROUP_COUNT as f32).ceil() as u32
    }

    pub fn new(rmg: &mut Rmg) -> Result<Self, RmgError> {
        let pipeline = rmg.compute_pipeline("main", SHADER_COMP).unwrap();

        let feedback_image = rmg.new_image_uninitialized(
            ImgDesc::storage_image_2d(64, 64, vk::Format::R8G8B8A8_UNORM),
            Some("Feedback image"),
        )?;
        let sim_buffer = rmg.new_buffer::<SimObj>(OBJECT_COUNT, Some("SimBuffer 1"))?;

        let pass = rmg
            .new_compute_pass(pipeline.clone())
            //Setup the push constant
            .with_push_constant(|rmg| shared::SimPush {
                sim_buffer: rmg.resource_handle(sim_buffer.clone()).unwrap(),
                img_handle: rmg.resource_handle(feedback_image.clone()).unwrap(),
                is_init: 0,
                buf_size: OBJECT_COUNT as u32,
                img_height: 64,
                img_width: 64,
                pad: [0; 2],
            })
            .use_buffer(sim_buffer.clone(), BufferUsage::ReadWrite)
            .use_image(feedback_image.clone(), ImageUsage::StorageWrite)
            .dispatch_size([Self::dispatch_count(), 1, 1])
            .unwrap()
            .finish();

        Ok(Simulation {
            sim_buffer,
            feedback_image,
            is_init: false,
            pass,
        })
    }

    pub fn compute_pass(&mut self, rmg: &mut Rmg) -> &mut GenericComputePass<shared::SimPush> {
        //handle whether this was called before
        let is_init = if !self.is_init {
            self.is_init = true;
            0
        } else {
            1
        };

        //update the compute pass PC
        *self.pass.push_constant_content_mut() = shared::SimPush {
            sim_buffer: rmg.resource_handle(self.sim_buffer.clone()).unwrap(),
            img_handle: rmg.resource_handle(self.feedback_image.clone()).unwrap(),
            is_init,
            buf_size: OBJECT_COUNT as u32,
            img_height: 64,
            img_width: 64,
            pad: [0; 2],
        };

        &mut self.pass
    }
}
