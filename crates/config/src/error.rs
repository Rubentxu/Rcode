//! Configuration loading error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse config JSON: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Config file not found: {0}")]
    NotFound(String),

    #[error("Invalid config: {0}")]
    Invalid(String),
}
