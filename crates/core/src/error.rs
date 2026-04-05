//! Error types for RCode

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("Rate limit error: {0}")]
    RateLimit(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Service unavailable: {0}")]
    Unavailable(String),
}

#[derive(Error, Debug)]
pub enum RCodeError {
    #[error("Agent error: {0}")]
    Agent(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Permission denied: {0}")]
    Permission(String),

    #[error("Validation error: field '{field}': {message}")]
    Validation { field: String, message: String },

    #[error("Timeout error: {duration}s")]
    Timeout { duration: u64 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, RCodeError>;
