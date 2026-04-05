//! Event handling for TUI (keyboard, mouse, async)

use crossterm::event::{Event as CrosstermEvent, KeyEvent, MouseEvent};
use rcode_event::Event;
use std::time::Duration;

/// TUI input events
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Keyboard key pressed
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick (periodic timer event)
    Tick,
    /// Bus event from the event bus (streaming, messages, etc.)
    BusEvent(Event),
}

/// Event handler configuration
pub struct EventHandlerConfig {
    /// Tick rate for periodic events (e.g., for animations)
    pub tick_rate: Duration,
    /// Whether to enable mouse support
    pub enable_mouse: bool,
}

impl Default for EventHandlerConfig {
    fn default() -> Self {
        Self {
            tick_rate: Duration::from_millis(50),
            enable_mouse: true,
        }
    }
}

/// Parse crossterm events into our InputEvent format
pub fn parse_event(event: CrosstermEvent) -> Option<InputEvent> {
    match event {
        CrosstermEvent::Key(key) => Some(InputEvent::Key(key)),
        CrosstermEvent::Mouse(mouse) => Some(InputEvent::Mouse(mouse)),
        CrosstermEvent::Resize(width, height) => Some(InputEvent::Resize(width, height)),
        _ => None,
    }
}
