//! Iced _Integration-Example_ based test, that renders a Iced GUI via MarpII.
//!
//! It uses the marpii-gpu glue to generate a wgpu-context, that is used by the standard Iced renderer.
//! In parallel a simple compute shader renders an image using standard MarpII. Finally both images are combined
//! before rendering to a surface.

mod ctrl;

use iced_wgpu::graphics::Viewport;
use iced_wgpu::{wgpu, Engine, Renderer};
use iced_winit::conversion;
use iced_winit::core::mouse;
use iced_winit::core::renderer;
use iced_winit::core::{Color, Font, Pixels, Size, Theme};
use iced_winit::futures;
use iced_winit::runtime::program;
use iced_winit::runtime::Debug;
use iced_winit::winit;
use iced_winit::Clipboard;

use marpii::context::Ctx;
use marpii::gpu_allocator::vulkan::Allocator;
use marpii_wgpu::WgpuCtx;
use winit::{
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    keyboard::ModifiersState,
};

use std::sync::Arc;

pub fn clear<'a>(
    target: &'a wgpu::TextureView,
    encoder: &'a mut wgpu::CommandEncoder,
    background_color: Color,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear({
                    let [r, g, b, a] = background_color.into_linear();

                    wgpu::Color {
                        r: r as f64,
                        g: g as f64,
                        b: b as f64,
                        a: a as f64,
                    }
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    })
}

pub fn main() -> Result<(), winit::error::EventLoopError> {
    tracing_subscriber::fmt::init();

    // Initialize winit
    let event_loop = EventLoop::new()?;

    let mut runner = Runner::Loading;
    event_loop.run_app(&mut runner)
}

