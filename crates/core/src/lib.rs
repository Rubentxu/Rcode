//! Core domain types and traits for opencode-rust
//!
//! This crate contains the fundamental abstractions:
//! - `Agent` trait for agent implementations
//! - `Session`, `Message`, `Part` types for conversation state
//! - `Tool` trait for tool implementations
//! - `LlmProvider` trait for AI provider integrations
//! - `Event` types for the event bus
//! - `Skill` types for skill system

pub mod agent;
pub mod session;
pub mod message;
pub mod tool;
pub mod event;
pub mod error;
pub mod provider;
pub mod config;
pub mod config_loader;
pub mod permission;
pub mod skill;

pub use agent::*;
pub use session::*;
pub use message::*;
pub use tool::*;
pub use event::*;
pub use error::*;
pub use provider::*;
pub use config::*;
pub use config_loader::*;
pub use permission::*;
pub use skill::*;
