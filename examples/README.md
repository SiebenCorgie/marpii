

# MarpII examples

- hello_triangle: simple compute shader dispatch via MarpII and MarpII-Commands. Make sure to execute the shader builder before running!
- rmg_rendering: example using RMG to schedule a forward rendered cloud of objects with async-compute pass. Utilising transfer/compute and graphics queues if available. Execute via `cargo run --bin rmg_rendering -- /path/to/file.gltf`. Make sure to execute the shader builder before running!

- shared: shared definitions (glsl and rust) between shaders and CPU side code
- rmg_shader: shader for the rmg example
- rust_shader: shader for the hello_triangle example
- shader_builder: builds all example shaders. You need to have `glslangValidator` in your `$PATH`.
