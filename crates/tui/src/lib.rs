//! RCode TUI - Terminal User Interface using Ratatui
#![allow(
    clippy::collapsible_if,
    clippy::unwrap_or_default,
    clippy::too_many_arguments,
    clippy::needless_borrow
)]

pub mod app;
pub mod events;
pub mod run;
pub mod views;

pub use app::{AppMode, RcodeTui};
pub use run::run;
