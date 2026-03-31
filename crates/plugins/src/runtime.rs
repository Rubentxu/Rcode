//! Runtime API for plugin self-management
//!
//! This module provides the PluginRuntime which exposes safe APIs
//! that plugins can use to manage other plugins.

use std::sync::Arc;
use tracing::{info, error};

use super::{PluginManager, PluginSpec, Result};

/// Runtime API available to plugins for managing other plugins
pub struct PluginRuntime {
    manager: Arc<PluginManager>,
}

impl PluginRuntime {
    /// Create a new PluginRuntime
    pub fn new(manager: Arc<PluginManager>) -> Self {
        Self { manager }
    }

    /// Add a plugin spec to the installation queue
    /// Note: This just registers the spec, actual installation happens via install()
    pub fn plugins_add(&self, spec: PluginSpec) -> Result<()> {
        info!("PluginRuntime: plugins_add called for {}", spec.id);
        // In a real implementation, this might queue the plugin for later installation
        Ok(())
    }

    /// Install a plugin from a spec
    pub fn plugins_install(&self, spec: PluginSpec) -> Result<()> {
        info!("PluginRuntime: plugins_install called for {}", spec.id);
        // This would typically be called within a tokio runtime
        // For now, we spawn a task to handle the async call
        let manager = self.manager.clone();
        tokio::runtime::Handle::current().spawn(async move {
            if let Err(e) = manager.install(spec).await {
                error!("Plugin installation failed: {}", e);
            }
        });
        Ok(())
    }

    /// Activate a plugin by ID
    pub fn plugins_activate(&self, id: &str) -> Result<()> {
        info!("PluginRuntime: plugins_activate called for {}", id);
        let manager = self.manager.clone();
        let id = id.to_string();
        tokio::runtime::Handle::current().spawn(async move {
            if let Err(e) = manager.activate(&id).await {
                error!("Plugin activation failed: {}", e);
            }
        });
        Ok(())
    }

    /// Deactivate a plugin by ID
    pub fn plugins_deactivate(&self, id: &str) -> Result<()> {
        info!("PluginRuntime: plugins_deactivate called for {}", id);
        let manager = self.manager.clone();
        let id = id.to_string();
        tokio::runtime::Handle::current().spawn(async move {
            if let Err(e) = manager.deactivate(&id).await {
                error!("Plugin deactivation failed: {}", e);
            }
        });
        Ok(())
    }

    /// Check if a plugin is active
    pub fn plugins_is_active(&self, id: &str) -> bool {
        self.manager.is_active(id)
    }

    /// List all active plugins
    pub fn plugins_list_active(&self) -> Vec<String> {
        self.manager.list_active()
    }

    /// List all installed plugins
    pub fn plugins_list_all(&self) -> Vec<String> {
        self.manager.list_plugins()
    }
}

impl Clone for PluginRuntime {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
        }
    }
}