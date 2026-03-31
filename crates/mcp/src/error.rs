//! Error types for MCP client

use thiserror::Error;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("JSON-RPC error: {0}")]
    JsonRpc(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, McpError>;
