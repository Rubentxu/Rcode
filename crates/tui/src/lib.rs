//! RCode TUI - Terminal User Interface using Ratatui

pub mod app;
pub mod events;
pub mod run;
pub mod views;

pub use app::{AppMode, RcodeTui};
pub use run::run;
