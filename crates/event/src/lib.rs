//! Event bus implementation using tokio broadcast

pub mod bus;

pub use bus::{EventBus, Event, EventSubscriber, SseEvent};

pub fn event<T: Clone + Send + Sync + 'static>(id: &'static str) -> EventDef<T> {
    EventDef { id, _phantom: std::marker::PhantomData }
}

pub struct EventDef<T> {
    pub id: &'static str,
    _phantom: std::marker::PhantomData<T>,
}
