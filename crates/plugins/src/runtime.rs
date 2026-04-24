//! Runtime API for plugin self-management
//!
//! This module provides the PluginRuntime which exposes safe APIs
//! that plugins can use to manage other plugins.

use std::sync::Arc;
use parking_lot::Mutex;
use tracing::{info, error};

use super::{PluginManager, PluginSpec, Result};

/// Runtime API available to plugins for managing other plugins
pub struct PluginRuntime {
    manager: Arc<PluginManager>,
    /// Specs queued via `plugins_add` — drained on the next `install` cycle
    pending_specs: Mutex<Vec<PluginSpec>>,
}

impl PluginRuntime {
    /// Create a new PluginRuntime
    pub fn new(manager: Arc<PluginManager>) -> Self {
        Self {
            manager,
            pending_specs: Mutex::new(Vec::new()),
        }
    }

    /// Add a plugin spec to the pending installation queue.
    ///
    /// The spec will be installed on the next call to `drain_pending()` or
    /// when the caller decides to flush the queue.
    pub fn plugins_add(&self, spec: PluginSpec) -> Result<()> {
        info!("PluginRuntime: queuing plugin '{}' for installation", spec.id);
        self.pending_specs.lock().push(spec);
        Ok(())
    }

    /// Drain all pending specs and install them.
    ///
    /// Returns the IDs of every spec that was attempted (regardless of outcome).
    pub fn drain_pending(&self) -> Vec<PluginSpec> {
        let specs: Vec<PluginSpec> = std::mem::take(&mut *self.pending_specs.lock());
        for spec in &specs {
            let manager = self.manager.clone();
            let spec_clone = spec.clone();
            tokio::runtime::Handle::current().spawn(async move {
                if let Err(e) = manager.install(spec_clone).await {
                    error!("Plugin installation failed during drain: {}", e);
                }
            });
        }
        specs
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

    /// Returns the number of specs currently in the pending queue
    pub fn pending_count(&self) -> usize {
        self.pending_specs.lock().len()
    }
}

impl Clone for PluginRuntime {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
            // Clone does NOT share the pending queue — each clone starts empty
            pending_specs: Mutex::new(Vec::new()),
        }
    }
}
