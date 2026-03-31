//! Plugin types and definitions

use async_trait::async_trait;
use opencode_core::error::Result as CoreResult;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

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
