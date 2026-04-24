//! Runtime errors

use thiserror::Error;

/// Runtime-specific errors
#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Agent execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Runtime shutdown failed: {0}")]
    ShutdownFailed(String),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Spawn failed: {0}")]
    SpawnFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

/// Result type alias for runtime operations
pub type Result<T> = std::result::Result<T, RuntimeError>;
