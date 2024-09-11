//! Example that shows how to configure a MarpII context at build-time to use a set of
//! Vulkan features, iff they are present.
//!
//! Using that we can for instance configure advanced rendering features or compute features based
//! on the hardware we are running on. A common example would be ray-tracing capabilities.

use marpii::{self, ash::vk::PhysicalDeviceDynamicRenderingFeatures};

fn main() {
    simple_logger::SimpleLogger::new().init().unwrap();

    let ev = winit::event_loop::EventLoop::new().unwrap();
    let window_attributes =
        winit::window::Window::default_attributes().with_title("hello triangle");
    #[allow(deprecated)]
    let window = ev.create_window(window_attributes).unwrap();

    let _ctx = marpii::context::Ctx::custom_context(Some(&window), true, |mut builder| {
        //The simplest thing we might want to check is if a extension is supported.
        let dynamic = builder
            .instance
            .get_feature::<marpii::ash::vk::PhysicalDeviceDynamicRenderingFeatures>(
                &builder.physical_device,
            );
        if dynamic.dynamic_rendering > 0 {
            println!("Dynamic rendering is supported!");
        } else {
            println!("DynamicRendering not supported!");
        }

        //Bigger extensions might also declare custom limits, for instance all things related to accelleration structures.
        let accel = builder
            .instance
            .get_feature::<marpii::ash::vk::PhysicalDeviceAccelerationStructureFeaturesKHR>(
                &builder.physical_device,
            );

        if accel.acceleration_structure > 0 {
            println!(
                "Acceleration structure support with following properties:\n{:#?}",
                accel
            );
        } else {
            println!("AccelerationStructure not supported!");
        }

        //For faster query you can also just check the whole Vulkan 1.3 level
        let vk13 = builder
            .instance
            .get_feature::<marpii::ash::vk::PhysicalDeviceVulkan13Features>(
                &builder.physical_device,
            );
        println!("Vulkan 1.3 feature support: {:#?}", vk13);

        //Finally depending on the querried results, you might want to push your feature set.
        // In this case we enable just the dynamic rendering feature
        if dynamic.dynamic_rendering > 0 {
            builder = builder.with_feature(
                PhysicalDeviceDynamicRenderingFeatures::default().dynamic_rendering(true),
            );
        }

        //Similar to the feature query, you can also a property query if you need further information
        // regarding the GPU's properties.
        println!(
            "Test Property: {:#?}",
            builder
                .instance
                .get_property::<marpii::ash::vk::PhysicalDeviceVulkan13Properties>(
                    &builder.physical_device
                )
        );

        builder
    })
    .unwrap();
}
