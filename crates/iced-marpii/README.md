# Iced-Marpii

A Iced renderer based on MarpII.


## Usage

Import the `iced-marpii` crate into your application. Then change the `Renderer` type of you application's `impl View` methode:

```rust
type MElement<'a, M> = Element<'a, M, Theme, iced_marpii::Renderer>;
//..
fn view(&self) -> MElement<Message>{
    //.. your app's view implementation
}
```

Have a look at the `styling` and `iced-counter` examples. If you want to use the `marpii-rmg` framework in you app, also have a look at `examples/custom-rmg-widget`.

### Development

- `shaders/` contains the shader crate and checked-in SPIR-V code, that is used by the crate.
- To rebuild the shaders, change into `shaders/shader-builder` and call `cargo build`. This might take some minutes if you do this the first time.
- To use Vulkan validation layers, set `RMG_VALIDATE=1` before launching. Most end-user don't have layers installed, which is why this is disabled by default.

## License

The whole project is licensed under MPL v2.0, all contributions will be licensed the same. Have a look at Mozilla's [FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/) to see if this fits your use-case.
