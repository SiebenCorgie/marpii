use std::sync::Arc;

use ash::vk;

use crate::{context::Device, DeviceError, MarpiiError};

use super::QueryPool;

///Single 64bit timestamp. Note that the timestamp value alone does not necessarily stands for a nanosecond measurement.
/// Instead this is the amount of `physical_device_limits.timestampPeriod` increments.
pub type Timestamp = u64;

/// Timestamp querypool abstraction for easier data retrieval.
pub struct Timestamps {
    pool: QueryPool,
    in_flight: u32,
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
        if vk12.host_query_reset == 0 {
            return Err(MarpiiError::DeviceError(
                crate::DeviceError::UnsupportedFeature("VK_EXT_host_query_reset".to_owned()),
            ));
        }

        //get physical device feature
        let pdf = device.get_feature::<vk::PhysicalDeviceHostQueryResetFeatures>();
        if pdf.host_query_reset == 0 {
            return Err(MarpiiError::DeviceError(
                crate::DeviceError::UnsupportedFeature(
                    "PhysicalDeviceHostQueryResetFeatures.host_query_reset".to_owned(),
                ),
            ));
        }

        let pool = QueryPool::new(device, timestamp_count as u32, vk::QueryType::TIMESTAMP)
            .map_err(|e| MarpiiError::DeviceError(DeviceError::VkError(e)))?;

        Ok(Timestamps {
            pool,
            //NOTE: We use the availability method where the result at `index` is signaled via a boolean value at `index+1`.
            data_poins: vec![0; timestamp_count * 2],
            in_flight: 0,
            async_data_points: vec![None; timestamp_count],
        })
    }

    ///Tells the `command_buffer` to write the timestamp value at the `stage` to the `timestamp` (index).
    ///
    /// # Note
    ///
    /// This will do nothing if `timestamp` exceeds the `timestamp_count` used when creating this pool.
    pub fn write_timestamp(
        &mut self,
        command_buffer: &vk::CommandBuffer,
        stage: vk::PipelineStageFlags2,
        timestamp: u32,
    ) {
        if timestamp >= (self.data_poins.len() as u32 / 2) {
            #[cfg(feature = "logging")]
            log::error!(
                "Timestamp {} exceeds pool size {}",
                timestamp,
                self.pool.size
            );
            return;
        }
        self.in_flight += 1;
        unsafe {
            self.pool.device.inner.cmd_write_timestamp2(
                *command_buffer,
                stage,
                self.pool.pool,
                timestamp,
            )
        };
    }

    pub fn reset(&mut self, command_buffer: &vk::CommandBuffer) -> Result<(), vk::Result> {
        self.in_flight = 0;
        self.pool.reset(command_buffer)
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
            &mut self.data_poins[0..(self.in_flight as usize)],
            vk::QueryResultFlags::WAIT,
        )?;
        Ok(&self.data_poins[0..(self.in_flight as usize)])
    }

    ///Returns all times stamps by immediately. Timestamps that where not yet available are `None`.
    ///
    /// # Note
    ///
    /// You might want to preffer this over the blocking alternative in a realtime scenario.
    pub fn get_timestamps(&mut self) -> Result<&[Option<Timestamp>], vk::Result> {
        //null before using
        self.data_poins.fill(0);

        //Use tuple as referenced here: https://github.com/ash-rs/ash/issues/100#issuecomment-1530041456
        // NOTE: This might break at some point!

        let target_slice: &mut [[u64; 2]] = bytemuck::cast_slice_mut(self.data_poins.as_mut());
        self.pool.query_results(
            &mut target_slice[0..(self.in_flight as usize)],
            vk::QueryResultFlags::WITH_AVAILABILITY | vk::QueryResultFlags::TYPE_64,
        )?;

        //sort out availability
        for (idx, dta) in self.data_poins[0..(self.in_flight as usize * 2)]
            .chunks_exact(2)
            .enumerate()
        {
            if dta[1] > 0 {
                self.async_data_points[idx] = Some(dta[0]);
            } else {
                self.async_data_points[idx] = None;
            }
        }

        Ok(&self.async_data_points[0..self.in_flight as usize])
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
