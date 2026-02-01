### MarpII

- ash: general vulkan API bindings
- ash-window: convenient abstraction over window handles. Allows for a generic implementation of `Surface` without having to handle multiple window crates.
- raw-window-handle: used to be able to expose the window handle needed for `Surface`
- thiserror: convenient error handling.
- const-cstr: Allows defining constant CStrings. They are used for default messages in the debug callback.
- small-vec: Whenever only small collections are needed this allows us to uses arrays in the general case and Vecs if those are not big enough.
- ahash: in the cases where we need a hash map/set ahash is used for speed.
- oos: Our own "OwnedOrShared" wrapper around values that are either `T` or `Arc<T>`.
- gpu-allocator: standard Vulkan memory allocator
- log: logging if enabled
- puffin: profiling if enabled
- rspirv-reflect: spirv-module reflection if enabled. Allows convenient descriptor-layout creation.
- bytemuck: Static checking while working with raw data

### MarpII-Rmg

- marpii/marpii-commands/marpii-descriptor: marpii binding
- anyhow: convenient error handling
- thiserror: convenient error handling
- ahash: in the cases where we need a hash map/set ahash is used for speed.
- slotmap: Fast and safe Vec-like mapping from handles to internal resource
- log: logging if enabled
- winit: swapchain handling

### MarpII-Commands
- marpii: marpii binding
- log: logging if enabled
- image: image-upload helper
- smallvec

### MarpII-Descriptor

- marpii: marpii binding
- ahash: in the cases where we need a hash map/set ahash is used for speed.
- log: logging if enabled

### Iced-Marpii

- marpii
- marpii-rmg
- iced 0.14 (core & graphics)
- lyon: for handling tesselation of _shapes_ in the geometry renderer extension
- log
- ahash
- cosmic-text: rust-only text-shaping and rendering to glyph-atlas
- etager: glyph-atlas-packing
- lru: for caching
