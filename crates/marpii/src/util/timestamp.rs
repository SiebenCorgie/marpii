use std::sync::Arc;

use ash::vk;

use crate::{context::Device, DeviceError, MarpiiError};

use super::QueryPool;

///Single 64bit timestamp. Note that the timestamp value alone does not necessarily stands for a nanosecond measurement.
/// Instead this is the amount of `physical_device_limits.timestampPeriod` increments.
pub type Timestamp = u64;

/// Timestamp querypool abstraction for easier data retrieval.
pub struct Timestamps {
    pub pool: QueryPool,
    data_poins: Vec<Timestamp>,
    async_data_points: Vec<Option<Timestamp>>,
}

impl Timestamps {
    pub fn new(device: &Arc<Device>, timestamp_count: usize) -> Result<Self, MarpiiError> {
        //First, test for the feature
        if device
            .physical_device_properties
            .limits
            .timestamp_compute_and_graphics
            == 0
        {
            return Err(MarpiiError::DeviceError(
                crate::DeviceError::UnsupportedFeature(
                    "PhysicalDeviceLimits: timestamp_compute_and_graphics".to_owned(),
                ),
            ));
        }

        let vk12 = device.get_feature::<vk::PhysicalDeviceVulkan12Features>();
        if !vk12.host_query_reset > 0 {
            return Err(MarpiiError::DeviceError(
                crate::DeviceError::UnsupportedFeature("VK_EXT_host_query_reset".to_owned()),
            ));
        }

        //get physical device feature
        let pdf = device.get_feature::<vk::PhysicalDeviceHostQueryResetFeatures>();
        if !pdf.host_query_reset > 0 {
            return Err(MarpiiError::DeviceError(
                crate::DeviceError::UnsupportedFeature(
                    "PhysicalDeviceHostQueryResetFeatures.host_query_reset".to_owned(),
                ),
            ));
        }

        let create_info = vk::QueryPoolCreateInfo::builder()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(timestamp_count as u32);
        let pool = unsafe {
            device
                .inner
                .create_query_pool(&create_info, None)
                .map_err(|e| MarpiiError::DeviceError(DeviceError::VkError(e)))?
        };

        let pool = QueryPool {
            device: device.clone(),
            pool,
            size: timestamp_count as u32,
        };

        Ok(Timestamps {
            pool,
            //NOTE: We use the availability method where the result at `index` is signaled via a boolean value at `index+1`.
            data_poins: vec![0; timestamp_count * 2],
            async_data_points: vec![None; timestamp_count],
        })
    }

    ///Tells the `command_buffer` to write the timestamp value at the `stage` to the `timestamp` (index).
    ///
    /// # Note
    ///
    /// This will do nothing if `timestamp` exceeds the `timestamp_count` used when creating this pool.
    pub fn write_timestamp(
        &self,
        command_buffer: &vk::CommandBuffer,
        stage: vk::PipelineStageFlags2,
        timestamp: u32,
    ) {
        if timestamp >= self.pool.size {
            #[cfg(feature = "logging")]
            log::error!(
                "Timestamp {} exceeds pool size {}",
                timestamp,
                self.pool.size
            );
            return;
        }

        unsafe {
            self.pool.device.inner.cmd_write_timestamp2(
                *command_buffer,
                stage,
                self.pool.pool,
                timestamp,
            )
        };
    }

    ///Returns all times stamps by blocking until all are written.
    ///
    /// # Note
    ///
    /// You might want to preffer the non-blocking [alternative](Self::get_timestamps) in a realtime scenario.
    pub fn get_timestamps_blocking(&mut self) -> Result<&[Timestamp], vk::Result> {
        //null before using
        self.data_poins.fill(0);

        self.pool.query_results_u64(
            &mut self.data_poins[0..(self.pool.size as usize)],
            vk::QueryResultFlags::WAIT,
        )?;
        Ok(&self.data_poins[0..(self.pool.size as usize)])
    }

    ///Returns all times stamps by immediately. Timestamps that where not yet available are `None`.
    ///
    /// # Note
    ///
    /// You might want to preffer this over the blocking alternative in a realtime scenario.
    pub fn get_timestamps(&mut self) -> Result<&[Option<Timestamp>], vk::Result> {
        //null before using
        self.data_poins.fill(0);

        self.pool.query_results_u64(
            &mut self.data_poins,
            vk::QueryResultFlags::WITH_AVAILABILITY,
        )?;

        //sort out availability
        for (idx, dta) in self.data_poins.chunks_exact(2).enumerate() {
            if dta[1] > 0 {
                self.async_data_points[idx] = Some(dta[0]);
            } else {
                self.async_data_points[idx] = None;
            }
        }

        Ok(&self.async_data_points)
    }

    ///Returns the timestamp increments in nanosecond. This means *how much time passes (in nanoseconds) between `t=n` and `t=n+1`*.
    pub fn get_timestamp_increment(&self) -> f32 {
        self.pool
            .device
            .physical_device_properties
            .limits
            .timestamp_period
    }
}
