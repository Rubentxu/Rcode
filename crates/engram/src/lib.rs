//! Engram persistent memory system
//!
//! This crate provides persistent memory storage and retrieval for opencode-rust,
//! allowing agents to save decisions, discoveries, patterns, and other observations
//! that persist across sessions.
//!
//! ## Core Components
//!
//! - [`EngramClient`] - Main client for memory operations
//! - [`EngramStorage`] - SQLite storage layer with FTS5 search
//! - [`EngramTool`] - Tool implementation for agent integration
//! - [`EngramSessionIntegration`] - Session service integration
//! - [`Observation`] - The main data structure for memory entries
//!
//! ## Usage
//!
//! ```rust,ignore
//! use opencode_engram::{EngramClient, EngramTool};
//! use std::sync::Arc;
//!
//! // Create client
//! let client = Arc::new(EngramClient::new("/path/to/engram.db")?);
//!
//! // Save observations
//! let obs = Observation::new(
//!     "Rust error handling".to_string(),
//!     "Use thiserror for custom error types".to_string(),
//!     ObservationType::Pattern,
//! );
//! let id = client.save(obs).await?;
//!
//! // Search memory
//! let results = client.search("error handling", 10).await?;
//!
//! // Use as tool in agent
//! let tool = EngramTool::new(client);
//! ```

pub mod client;
pub mod engram_tool;
pub mod error;
pub mod session_integration;
pub mod storage;
pub mod types;

pub use client::EngramClient;
pub use engram_tool::EngramTool;
pub use error::{EngramError, Result};
pub use session_integration::EngramSessionIntegration;
pub use storage::EngramStorage;
pub use types::{Observation, ObservationType, Scope};
