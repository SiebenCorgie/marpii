<div align="center">

# MarpII

Second iteration of [marp](gitlab.com/tendsinmende/marp). Vulkan wrapper around the [ash](crates.io/crates/ash) crate. Focuses on stable resource creation and usability. Tries to minimised duplication between ash and itself.

[![pipeline status](https://gitlab.com/tendsinmende/marpii/badges/main/pipeline.svg)](https://gitlab.com/tendsinmende/marpii/-/commits/main)

</div>

## Difference to Marp
Marp tries to wrap the Ash crate completely. The target was to create a high-level-ish API that allows faster iterations while programming without sacrificing speed of the resulting application.

This works for simple applications, like [algae's test application](https://gitlab.com/tendsinmende/algae/-/tree/main/crates/vulkan_runner) but became limiting when writing bigger applications like [nako's renderer](https://gitlab.com/tendsinmende/nako/-/tree/main/crates/nakorender).

More sophisticated applications sometimes need to create more complex systems that need access to Vulkan's low level primitives. This is where MarpII shines. It provides helpful helpers that can, but must not be used.

The main [marpii](crates/marpii) crate provides helper function for the most common vulkan objects like pipelines, images, buffers etc. If manages lifetimes of objects that are created through the device. This usually happens "on drop" of those resources. Additionally, some implicit lifetime tracking (for instance command-pools must outlive the command buffer created from those pools) are implemented by keeping a reference to the pool until the command buffer is dropped.

On top of those low-level-ish helpers higher level helpers are implemented. Most notably:


- marpii-descriptor: Self growing descriptor pool with descriptor sets that keep their bound resources alive until they are dropped.
- marpii-command: Command-buffer helper that keeps resources alive until the command buffer is not in use anymore. 
- marpii-command-graph: Provides highlevel "pass" abstraction and resource state tracking. Allows easy combination of graphics/renderpasses for rapid rendering prototyping.
- marpii-rmg: Similar to marpii-command-graph but handles resource creation and usage, at the cost of stricter usage requirements. 

## Defaults and opinionated design

MarpII has some design decisions that are opinionated. For instance, where ever it matters the target vulkan version will be the latest stable major release. As of writing (march 2022) this is 1.3. 

## Getting started

### Library usage

Usage of the library is as usual by including the crate in your `Cargo.toml`.
Examples can be found in the `examples` directory, marpii is also documented. A simple `cargo doc --open` should provide you with the necessary documentation.

### Helpers

Apart from the main crate that is closely related to Vulkan multiple helper crates exist that should make working with vulkan easier. Have a look at their READMEs for a description on what they do and how experimental they are.

- marpii-commands: CommandBuffer helper that captures resources that are needed for the execution of the command buffer.
- marpii-command-graph: Experimental frame-graph helper. Allows defining multiple abstract `Pass`es which are build from `SubPass`es. inter-pass dependencies are found and resource transition is taken care of. This basically allows the user to create complex frame graphs easily. 
- marpii-descriptor: Multiple `DescriptorSet` helpers. Similar to the command-buffer helper resources are captured to keep the descriptor sets valid. Also handles descriptor allocation and freeing for you.

### Examples

Examples are executed via `cargo run --bin example_name`. Have a look at `examples/` for available applications.


## Dependencies
A list of dependencies used in the crates of this project. Have a look at the `Cargo.toml` of each crate for further information about features and versions.

### MarpII

- ash: general vulkan API bindings
- ash-window: convenient abstraction over window handles. Allows for a generic implementation of `Surface` without having to handle multiple window crates.
- raw-window-handle: used to be able to expose the window handle needed for `Surface`
- anyhow: convenient error handling. Otherwise we'd have either one really big error ENum, or multiple small ones. Either way, there are a lot of different error states that are not really recoverable. So a simple string like error is enough in most cases.
- const-cstr: Allows defining constant CStrings. They are used for default messages in the debug callback. 
- small-vec: Whenever only small collections are needed this allows us to uses arrays in the general case and Vecs if those are not big enough.
- fxhash: in the cases where we need a hash map/set fx-hash is used for speed.
- gpu-allocator: standard Vulkan memory allocator
- log: logging if enabled
- puffin: profiling if enabled
- rspirv-reflect: spirv-module reflection if enabled. Allows convenient descriptor-layout creation.

### MarpII-Command-Graph

- marpii/marpii-commands: marpii binding
- anyhow: convenient error handling.
- fxhash: in the cases where we need a hash map/set fx-hash is used for speed.
- slotmap: Fast and safe Vec-like access for `Passe`s within a Graph.
- log: logging if enabled
- winit: Swapchain handling

### MarpII-mrg

- marpii/marpii-commands: marpii binding
- anyhow: convenient error handling.
- thiserror: For specific errors.
- fxhash: in the cases where we need a hash map/set fx-hash is used for speed.
- slotmap: Fast and safe Vec-like access for `Passe`s and resources within a Graph.
- log: logging if enabled
- winit: Swapchain handling

### MarpII-Commands
- marpii: marpii binding
- anyhow: convenient error handling.
- log: logging if enabled

### MarpII-Descriptor

- marpii: marpii binding
- anyhow: convenient error handling.
- fxhash: in the cases where we need a hash map/set fx-hash is used for speed.
- log: logging if enabled

## Contributing

You are welcome to contribute. All contributions are licensed under the MPL v2.0.

Note that the project is currently in its early stages. Actual contribution might be difficult.

## License

The whole project is licensed under MPL v2.0, all contributions will be licensed the same. Have a look at Mozilla's [FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/) to see if this fits your use-case.
