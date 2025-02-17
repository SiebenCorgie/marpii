pub(crate) mod event;
pub(crate) mod primitive;
pub(crate) mod widget;
use std::any::{Any, TypeId};
use std::hash::{Hash, Hasher};

use ahash::{AHashMap, AHasher};
pub use widget::MarpiiSurface;

pub use event::Event;
use iced_core::mouse;
use iced_core::{Rectangle, Shell};
pub use primitive::{Primitive, Renderer};

///Unique key that identifies _any_ persistently stored data.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct PersistentKey(u64);

///Storage for custom data used by the renderer.
#[derive(Default)]
pub struct Persistent {
    map: AHashMap<PersistentKey, Box<dyn Any>>,
}

impl Persistent {
    fn key_for_name(name: &str) -> PersistentKey {
        let mut hasher = AHasher::default();
        name.hash(&mut hasher);
        PersistentKey(hasher.finish())
    }

    ///Persistently stores `data` under `name`. Returns the generated key for that data.
    ///
    ///If there is already something stored under this name, it is overwritten.
    pub fn store_named<T: 'static>(&mut self, name: &str, data: T) -> PersistentKey {
        let key = Self::key_for_name(name);
        let _ = self.map.insert(key.clone(), Box::new(data));

        key
    }

    ///Stores any datum of type `T`. Note that it uses the hash of the type-id to do identification. So there
    /// can only be one value of type `T` stored at a time. If that is not desired, consider assigning a name
    /// and using [Self:store_named] and [Self::get_named].
    ///
    /// If anything with the same hash-value is stored, it will be overwritten.
    pub fn store<T: 'static>(&mut self, data: T) -> PersistentKey {
        let mut hasher = AHasher::default();
        TypeId::of::<T>().hash(&mut hasher);
        let key = PersistentKey(hasher.finish());
        let _ = self.map.insert(key.clone(), Box::new(data));
        key
    }

    pub fn get<T: 'static>(&self, key: &PersistentKey) -> Option<&T> {
        if let Some(data) = self.map.get(key) {
            data.downcast_ref()
        } else {
            None
        }
    }

    pub fn get_mut<T: 'static>(&mut self, key: &PersistentKey) -> Option<&mut T> {
        if let Some(data) = self.map.get_mut(key) {
            data.downcast_mut()
        } else {
            None
        }
    }

    ///Tries to retrieve an object of type `T` stored under `name`.
    ///
    ///Returns None if either the object is not of type T, or there is no object at all under that name.
    pub fn get_named<T: 'static>(&self, name: &str) -> Option<&T> {
        let key = Self::key_for_name(name);
        if let Some(thing) = self.map.get(&key) {
            thing.downcast_ref()
        } else {
            None
        }
    }
    pub fn get_named_mut<T: 'static>(&mut self, name: &str) -> Option<&mut T> {
        let key = Self::key_for_name(name);
        if let Some(thing) = self.map.get_mut(&key) {
            thing.downcast_mut()
        } else {
            None
        }
    }
}

///Creates a new ['MarpiiSurface'] for a custom `program`.
pub fn marpii_surface<Message, P>(program: P) -> MarpiiSurface<Message, P>
where
    P: Program<Message>,
{
    MarpiiSurface::new(program)
}

/// The state and logic of a [`MarpiiSurface`] widget.
///
/// A [`Program`] can mutate the internal state of a [`MarpiiSurface`] widget
/// and produce messages for an application.
pub trait Program<Message> {
    /// The internal state of the [`Program`].
    type State: Default + 'static;

    /// The type of primitive this [`Program`] can draw.
    type Primitive: Primitive + 'static;

    /// Update the internal [`State`] of the [`Program`]. This can be used to reflect state changes
    /// based on mouse & other events. You can use the [`Shell`] to publish messages, request a
    /// redraw for the window, etc.
    ///
    /// By default, this method does and returns nothing.
    ///
    /// [`State`]: Self::State
    fn update(
        &self,
        _state: &mut Self::State,
        _event: Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
        _shell: &mut Shell<'_, Message>,
    ) -> (event::Status, Option<Message>) {
        (event::Status::Ignored, None)
    }

    /// Draws the [`Primitive`].
    ///
    /// [`Primitive`]: Self::Primitive
    fn draw(
        &self,
        state: &Self::State,
        cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive;

    /// Returns the current mouse interaction of the [`Program`].
    ///
    /// The interaction returned will be in effect even if the cursor position is out of
    /// bounds of the [`MarpiiSurface`]'s program.
    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}
