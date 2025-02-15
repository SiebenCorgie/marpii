# Iced-Marpii

A Iced renderer based on MarpII.

### Notes

- `shaders/` contains the shader crate and checked-in SPIR-V code, that is used by the crate.
- To rebuild the shaders, change into `shaders/shader-builder` and call `cargo build`. This might take some minutes if you do this the first time.
- To use vulkan validation layers, either set `ICED_MARPII_VALIDATE=1` or compile with the validation feature enabled.
- Have a look at the examples for anything else!
