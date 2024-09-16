//! Small example that injects a marpii device into a wgpu adapter.

use marpii_wgpu::{instance_builder_for_wgpu, WgpuCtx};

/// This example shows how to describe the adapter in use.
async fn run() {
    let mut instance_builder = marpii::context::Instance::load().unwrap();
    instance_builder = instance_builder_for_wgpu(instance_builder).unwrap();
    let instance = instance_builder.build().unwrap();
    let marpii_context = marpii::context::Ctx::new_default_from_instance(instance, None).unwrap();

    let wgpu_context = WgpuCtx::new(&marpii_context).unwrap();
    log::info!("Selected adapter: {:?}", wgpu_context.adapter().get_info());
    log::info!("Teddy: \n{:?}", wgpu_context.device().features())
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
