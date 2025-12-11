use super::{Event, Program};
use iced::{window, Length, Rectangle, Size};
use iced_core::{
    layout::{self, Layout},
    mouse,
    widget::{
        self,
        tree::{self, Tree},
        Widget,
    },
    Clipboard, Shell,
};
use std::marker::PhantomData;

//NOTE: Most of this is just taken from the wgpu `Shader` widget.

/// A widget which can render a custom MarpII-Rmg pass.
///
/// Must be initialized with a [`Program`], which describes the internal widget state & how
/// its [`Program::Primitive`]s are drawn.
pub struct MarpiiSurface<Message, P: Program<Message>> {
    width: Length,
    height: Length,
    program: P,
    _message: PhantomData<Message>,
}

impl<Message, P: Program<Message>> MarpiiSurface<Message, P> {
    /// Create a new custom [`MarpiiSurface`].
    pub fn new(program: P) -> Self {
        Self {
            width: Length::Fixed(100.0),
            height: Length::Fixed(100.0),
            program,
            _message: PhantomData,
        }
    }

    /// Set the `width` of the custom [`MarpiiSurface`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Set the `height` of the custom [`MarpiiSurface`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
}

impl<P, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for MarpiiSurface<Message, P>
where
    P: Program<Message>,
    Renderer: super::Renderer,
{
    fn tag(&self) -> tree::Tag {
        struct Tag<T>(T);
        tree::Tag::of::<Tag<P::State>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(P::State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced_core::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        //translates all events that apply into the widget's event type
        let custom_shader_event = match event {
            iced_core::Event::Mouse(mouse_event) => Some(Event::Mouse(*mouse_event)),
            iced_core::Event::Keyboard(keyboard_event) => {
                Some(Event::Keyboard(keyboard_event.clone()))
            }
            iced_core::Event::Touch(touch_event) => Some(Event::Touch(touch_event.clone())),
            iced::Event::InputMethod(input_method) => {
                Some(Event::InputMethod(input_method.clone()))
            }
            iced_core::Event::Window(window::Event::RedrawRequested(instant)) => {
                Some(Event::RedrawRequested(*instant))
            }
            iced_core::Event::Window(_) => None,
        };

        if let Some(custom_shader_event) = custom_shader_event {
            let state = tree.state.downcast_mut::<P::State>();

            let message = self
                .program
                .update(state, custom_shader_event, bounds, cursor, shell);

            if let Some(message) = message {
                shell.publish(message);
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<P::State>();

        self.program.mouse_interaction(state, bounds, cursor)
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &iced_core::renderer::Style,
        layout: Layout<'_>,
        cursor_position: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<P::State>();

        renderer.draw_primitive(bounds, self.program.draw(state, cursor_position, bounds));
    }
}

impl<'a, Message, Theme, Renderer, P> From<MarpiiSurface<Message, P>>
    for iced_core::Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: super::Renderer,
    P: Program<Message> + 'a,
{
    fn from(custom: MarpiiSurface<Message, P>) -> iced_core::Element<'a, Message, Theme, Renderer> {
        iced_core::Element::new(custom)
    }
}

impl<Message, T> Program<Message> for &T
where
    T: Program<Message>,
{
    type State = T::State;
    type Primitive = T::Primitive;

    fn update(
        &self,
        state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        shell: &mut Shell<'_, Message>,
    ) -> Option<Message> {
        T::update(self, state, event, bounds, cursor, shell)
    }

    fn draw(
        &self,
        state: &Self::State,
        cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        T::draw(self, state, cursor, bounds)
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        T::mouse_interaction(self, state, bounds, cursor)
    }
}
