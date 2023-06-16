use std::sync::Arc;

use ash::vk;

use crate::context::Device;

///Abstract query pool implementation. Have a look at one of the specialisations
/// like [Timestamps].
///
/// In generally there are [multiple](https://registry.khronos.org/vulkan/specs/1.3-extensions/html/vkspec.html#queries)
/// types of queries, including performance queries and occlusion queries.
pub struct QueryPool {
    pub pool: vk::QueryPool,
    pub device: Arc<Device>,
    //NOTE hiding since changing that would make the struct invalid
    pub(crate) size: u32,
}

impl QueryPool {
    pub fn new(device: &Arc<Device>, size: u32, ty: vk::QueryType) -> Result<Self, vk::Result> {
        let create_info = vk::QueryPoolCreateInfo::builder()
            .query_type(ty)
            .query_count(size);
        let pool = unsafe { device.inner.create_query_pool(&create_info, None)? };

        Ok(QueryPool {
            pool,
            device: device.clone(),
            size,
        })
    }
    ///Resets the timestamp pool of `self`.
    pub fn reset(&mut self, command_buffer: &vk::CommandBuffer) -> Result<(), vk::Result> {
        unsafe {
            self.device
                .inner
                .cmd_reset_query_pool(*command_buffer, self.pool, 0, self.size);
        }

        Ok(())
    }

    ///Reads back the results in 32bit format.
    ///
    /// # Note
    ///
    /// This operation might block if your flags contain the `QUERY_RESULT_WAIT_BIT`
    pub fn query_results_u32(
        &self,
        dst: &mut [u32],
        flags: vk::QueryResultFlags,
    ) -> Result<(), vk::Result> {
        if dst.len() == 0 {
            return Ok(());
        }
        assert!(
            !flags.contains(vk::QueryResultFlags::TYPE_64),
            "query_results_u32 can not contain 64bit flag!"
        );

        unsafe {
            self.device
                .inner
                .get_query_pool_results(self.pool, 0, dst.len() as u32, dst, flags)
        }
    }

    ///Reads back the results in 32bit format.
    ///
    /// # Note
    ///
    /// This operation might block if your flags contain the `QUERY_RESULT_WAIT_BIT`
    pub fn query_results_u64(
        &self,
        dst: &mut [u64],
        flags: vk::QueryResultFlags,
    ) -> Result<(), vk::Result> {
        if dst.len() == 0 {
            return Ok(());
        }

        let flags = flags | vk::QueryResultFlags::TYPE_64;
        unsafe {
            self.device
                .inner
                .get_query_pool_results(self.pool, 0, dst.len() as u32, dst, flags)
        }
    }
}

impl Drop for QueryPool {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_query_pool(self.pool, None);
        }
    }
}
