//! Small example that injects a marpii device into a wgpu adapter.

use marpii_wgpu::{instance_builder_for_wgpu, wgpu_desired_extensions, wgpu_device, wgpu_instance};

/// This example shows how to describe the adapter in use.
async fn run() {
    let mut instance_builder = marpii::context::Instance::load().unwrap();
    instance_builder = instance_builder_for_wgpu(instance_builder).unwrap();
    let instance = instance_builder.build().unwrap();
    let marpii_context = marpii::context::Ctx::new_default_from_instance(instance, None).unwrap();

    #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
    let (adapter, device, queue) = {
        let wgpu_instance =
            wgpu_instance(&marpii_context.instance).expect("Failed to create wgpu instance!");

        #[cfg(not(target_arch = "wasm32"))]
        {
            log::info!("Available adapters:");
            for a in wgpu_instance.enumerate_adapters(wgpu::Backends::all()) {
                log::info!("    {:?}", a.get_info())
            }
        }

        let queue = marpii_context
            .device
            .first_queue_for_attribute(true, false, false)
            .unwrap();
        wgpu_device(&wgpu_instance, &marpii_context.device, &queue).unwrap()
    };

    log::info!("Selected adapter: {:?}", adapter.get_info());
    log::info!("Teddy: \n{:?}", device.features())
}

pub fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::builder()
            .filter(Some(module_path!()), log::LevelFilter::Info)
            .parse_default_env()
            .init();
        pollster::block_on(run());
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run());
    }
}
