//! MCP (Model Context Protocol) implementation
//!
//! This crate provides a client for connecting to MCP servers
//! and exposing their tools as regular tools in the system.

pub mod client;
pub mod transport;
pub mod types;
pub mod registry;
pub mod error;

pub use client::McpClient;
pub use transport::{McpTransport, StdioTransport, HttpTransport};
pub use types::{McpTool, McpMessage, McpToolResult};
pub use registry::McpServerRegistry;
pub use error::McpError;
