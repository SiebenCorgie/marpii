//! Handle events of a custom marpii widget.
use iced_core::keyboard;
use iced_core::mouse;
use iced_core::time::Instant;
use iced_core::touch;

pub use iced_core::event::Status;

/// A [`MarpiiSurface`](crate::MarpiiSurface) event.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A mouse event.
    Mouse(mouse::Event),

    /// A touch event.
    Touch(touch::Event),

    /// A keyboard event.
    Keyboard(keyboard::Event),

    /// A window requested a redraw.
    RedrawRequested(Instant),
}
