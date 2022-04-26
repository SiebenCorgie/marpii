//!# Push Constants
//! Push constants are small special buffers which can be updated really fast. They are best used when some small data block need to change fast.
//!
//! Push constants can be bound on a per-stage basis. However, marp currently doesn't allow offsets into the objects to be bound.

use ash::vk;

///A push constant with some data.
/// #Safety
/// the type `T` has to be aligned correctly, otherwise data read by the shader
/// might be interpreted wrong.
pub struct PushConstant<T: Sized> {
    inner_range: vk::PushConstantRange,
    stage: vk::ShaderStageFlags,
    content: T,
}

impl<T: Sized + 'static> PushConstant<T> {
    ///Creates a new Push constant from type `T`. Derives all needed data from `T`. However, data should not be borrowed,
    /// since the derived size will always have a value of 8. Instead pass the data to the function and manipulate it via `get_content_mut()`
    pub fn new(content: T, stages: vk::ShaderStageFlags) -> Self {
        let inner_range = vk::PushConstantRange::builder()
            .stage_flags(stages)
            .offset(0) //Allways 0 for now
            .size(std::mem::size_of::<T>() as u32)
            .build();

        PushConstant {
            inner_range,
            stage: stages,
            content,
        }
    }

    pub fn range(&self) -> &vk::PushConstantRange {
        &self.inner_range
    }

    pub fn get_content(&self) -> &T {
        &self.content
    }

    pub fn get_content_mut(&mut self) -> &mut T {
        &mut self.content
    }

    pub fn get_stage(&self) -> vk::ShaderStageFlags {
        self.stage
    }

    pub fn content_as_bytes<'a>(&'a self) -> &'a [u8] {
        let pointer: *const T = &self.content;
        let u_pointer: *const u8 = pointer as *const u8;
        let sli: &[u8] = unsafe { std::slice::from_raw_parts(u_pointer, std::mem::size_of::<T>()) };

        sli
    }
}
