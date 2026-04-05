//! Plugin system for RCode
//!
//! This crate provides a plugin architecture for extending RCode capabilities
//! through dynamically loaded plugins.

pub mod error;
pub mod manager;
pub mod loader;
pub mod manifest;
pub mod runtime;
pub mod types;

pub use error::{PluginError, Result};
pub use manager::PluginManager;
pub use loader::{LocalPluginLoader, PluginLibraryLoader};
pub use manifest::PluginManifest;
pub use runtime::PluginRuntime;
pub use types::*;
