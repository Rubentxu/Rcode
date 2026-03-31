//! LSP error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LspError {
    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Initialization failed: {0}")]
    Initialization(String),

    #[error("Communication error: {0}")]
    Communication(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Server error: {0}")]
    Server(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Process error: {0}")]
    Process(String),
}

pub type Result<T> = std::result::Result<T, LspError>;
