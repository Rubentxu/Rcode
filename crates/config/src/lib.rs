//! Configuration types and loading for RCode

pub use rcode_core::RcodeConfig;

pub mod loader;
pub use loader::load_config;

pub mod error;
pub use error::ConfigError;

pub type Result<T> = std::result::Result<T, ConfigError>;
