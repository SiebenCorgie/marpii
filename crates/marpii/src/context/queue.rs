///Abstract queue that collects a [ash::vk::Queue](ash::vk::Queue) and its family.
#[derive(Clone, Debug)]
pub struct Queue {
    pub inner: ash::vk::Queue,
    pub family_index: u32,
    pub properties: ash::vk::QueueFamilyProperties,
}

pub struct QueueBuilder {
    ///The family's index.
    pub family_index: u32,
    ///its properties
    pub properties: ash::vk::QueueFamilyProperties,
    ///The length of this vector determins how many instances of this queue are created. The number determins the
    /// priority of each queue on the hardware. See the [documentation](https://www.khronos.org/registry/vulkan/specs/1.3-extensions/man/html/VkDeviceQueueCreateInfo.html) for more information about this topic.
    pub priorities: Vec<f32>,
}

impl QueueBuilder {
    ///Sets the queue ammount that is being created (length of the vector) and each queues priority. Have a look at the
    /// `priorities` field documentation.
    ///
    /// Note that only the first `n` priorities are resprected if the length of the vector exceeds `n = self.properties.queue_count`.
    pub fn with_queues(&mut self, mut queue_priorities: Vec<f32>) {
        if queue_priorities.len() > self.properties.queue_count as usize {
            queue_priorities.resize(self.properties.queue_count as usize, 0.0);
        }

        self.priorities = queue_priorities;
    }

    pub fn as_create_info<'a>(&'a self) -> ash::vk::DeviceQueueCreateInfoBuilder<'a> {
        ash::vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(self.family_index)
            .queue_priorities(&self.priorities)
    }
}
