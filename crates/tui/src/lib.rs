//! OpenCode TUI - Terminal User Interface using Ratatui

pub mod app;
pub mod events;
pub mod run;
pub mod views;

pub use app::{AppMode, OpencodeTui};
pub use run::run;
