//! LSP (Language Server Protocol) integration for code intelligence
//!
//! This crate provides:
//! - [`LspClient`] for connecting to language servers via stdio
//! - [`LanguageServerRegistry`] for managing multiple LSP connections
//! - [`LspToolAdapter`] for integrating LSP features into the agent tool system
//!
//! # Example
//!
//! ```ignore
//! use rcode_lsp::{LspClient, LanguageServerRegistry};
//!
//! // Start a Rust language server
//! let registry = LanguageServerRegistry::new();
//! registry.start_server(
//!     "rust".to_string(),
//!     &["rust-analyzer"],
//!     std::path::Path::new("/project"),
//!     "rust"
//! ).await?;
//! ```

pub mod client;
pub mod error;
pub mod lsp_tool;
pub mod registry;
pub mod transport;
pub mod types;

pub use client::LspClient;
pub use error::{LspError, Result};
pub use lsp_tool::LspToolAdapter;
pub use registry::LanguageServerRegistry;
pub use types::*;
