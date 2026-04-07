//! RCode CLI library
#![allow(
    clippy::collapsible_else_if,
    clippy::await_holding_lock,
    clippy::map_clone,
    clippy::derivable_impls
)]

pub mod commands;

pub use commands::{Run, Serve, Tui, Acp};
