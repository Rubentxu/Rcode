//! Plugin manager for loading, unloading, and accessing plugins

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, warn, error};

use super::{Plugin, PluginCapabilities, PluginError, Result, RouteDefinition};

/// Manager for all loaded plugins
pub struct PluginManager {
    plugins: RwLock<HashMap<String, Arc<dyn Plugin>>>,
}

impl PluginManager {
    /// Create a new empty plugin manager
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
        }
    }
    
    /// Load a plugin into the manager
    pub async fn load_plugin(&self, plugin: Arc<dyn Plugin>) -> Result<()> {
        let id = plugin.id().to_string();
        
        // Check if already loaded
        {
            let plugins = self.plugins.read();
            if plugins.contains_key(&id) {
                return Err(PluginError::AlreadyLoaded(id));
            }
        }
        
        // Call on_load hook
        if let Err(e) = plugin.on_load().await {
            error!("Plugin {} failed to load: {}", id, e);
            return Err(PluginError::LoadFailed(format!("on_load failed: {}", e)));
        }
        
        // Store the plugin
        {
            let mut plugins = self.plugins.write();
            plugins.insert(id.clone(), plugin);
        }
        
        info!("Loaded plugin: {}", id);
        Ok(())
    }
    
    /// Unload a plugin by ID
    pub async fn unload_plugin(&self, id: &str) -> Result<()> {
        let plugin = {
            let mut plugins = self.plugins.write();
            plugins.remove(id)
        };
        
        match plugin {
            Some(p) => {
                if let Err(e) = p.on_unload().await {
                    warn!("Plugin {} on_unload returned error: {}", id, e);
                }
                info!("Unloaded plugin: {}", id);
                Ok(())
            }
            None => Err(PluginError::NotFound(id.to_string())),
        }
    }
    
    /// Get a plugin by ID
    pub fn get_plugin(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        let plugins = self.plugins.read();
        plugins.get(id).cloned()
    }
    
    /// Check if a plugin is loaded
    pub fn is_loaded(&self, id: &str) -> bool {
        let plugins = self.plugins.read();
        plugins.contains_key(id)
    }
    
    /// List all loaded plugin IDs
    pub fn list_plugins(&self) -> Vec<String> {
        let plugins = self.plugins.read();
        plugins.keys().cloned().collect()
    }
    
    /// Get all plugin capabilities (commands and routes)
    pub fn get_capabilities(&self) -> Vec<(String, PluginCapabilities)> {
        let plugins = self.plugins.read();
        plugins
            .iter()
            .map(|(id, plugin)| {
                (
                    id.clone(),
                    PluginCapabilities::new(plugin.commands(), plugin.routes()),
                )
            })
            .collect()
    }
    
    /// Get all commands from all plugins
    pub fn get_all_commands(&self) -> HashMap<String, (String, String)> {
        let plugins = self.plugins.read();
        let mut commands = HashMap::new();
        
        for (plugin_id, plugin) in plugins.iter() {
            for cmd in plugin.commands() {
                commands.insert(
                    cmd.name.clone(),
                    (plugin_id.clone(), cmd.description.clone()),
                );
            }
        }
        
        commands
    }
    
    /// Get all routes from all plugins
    pub fn get_all_routes(&self) -> Vec<(String, RouteDefinition)> {
        let plugins = self.plugins.read();
        let mut routes = Vec::new();
        
        for (plugin_id, plugin) in plugins.iter() {
            for route in plugin.routes() {
                routes.push((plugin_id.clone(), route));
            }
        }
        
        routes
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
