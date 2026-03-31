//! Plugin error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),

    #[error("Plugin already activated: {0}")]
    AlreadyActivated(String),

    #[error("Plugin not activated: {0}")]
    NotActivated(String),

    #[error("Plugin load failed: {0}")]
    LoadFailed(String),

    #[error("Plugin unload failed: {0}")]
    UnloadFailed(String),

    #[error("Plugin activation failed: {0}")]
    ActivationFailed(String),

    #[error("Plugin deactivation failed: {0}")]
    DeactivationFailed(String),

    #[error("Plugin install failed: {0}")]
    InstallFailed(String),

    #[error("Plugin rollback failed: {0}")]
    RollbackFailed(String),

    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),

    #[error("Symbol not found in plugin: {0}")]
    SymbolNotFound(String),

    #[error("Plugin initialization failed: {0}")]
    InitFailed(String),

    #[error("Duplicate plugin: {0}")]
    DuplicatePlugin(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("LibLoading error: {0}")]
    LibLoading(#[from] libloading::Error),
}

pub type Result<T> = std::result::Result<T, PluginError>;
