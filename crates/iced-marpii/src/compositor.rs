use iced_graphics::{compositor, error::Reason};
use marpii::{ash::vk, resources::ImgDesc, OoS};
use marpii_rmg::{ImageHandle, Rmg};
use marpii_rmg_tasks::SwapchainPresent;

use crate::{quad::QuadRenderer, renderer::Renderer, text::TextRenderer};

mod rendering;

pub struct Compositor {
    rmg: Rmg,
    settings: iced_graphics::Settings,

    //the color buffer we use for rendering. Note that we _blit_ to the swapchain.
    color_buffer: ImageHandle,
    //the depth buffer we use for ordering _everything_
    depth_buffer: ImageHandle,

    //quad renderer
    quads: QuadRenderer,
    //text renderer
    text: TextRenderer,
}

impl Compositor {
    pub const COLOR_USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::from_raw(
        vk::ImageUsageFlags::COLOR_ATTACHMENT.as_raw()
            | vk::ImageUsageFlags::TRANSFER_SRC.as_raw()
            | vk::ImageUsageFlags::STORAGE.as_raw(),
    );
    pub const DEPTH_USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::from_raw(
        vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT.as_raw()
            | vk::ImageUsageFlags::TRANSFER_SRC.as_raw()
            | vk::ImageUsageFlags::STORAGE.as_raw(),
    );
}

impl iced_graphics::compositor::Compositor for Compositor {
    type Renderer = Renderer;
    type Surface = SwapchainPresent;

    async fn with_backend<W: iced_graphics::compositor::Window + Clone>(
        settings: iced_graphics::Settings,
        compatible_window: W,
        backend: Option<&str>,
    ) -> Result<Self, iced_graphics::Error> {
        match backend {
            Some("marpii") | None => {}
            Some(other) => {
                return Err(iced_graphics::Error::GraphicsAdapterNotFound {
                    backend: "marpii",
                    reason: Reason::DidNotMatch {
                        preferred_backend: other.to_owned(),
                    },
                });
            }
        }

        //Init RMG
        let use_validation =
            cfg!(feature = "validation") || std::env::var("ICED_MARPII_VALIDATE").is_ok();

        let (ctx, surface) =
            marpii::context::Ctx::default_with_surface(&compatible_window, use_validation)
                .map_err(|e| iced_graphics::Error::GraphicsAdapterNotFound {
                    backend: "marpii",
                    reason: Reason::RequestFailed(format!("{e}")),
                })?;
        let mut rmg = Rmg::new(ctx).map_err(|e| iced_graphics::Error::GraphicsAdapterNotFound {
            backend: "marpii",
            reason: Reason::RequestFailed(format!("{e}")),
        })?;
        //and build swapchain handler
        let swapchain = SwapchainPresent::new(&mut rmg, surface).map_err(|e| {
            iced_graphics::Error::BackendError(format!(
                "Failed to greate present surface for window: {e}"
            ))
        })?;

        let width = swapchain.image_desc().extent.width;
        let height = swapchain.image_desc().extent.height;

        //If the swapchain is 8bit, or srgb, we use a different format
        let color_format = if marpii::util::is_srgb(swapchain.format())
            || marpii::util::byte_per_pixel(swapchain.format()).unwrap_or(1) == 1
        {
            rmg.ctx
                .device
                .select_format(
                    Self::COLOR_USAGE,
                    vk::ImageTiling::OPTIMAL,
                    &[
                        vk::Format::R16G16B16A16_SFLOAT,
                        vk::Format::R32G32B32A32_SFLOAT,
                        vk::Format::R8G8B8A8_UNORM,
                    ],
                )
                .expect("Could not select color-buffer format!")
        } else {
            swapchain.format()
        };

        let color_buffer = rmg
            .new_image_uninitialized(
                ImgDesc::color_attachment_2d(width, height, color_format)
                    .add_usage(Self::COLOR_USAGE),
                Some("color-buffer"),
            )
            .unwrap();
        let depth_format = rmg
            .ctx
            .device
            .select_format(
                Self::DEPTH_USAGE,
                vk::ImageTiling::OPTIMAL,
                &[
                    vk::Format::D16_UNORM,
                    vk::Format::D16_UNORM_S8_UINT,
                    vk::Format::D24_UNORM_S8_UINT,
                    vk::Format::D32_SFLOAT,
                    vk::Format::D32_SFLOAT_S8_UINT,
                ],
            )
            .expect("Could not select depth-buffer format!");
        let depth_buffer = rmg
            .new_image_uninitialized(
                ImgDesc::depth_attachment_2d(width, height, depth_format)
                    .add_usage(Self::DEPTH_USAGE),
                Some("depth-buffer"),
            )
            .unwrap();

        let quads = QuadRenderer::new(
            &mut rmg,
            &settings,
            color_buffer.clone(),
            depth_buffer.clone(),
        );
        let text = TextRenderer::new(
            &mut rmg,
            &settings,
            color_buffer.clone(),
            depth_buffer.clone(),
        );

        Ok(Self {
            rmg,
            color_buffer,
            depth_buffer,
            settings,
            quads,
            text,
        })
    }

    fn create_renderer(&self) -> Self::Renderer {
        Renderer::new(&self.settings)
    }

    fn present<T: AsRef<str>>(
        &mut self,
        renderer: &mut Self::Renderer,
        surface: &mut Self::Surface,
        viewport: &iced_graphics::Viewport,
        background_color: iced::Color,
        overlay: &[T],
    ) -> Result<(), iced_graphics::compositor::SurfaceError> {
        //If there is an overlay, push that into the renderer
        if overlay.len() > 0 {
            renderer.draw_overlay(overlay, viewport);
        }

        //prepare all the renderer data. This is where
        //we upload anything that is needed to the gpu.
        self.prepare(renderer, viewport);
        //this call the actual rendering passes
        self.render_to_surface(renderer, surface, viewport, background_color);
        self.end();

        Ok(())
    }

    fn create_surface<W: iced_graphics::compositor::Window + Clone>(
        &mut self,
        window: W,
        width: u32,
        height: u32,
    ) -> Self::Surface {
        let surface = marpii::surface::Surface::new(&self.rmg.ctx.instance, &window)
            .expect("Failed to create surface");

        let swapchain = SwapchainPresent::new(&mut self.rmg, OoS::new(surface))
            .expect("Could not create swapchain for surface!");
        self.notify_resize(width, height);
        swapchain
    }

    fn configure_surface(&mut self, _surface: &mut Self::Surface, width: u32, height: u32) {
        self.notify_resize(width, height);
        /*
        let surface_extent = vk::Extent2D { width, height };
        surface
            .recreate(surface_extent)
            .expect("Failed to explicitly resize swapchain!");
            */
    }

    fn screenshot<T: AsRef<str>>(
        &mut self,
        _renderer: &mut Self::Renderer,
        _surface: &mut Self::Surface,
        _viewport: &iced_graphics::Viewport,
        _background_color: iced::Color,
        _overlay: &[T],
    ) -> Vec<u8> {
        log::error!("Screenshotting not implemented!");
        Vec::with_capacity(0)
    }

    fn fetch_information(&self) -> compositor::Information {
        log::warn!("information getting not supported");

        let information = "the ol' adapter info".to_owned();

        compositor::Information {
            adapter: information,
            backend: "MarpII".to_owned(),
        }
    }
}

impl Drop for Compositor {
    fn drop(&mut self) {
        //wait for everything to finish, otherwise we get a segfault from
        //vulkan 👀
        self.rmg.wait_for_idle().unwrap()
    }
}
