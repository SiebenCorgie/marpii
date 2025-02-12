use iced_graphics::{compositor, error::Reason};
use marpii::{OoS, ash::vk};
use marpii_rmg::Rmg;
use marpii_rmg_tasks::SwapchainPresent;

use super::Renderer;

pub struct Compositor {
    rmg: Rmg,
    settings: iced_graphics::Settings,
    //Swapchain handling
    swapchain: SwapchainPresent,
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
        let (ctx, surface) = marpii::context::Ctx::default_with_surface(&compatible_window, true)
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

        Ok(Self {
            rmg,
            swapchain,
            settings,
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
        todo!("Build the renderpass and everything")
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

        swapchain
    }

    fn configure_surface(&mut self, surface: &mut Self::Surface, width: u32, height: u32) {
        let surface_extent = vk::Extent2D { width, height };
        surface
            .recreate(surface_extent)
            .expect("Failed to explicitly resize swapchain!");
    }

    fn screenshot<T: AsRef<str>>(
        &mut self,
        renderer: &mut Self::Renderer,
        surface: &mut Self::Surface,
        viewport: &iced_graphics::Viewport,
        background_color: iced::Color,
        overlay: &[T],
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
