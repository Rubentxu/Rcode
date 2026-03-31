//! Configuration types and loading for opencode

pub use opencode_core::OpencodeConfig;

pub mod loader;
pub use loader::load_config;

pub mod error;
pub use error::ConfigError;

pub type Result<T> = std::result::Result<T, ConfigError>;
