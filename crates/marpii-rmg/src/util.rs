use crate::{ImageHandle, Resources};
use marpii::ash::vk;
use marpii::context::Device;
use std::sync::Arc;

#[derive(Debug)]
struct StateCache {
    image: ImageHandle,
    old_access: vk::AccessFlags2,
    new_access: vk::AccessFlags2,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
}

///Helper that transitions a Image to a new state (access mask, layout) temporarly from its current state, and back. Always works on the whole
/// pipeline stage.
//TODO: remove, once better sync for the registry mechanism is implemented.
pub struct TempLayoutChange<const N: usize> {
    cache: [StateCache; N],
}

impl<const N: usize> TempLayoutChange<N> {
    pub fn to_state(
        resources: &Resources,
        device: &Arc<Device>,
        command_buffer: &vk::CommandBuffer,
        temp_states: [(ImageHandle, vk::AccessFlags2, vk::ImageLayout); N],
    ) -> Self {
        //setup state cache for each
        let state_cache: [StateCache; N] = temp_states
            .into_iter()
            .map(|(img, new_access, new_layout)| {
                let old_state = {
                    let img_access = resources.get_image_state(&img);

                    (img_access.mask, img_access.layout)
                };

                StateCache {
                    image: img,
                    new_access,
                    new_layout,
                    old_access: old_state.0,
                    old_layout: old_state.1,
                }
            })
            .collect::<Vec<_>>()
            .try_into()
            .expect("Failed to collect state change chache"); //shouldn't happen

        let mut barriers = [vk::ImageMemoryBarrier2::default(); N];
        for (idx, state) in state_cache.iter().enumerate() {
            barriers[idx] = vk::ImageMemoryBarrier2::builder()
                .image(state.image.imgref.inner)
                .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .src_access_mask(state.old_access)
                .dst_access_mask(state.new_access)
                .subresource_range(state.image.imgref.subresource_all())
                .old_layout(state.old_layout)
                .new_layout(state.new_layout)
                .build();
        }

        //Schedule transitions
        unsafe {
            device.inner.cmd_pipeline_barrier2(
                *command_buffer,
                &vk::DependencyInfo::builder().image_memory_barriers(&barriers),
            );
        }
        TempLayoutChange { cache: state_cache }
    }

    ///Reverts the image back to the old state
    pub fn revert(
        self,
        device: &Arc<Device>,
        command_buffer: &vk::CommandBuffer,
    ) {
        //create reversing barrierers and schedule

        let mut barriers = [vk::ImageMemoryBarrier2::default(); N];

        for (idx, state) in self.cache.iter().enumerate() {
            barriers[idx] = vk::ImageMemoryBarrier2::builder()
                .image(state.image.imgref.inner)
                .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .src_access_mask(state.old_access)
                .dst_access_mask(state.new_access)
                .subresource_range(state.image.imgref.subresource_all())
                .old_layout(state.old_layout)
                .new_layout(state.new_layout)
                .build();
        }

        unsafe{
            device.inner.cmd_pipeline_barrier2(
                *command_buffer,
                &vk::DependencyInfo::builder().image_memory_barriers(&barriers),
            );
        }
    }
}
