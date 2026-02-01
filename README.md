<div align="center">

# MarpII

Second iteration of [marp](gitlab.com/tendsinmende/marp). Vulkan wrapper around the [ash](crates.io/crates/ash) crate. Focuses on stable resource creation and usability. Tries to minimized duplication between ash and itself.

[![pipeline status](https://gitlab.com/tendsinmende/marpii/badges/main/pipeline.svg)](https://gitlab.com/tendsinmende/marpii/-/commits/main)
[![dependency status](https://deps.rs/repo/gitlab/tendsinmende/marpii/status.svg)](https://deps.rs/repo/gitlab/tendsinmende/marpii)
[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/L3L3F09W2)
</div>

## Difference to Marp
Marp tries to wrap the Ash crate. The target was to create a high-level-ish API that allows faster iterations while programming without sacrificing speed of the resulting application.

This works for simple applications, like [algae's test application](https://gitlab.com/tendsinmende/algae/-/tree/main/crates/vulkan_runner) but became limiting when writing bigger applications like [nako's renderer](https://gitlab.com/tendsinmende/nako/-/tree/main/crates/nakorender).

More sophisticated applications sometimes need to create more complex systems that need access to Vulkan's low level primitives. This is where MarpII shines. It provides helpful helpers that can, but don't have to be used.

The main [marpii](crates/marpii) crate provides helper function for the most common Vulkan objects like pipelines, images, buffers etc. It manages lifetimes of objects that are created through the device. This usually happens "on drop" of those resources. Additionally, some implicit lifetime tracking (for instance command-pools must outlive the command buffer created from those pools) are implemented by keeping a reference to the pool until the command buffer is dropped.

## Defaults and opinionated design

MarpII has some design decisions that are opinionated. For instance, where ever it matters the target Vulkan version will be the latest stable major release. As of writing (march 2022) this is 1.3. It also uses `Arc<T>` to keep objects alive. The added safety/convenience is payed by some overhead.

## Getting started

### Library usage

Usage of the library is as usual by including the crate in your `Cargo.toml`. We don't (yet) publish to crates-io, so you'll need a git-dependency. See [Development and versions](#Development-and-versions) for further information.
Examples can be found in the `examples` directory; marpii is also documented. A simple `cargo doc --open` should provide you with the necessary documentation.


### Development and versions

The `stable` branch tracks the `1.x` version of `marpii` and its companion crates. If you want to develop some kind of user-facing application, that's the correct way to go. Include it as a dependency via:

```toml
marpii = {git = "https://gitlab.com/tendsinmende/marpii.git", branch = "stable"}
```

`main` is the development branch. At the moment it is preparing a 2.0 release that should fix rough corners around RMG and introduce some modernization, particularly ray-tracing support and helpers, better bindless, a new frame-graph scheduler, and support for Android. To use that branch, include marpii via:

```toml
marpii = {git = "https://gitlab.com/tendsinmende/marpii.git", branch = "main"}
```

### Helpers

Apart from the main crate that is closely related to Vulkan multiple helper crates exist that should make working with Vulkan easier. Have a look at their READMEs for a description on what they do and how experimental they are.

- marpii-commands: CommandBuffer helper that captures resources that are needed for the execution of the command buffer.
- marpii-rmg: Frame-graph helper. Allows defining multiple sub `Task`s for a frame. Takes care of resources (Buffers/Images), layout and access transitions, pipeline barriers, inter-queue synchronisation etc. You basically only have to register which resources are used for a task, and how the draw/dispatch is done. There are also some helpers to make common tasks (compute shaders, vertex+fragment shaders and data-transfer) as specially friction less.
- marpii-rmg-shared: `no_std` crate that defines the resource handles used by RMG's bindless setup. Can be used in rust-gpu based shaders for convenient access. There is also a `shared.glsl` file for compatiblity with GLSL based shaders and RMG.
- marpii-descriptor: Multiple `DescriptorSet` helpers. Similar to the command-buffer helper resources are captured to keep the descriptor sets valid. Also handles descriptor allocation and freeing for you.

### Examples

Examples are executed via `cargo run --bin example_name`. Have a look at `examples/` for available applications.

## Dependencies
A list of dependencies used in the crates of this project can be found in [dependencies](dependencies.md). Take a look at the `Cargo.toml` of each crate for further information about features and versions.

We generally try to have the minimum amount of dependencies possible and try to stick to _big_ names as much as possible.


## Contributing

You are welcome to contribute. All contributions are licensed under the MPL v2.0.

## License

The whole project is licensed under MPL v2.0, all contributions will be licensed the same. Have a look at Mozilla's [FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/) to see if this fits your use-case.
