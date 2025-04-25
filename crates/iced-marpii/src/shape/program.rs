use iced::Rectangle;

use super::Frame;

pub type Event = crate::Event;
pub type Status = crate::custom::Status;

/// The state and logic of a [`ShapeCanvas`].
///
/// A [`Program`] can mutate internal state and produce messages for an
/// application.
///
/// [`ShapeCanvas`]: crate::shape::ShapeCanvas
pub trait Program<Message, Theme = iced::Theme, Renderer = crate::Renderer>
where
    Renderer: super::Renderer,
{
    /// The internal state mutated by the [`Program`].
    type State: Default + 'static;

    /// Updates the [`State`](Self::State) of the [`Program`].
    ///
    /// When a [`Program`] is used in a [`ShapeCanvas`], the runtime will call this
    /// method for each [`Event`].
    ///
    /// This method can optionally return a `Message` to notify an application
    /// of any meaningful interactions.
    ///
    /// By default, this method does and returns nothing.
    ///
    /// [`ShapeCanvas`]: crate::shape::ShapeCanvas
    fn update(
        &self,
        _state: &mut Self::State,
        _event: Event,
        _bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> (Status, Option<Message>) {
        (Status::Ignored, None)
    }

    /// Draws the state of the [`Program`], producing a bunch of [`Frame`].
    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> Vec<Frame>;

    /// Returns the current mouse interaction of the [`Program`].
    ///
    /// The interaction returned will be in effect even if the cursor position
    /// is out of bounds of the program's [`ShapeCanvas`].
    ///
    /// [`ShapeCanvas`]: crate::shape::ShapeCanvas
    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> iced_core::mouse::Interaction {
        iced_core::mouse::Interaction::default()
    }
}

impl<Message, Theme, Renderer, T> Program<Message, Theme, Renderer> for &T
where
    Renderer: super::Renderer,
    T: Program<Message, Theme, Renderer>,
{
    type State = T::State;

    fn update(
        &self,
        state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> (Status, Option<Message>) {
        T::update(self, state, event, bounds, cursor)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> Vec<Frame> {
        T::draw(self, state, renderer, theme, bounds, cursor)
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> iced_core::mouse::Interaction {
        T::mouse_interaction(self, state, bounds, cursor)
    }
}
