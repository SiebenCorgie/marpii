use std::marker::PhantomData;

use iced::{Element, Length, Rectangle, Size};
use iced_core::Widget;

use crate::Event;

use super::Program;

#[derive(Debug)]
pub struct ShapeCanvas<P, Message, Theme = iced::Theme, Renderer = crate::Renderer>
where
    Renderer: super::Renderer,
    P: Program<Message, Theme, Renderer>,
{
    width: Length,
    height: Length,
    program: P,
    message_: PhantomData<Message>,
    theme_: PhantomData<Theme>,
    renderer_: PhantomData<Renderer>,
}

impl<P, Message, Theme, Renderer> ShapeCanvas<P, Message, Theme, Renderer>
where
    P: Program<Message, Theme, Renderer>,
    Renderer: super::Renderer,
{
    const DEFAULT_SIZE: f32 = 100.0;

    /// Creates a new [`ShapeCanvas`].
    pub fn new(program: P) -> Self {
        ShapeCanvas {
            width: Length::Fixed(Self::DEFAULT_SIZE),
            height: Length::Fixed(Self::DEFAULT_SIZE),
            program,
            message_: PhantomData,
            theme_: PhantomData,
            renderer_: PhantomData,
        }
    }

    /// Sets the width of the [`ShapeCanvas`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`ShapeCanvas`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
}

impl<P, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for ShapeCanvas<P, Message, Theme, Renderer>
where
    Renderer: super::Renderer + iced_core::renderer::Renderer,
    P: Program<Message, Theme, Renderer>,
{
    fn tag(&self) -> iced_core::widget::tree::Tag {
        struct Tag<T>(T);
        iced_core::widget::tree::Tag::of::<Tag<P::State>>()
    }

    fn state(&self) -> iced_core::widget::tree::State {
        iced_core::widget::tree::State::new(P::State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &self,
        _tree: &mut iced_core::widget::Tree,
        _renderer: &Renderer,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        iced_core::layout::atomic(limits, self.width, self.height)
    }

    fn on_event(
        &mut self,
        tree: &mut iced_core::widget::Tree,
        event: iced_core::Event,
        layout: iced_core::Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut iced_core::Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> iced_core::event::Status {
        let bounds = layout.bounds();

        let canvas_event = match event {
            iced_core::Event::Mouse(mouse_event) => Some(Event::Mouse(mouse_event)),
            iced_core::Event::Touch(touch_event) => Some(Event::Touch(touch_event)),
            iced_core::Event::Keyboard(keyboard_event) => Some(Event::Keyboard(keyboard_event)),
            iced_core::Event::Window(_) => None,
        };

        if let Some(canvas_event) = canvas_event {
            let state = tree.state.downcast_mut::<P::State>();

            let (event_status, message) = self.program.update(state, canvas_event, bounds, cursor);

            if let Some(message) = message {
                shell.publish(message);
            }

            return event_status;
        }

        iced_core::event::Status::Ignored
    }

    fn mouse_interaction(
        &self,
        tree: &iced_core::widget::Tree,
        layout: iced_core::Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> iced_core::mouse::Interaction {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<P::State>();

        self.program.mouse_interaction(state, bounds, cursor)
    }

    fn draw(
        &self,
        tree: &iced_core::widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &iced_core::renderer::Style,
        layout: iced_core::Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        if bounds.width < 1.0 || bounds.height < 1.0 {
            return;
        }

        let state = tree.state.downcast_ref::<P::State>();

        renderer.with_translation(iced::Vector::new(bounds.x, bounds.y), |renderer| {
            let frames = self.program.draw(state, renderer, theme, bounds, cursor);

            for f in frames {
                renderer.draw_frame(f);
            }
        });
    }
}

impl<'a, P, Message, Theme, Renderer> From<ShapeCanvas<P, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + super::Renderer + iced_core::renderer::Renderer,
    P: 'a + Program<Message, Theme, Renderer>,
{
    fn from(
        canvas: ShapeCanvas<P, Message, Theme, Renderer>,
    ) -> Element<'a, Message, Theme, Renderer> {
        Element::new(canvas)
    }
}
