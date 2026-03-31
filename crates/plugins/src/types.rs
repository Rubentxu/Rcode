//! Plugin types and definitions

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use opencode_core::error::Result as CoreResult;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

/// Represents a command that can be registered by a plugin
#[derive(Debug, Clone)]
pub struct CommandDefinition {
    pub name: String,
    pub description: String,
}

/// Represents a route that can be registered by a plugin
#[derive(Debug, Clone)]
pub struct RouteDefinition {
    pub method: String,
    pub path: String,
}

/// Handler for plugin commands
pub trait CommandHandler: Send + Sync {
    fn handle(
        &self,
        args: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = CoreResult<CommandOutput>> + Send + '_>>;
}

/// Output from a command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutput {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Handler for plugin routes
pub trait RouteHandler: Send + Sync {
    fn handle(
        &self,
        request: RouteRequest,
    ) -> Pin<Box<dyn Future<Output = CoreResult<RouteResponse>> + Send + '_>>;
}

/// Request data for a route handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRequest {
    pub method: String,
    pub path: String,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub query_params: std::collections::HashMap<String, String>,
}

/// Response data from a route handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResponse {
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Vec<u8>,
}

/// Metadata for a discovered plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub main: String,
    pub path: std::path::PathBuf,
}

/// Plugin capability information
#[derive(Debug, Clone)]
pub struct PluginCapabilities {
    pub commands: Vec<CommandDefinition>,
    pub routes: Vec<RouteDefinition>,
}

impl PluginCapabilities {
    pub fn new(commands: Vec<CommandDefinition>, routes: Vec<RouteDefinition>) -> Self {
        Self { commands, routes }
    }
}

/// Core Plugin trait that all plugins must implement
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Unique identifier for this plugin
    fn id(&self) -> &str;
    
    /// Human-readable name for this plugin
    fn name(&self) -> &str;
    
    /// Plugin version string
    fn version(&self) -> &str;

    /// Optional description of what the plugin does
    fn description(&self) -> Option<&str> {
        None
    }
    
    /// Called when the plugin is loaded
    async fn on_load(&self) -> CoreResult<()>;
    
    /// Called when the plugin is unloaded
    async fn on_unload(&self) -> CoreResult<()>;
    
    /// Returns the commands this plugin provides
    fn commands(&self) -> Vec<CommandDefinition>;
    
    /// Returns the routes this plugin provides
    fn routes(&self) -> Vec<RouteDefinition>;
}

/// Plugin lifecycle state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    /// Plugin is installed but not activated
    Installed,
    /// Plugin is activated and running
    Activated,
    /// Plugin was activated but is now deactivated
    Deactivated,
    /// Plugin is in an error state
    Error(String),
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginState::Installed => write!(f, "installed"),
            PluginState::Activated => write!(f, "activated"),
            PluginState::Deactivated => write!(f, "deactivated"),
            PluginState::Error(msg) => write!(f, "error: {}", msg),
        }
    }
}

/// Entry in the plugin registry with full state tracking
pub struct PluginEntry {
    /// Unique plugin identifier
    pub id: String,
    /// Current lifecycle state
    pub state: PluginState,
    /// The plugin instance
    pub plugin: Arc<dyn Plugin>,
    /// When the plugin was installed
    pub installed_at: DateTime<Utc>,
    /// When the plugin was last activated (if ever)
    pub activated_at: Option<DateTime<Utc>>,
    /// Cached installation path
    pub cached_path: Option<PathBuf>,
}

impl PluginEntry {
    /// Create a new plugin entry in Installed state
    pub fn new(id: String, plugin: Arc<dyn Plugin>) -> Self {
        Self {
            id,
            state: PluginState::Installed,
            plugin,
            installed_at: Utc::now(),
            activated_at: None,
            cached_path: None,
        }
    }

    /// Transition to activated state
    pub fn activate(&mut self) {
        self.state = PluginState::Activated;
        self.activated_at = Some(Utc::now());
    }

    /// Transition to deactivated state
    pub fn deactivate(&mut self) {
        self.state = PluginState::Deactivated;
    }

    /// Transition to error state
    pub fn set_error(&mut self, error: String) {
        self.state = PluginState::Error(error);
    }
}

/// Specification for installing a plugin
#[derive(Debug, Clone)]
pub struct PluginSpec {
    /// Plugin identifier (for file plugins, this is the directory name)
    pub id: String,
    /// Source location (file path or npm package name)
    pub source: PluginSource,
    /// Optional version constraint (for npm packages)
    pub version_constraint: Option<String>,
}

impl PluginSpec {
    /// Create a spec from a local file path
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let id = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        Self {
            id,
            source: PluginSource::File(path),
            version_constraint: None,
        }
    }

    /// Create a spec from an npm package
    pub fn from_npm(package: impl Into<String>) -> Self {
        let package = package.into();
        Self {
            id: package.clone(),
            source: PluginSource::Npm(package),
            version_constraint: None,
        }
    }
}

/// Source location for a plugin
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource {
    /// Local file system path
    File(PathBuf),
    /// npm package name
    Npm(String),
}