#[allow(clippy::large_enum_variant)]
enum Runner {
    Loading,
    Ready {
        window: Arc<winit::window::Window>,
        #[allow(dead_code)]
        marpii_ctx: Ctx<Allocator>,
        wgpu_ctx: WgpuCtx,
        surface: wgpu::Surface<'static>,
        format: wgpu::TextureFormat,
        engine: Engine,
        renderer: Renderer,
        state: program::State<ctrl::Controls>,
        cursor_position: Option<winit::dpi::PhysicalPosition<f64>>,
        clipboard: Clipboard,
        viewport: Viewport,
        modifiers: ModifiersState,
        resized: bool,
        debug: Debug,
    },
}

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Self::Loading = self {
            let window = Arc::new(
                event_loop
                    .create_window(winit::window::WindowAttributes::default())
                    .expect("Create window"),
            );

            let physical_size = window.inner_size();
            let viewport = Viewport::with_physical_size(
                Size::new(physical_size.width, physical_size.height),
                window.scale_factor(),
            );
            let clipboard = Clipboard::connect(window.clone());

            let (marpii_ctx, _) = marpii::context::Ctx::custom_context(Some(&window), true, |db| {
                db.with_extensions(marpii::ash::khr::buffer_device_address::NAME)
                    .with_feature(
                        marpii::ash::vk::PhysicalDeviceBufferDeviceAddressFeatures::default()
                            .buffer_device_address(true),
                    )
                    .with_feature(
                        marpii::ash::vk::PhysicalDeviceSubgroupSizeControlFeatures::default()
                            .subgroup_size_control(true),
                    )
            })
            .unwrap();
            let wgpu_ctx = marpii_wgpu::WgpuCtx::new(&marpii_ctx).expect("Failde to init Wgpu Ctx");

            let surface = wgpu_ctx
                .instance()
                .create_surface(window.clone())
                .expect("Create window surface");

            let format = futures::futures::executor::block_on(async {
                let adapter = wgpu::util::initialize_adapter_from_env_or_default(
                    wgpu_ctx.instance(),
                    Some(&surface),
                )
                .await
                .expect("Create adapter");

                let capabilities = surface.get_capabilities(&adapter);

                capabilities
                    .formats
                    .iter()
                    .copied()
                    .find(wgpu::TextureFormat::is_srgb)
                    .or_else(|| capabilities.formats.first().copied())
                    .expect("Get preferred format")
            });

            surface.configure(
                wgpu_ctx.device(),
                &wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format,
                    width: physical_size.width,
                    height: physical_size.height,
                    present_mode: wgpu::PresentMode::AutoVsync,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: vec![],
                    desired_maximum_frame_latency: 2,
                },
            );

            //init gui
            let controls = ctrl::Controls::new();

            // Initialize iced
            let mut debug = Debug::new();
            let engine = Engine::new(
                wgpu_ctx.adapter(),
                wgpu_ctx.device(),
                wgpu_ctx.queue(),
                format,
                None,
            );
            let mut renderer = Renderer::new(
                &wgpu_ctx.device(),
                &engine,
                Font::default(),
                Pixels::from(16),
            );

            let state =
                program::State::new(controls, viewport.logical_size(), &mut renderer, &mut debug);

            // You should change this if you want to render continuously
            event_loop.set_control_flow(ControlFlow::Wait);

            *self = Self::Ready {
                window,
                marpii_ctx,
                wgpu_ctx,
                surface,
                format,
                engine,
                renderer,
                state,
                cursor_position: None,
                modifiers: ModifiersState::default(),
                clipboard,
                viewport,
                resized: false,
                debug,
            };
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Self::Ready {
            window,
            marpii_ctx: _,
            wgpu_ctx,
            surface,
            format,
            engine,
            renderer,
            state,
            viewport,
            cursor_position,
            modifiers,
            clipboard,
            resized,
            debug,
        } = self
        else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                if *resized {
                    let size = window.inner_size();

                    *viewport = Viewport::with_physical_size(
                        Size::new(size.width, size.height),
                        window.scale_factor(),
                    );

                    surface.configure(
                        wgpu_ctx.device(),
                        &wgpu::SurfaceConfiguration {
                            format: *format,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            width: size.width,
                            height: size.height,
                            present_mode: wgpu::PresentMode::AutoVsync,
                            alpha_mode: wgpu::CompositeAlphaMode::Auto,
                            view_formats: vec![],
                            desired_maximum_frame_latency: 2,
                        },
                    );

                    *resized = false;
                }

                match surface.get_current_texture() {
                    Ok(frame) => {
                        let mut encoder = wgpu_ctx.device().create_command_encoder(
                            &wgpu::CommandEncoderDescriptor { label: None },
                        );

                        let program = state.program();

                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());

                        //clear frame
                        {
                            // We clear the frame
                            let _render_pass =
                                clear(&view, &mut encoder, program.background_color());

                            // Draw the scene
                            //scene.draw(&mut render_pass);
                        }

                        // And then iced on top
                        renderer.present(
                            engine,
                            wgpu_ctx.device(),
                            wgpu_ctx.queue(),
                            &mut encoder,
                            None,
                            frame.texture.format(),
                            &view,
                            viewport,
                            &debug.overlay(),
                        );

                        // Then we submit the work
                        engine.submit(wgpu_ctx.queue(), encoder);
                        frame.present();

                        // Update the mouse cursor
                        window.set_cursor(iced_winit::conversion::mouse_interaction(
                            state.mouse_interaction(),
                        ));
                    }
                    Err(error) => match error {
                        wgpu::SurfaceError::OutOfMemory => {
                            panic!(
                                "Swapchain error: {error}. \
                            Rendering cannot continue."
                            )
                        }
                        _ => {
                            // Try rendering again next frame.
                            window.request_redraw();
                        }
                    },
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                *cursor_position = Some(position);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                *modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(_) => {
                *resized = true;
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }

        // Map window event to iced event
        if let Some(event) =
            iced_winit::conversion::window_event(event, window.scale_factor(), *modifiers)
        {
            state.queue_event(event);
        }

        // If there are events pending
        if !state.is_queue_empty() {
            // We update iced
            let _ = state.update(
                viewport.logical_size(),
                cursor_position
                    .map(|p| conversion::cursor_position(p, viewport.scale_factor()))
                    .map(mouse::Cursor::Available)
                    .unwrap_or(mouse::Cursor::Unavailable),
                renderer,
                &Theme::Dark,
                &renderer::Style {
                    text_color: Color::WHITE,
                },
                clipboard,
                debug,
            );

            // and request a redraw
            window.request_redraw();
        }
    }
}
