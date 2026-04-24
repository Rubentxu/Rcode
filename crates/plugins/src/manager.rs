//! Plugin manager for loading, unloading, and accessing plugins

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, warn, error, debug};

use super::{Plugin, PluginCapabilities, PluginError, PluginEntry, PluginSpec, PluginSource, PluginState, Result, RouteDefinition};
use super::loader::LocalPluginLoader;
use super::types::{CommandHandler, CommandOutput, RouteHandler, RouteRequest, RouteResponse};

/// Manager for all loaded plugins with full lifecycle support
pub struct PluginManager {
    /// All known plugins (installed/loaded)
    plugins: RwLock<HashMap<String, PluginEntry>>,
    /// Currently activated plugin IDs
    active_plugins: RwLock<HashSet<String>>,
    /// Cache of installed plugin paths (plugin_id -> cached_path)
    install_cache: RwLock<HashMap<String, std::path::PathBuf>>,
    /// Registered command handlers: command_name -> handler
    command_handlers: RwLock<HashMap<String, Arc<dyn CommandHandler>>>,
    /// Registered route handlers: (method, path) -> handler
    route_handlers: RwLock<HashMap<(String, String), Arc<dyn RouteHandler>>>,
}

impl PluginManager {
    /// Create a new empty plugin manager
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            active_plugins: RwLock::new(HashSet::new()),
            install_cache: RwLock::new(HashMap::new()),
            command_handlers: RwLock::new(HashMap::new()),
            route_handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Load a plugin into the manager (does not activate it)
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

        // Store the plugin entry
        {
            let mut plugins = self.plugins.write();
            plugins.insert(id.clone(), PluginEntry::new(id.clone(), plugin));
        }

        info!("Loaded plugin: {}", id);
        Ok(())
    }

    /// Install a plugin from a spec and load it
    pub async fn install(&self, spec: PluginSpec) -> Result<()> {
        let id = spec.id.clone();
        debug!("Installing plugin: {} from {:?}", id, spec.source);

        // Check if already installed
        {
            let plugins = self.plugins.read();
            if plugins.contains_key(&id) {
                return Err(PluginError::AlreadyLoaded(id));
            }
        }

        // Resolve the actual path (handles deduplication)
        let resolved_path = self.resolve_duplicates(&spec)?;

        // Store in install cache
        {
            let mut cache = self.install_cache.write();
            cache.insert(id.clone(), resolved_path.clone());
        }

        // Load the plugin from the resolved path and register it
        match &spec.source {
            PluginSource::File(path) => {
                // Use the directory of the resolved path as the plugin directory
                let plugin_dir = if path.is_dir() {
                    path.clone()
                } else {
                    path.parent().map(|p| p.to_path_buf()).unwrap_or(resolved_path.clone())
                };
                let loader = LocalPluginLoader::new(vec![]);
                match loader.load_from_dir(&plugin_dir).await {
                    Ok(plugin) => {
                        self.load_plugin(plugin).await?;
                    }
                    Err(e) => {
                        // Remove from cache since load failed
                        self.install_cache.write().remove(&id);
                        return Err(PluginError::LoadFailed(format!(
                            "Failed to load plugin {} from {:?}: {}", id, plugin_dir, e
                        )));
                    }
                }
            }
            PluginSource::Npm(_package_name) => {
                // npm plugins are not yet supported; path is a placeholder
                // This remains a stub intentionally — container/K8s runtime only
                debug!("npm plugin {} registered in cache (loading not yet supported)", id);
            }
        }

        info!("Installed plugin: {} -> {:?}", id, resolved_path);
        Ok(())
    }

    /// Resolve duplicates based on source type
    fn resolve_duplicates(&self, spec: &PluginSpec) -> Result<std::path::PathBuf> {
        match &spec.source {
            PluginSource::File(path) => {
                // For file plugins, resolve to absolute path and dedupe by exact path
                let resolved = path.canonicalize().unwrap_or_else(|_| path.clone());
                let cache = self.install_cache.read();

                // Check if we already have this exact path cached
                for (cached_id, cached_path) in cache.iter() {
                    if cached_path == &resolved {
                        info!("Plugin {} is duplicate of cached {}", spec.id, cached_id);
                        return Err(PluginError::DuplicatePlugin(format!(
                            "Plugin {} resolves to same file as {}",
                            spec.id, cached_id
                        )));
                    }
                }
                Ok(resolved)
            }
            PluginSource::Npm(package_name) => {
                // For npm packages, dedupe by package name
                let cache = self.install_cache.read();

                for (cached_id, _) in cache.iter() {
                    if cached_id == package_name {
                        info!("Plugin {} is duplicate npm package", spec.id);
                        return Err(PluginError::DuplicatePlugin(format!(
                            "npm package {} is already installed as {}",
                            package_name, cached_id
                        )));
                    }
                }

                // In a real implementation, this would resolve to node_modules path
                // For now, return a placeholder path
                Ok(std::path::PathBuf::from(format!("node_modules/{}", package_name)))
            }
        }
    }

    /// Activate a plugin by ID
    pub async fn activate(&self, id: &str) -> Result<()> {
        let mut plugins = self.plugins.write();

        let entry = plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;

        // Check if already activated
        if self.active_plugins.read().contains(id) {
            return Err(PluginError::AlreadyActivated(id.to_string()));
        }

        // Check if in error state
        if matches!(entry.state, PluginState::Error(_)) {
            return Err(PluginError::ActivationFailed(format!(
                "Plugin {} is in error state",
                id
            )));
        }

        // Transition to activated
        entry.activate();

        // Add to active set
        self.active_plugins.write().insert(id.to_string());

        info!("Activated plugin: {}", id);
        Ok(())
    }

    /// Activate a plugin with automatic rollback on failure
    pub async fn activate_with_rollback(&self, id: &str) -> Result<()> {
        // Capture the original state
        let original_state: HashSet<String> = self.active_plugins.read().clone();

        // Attempt activation
        match self.activate(id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                // Rollback to original state
                error!("Activation of {} failed, rolling back: {}", id, e);
                let mut active = self.active_plugins.write();
                *active = original_state;

                // Set plugin to error state
                if let Some(mut plugins) = self.plugins.try_write()
                    && let Some(entry) = plugins.get_mut(id)
                {
                    entry.set_error(e.to_string());
                }

                Err(PluginError::RollbackFailed(format!(
                    "Activation failed and rolled back: {}",
                    e
                )))
            }
        }
    }

    /// Deactivate a plugin by ID
    pub async fn deactivate(&self, id: &str) -> Result<()> {
        let mut plugins = self.plugins.write();

        let entry = plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;

        // Check if actually activated
        if !self.active_plugins.read().contains(id) {
            return Err(PluginError::NotActivated(id.to_string()));
        }

        // Transition to deactivated
        entry.deactivate();

        // Remove from active set
        self.active_plugins.write().remove(id);

        info!("Deactivated plugin: {}", id);
        Ok(())
    }

    /// Check if a plugin is currently activated
    pub fn is_active(&self, id: &str) -> bool {
        self.active_plugins.read().contains(id)
    }

    /// List all currently activated plugin IDs
    pub fn list_active(&self) -> Vec<String> {
        self.active_plugins.read().iter().cloned().collect()
    }

    /// Unload a plugin by ID (must be deactivated first)
    pub async fn unload_plugin(&self, id: &str) -> Result<()> {
        // Check if still active
        if self.is_active(id) {
            return Err(PluginError::DeactivationFailed(format!(
                "Cannot unload active plugin {}. Deactivate it first.",
                id
            )));
        }

        let entry = {
            let mut plugins = self.plugins.write();
            plugins.remove(id)
        };

        match entry {
            Some(entry) => {
                if let Err(e) = entry.plugin.on_unload().await {
                    warn!("Plugin {} on_unload returned error: {}", id, e);
                }

                // Remove from cache
                {
                    let mut cache = self.install_cache.write();
                    cache.remove(id);
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
        plugins.get(id).map(|e| e.plugin.clone())
    }

    /// Get a plugin entry (includes state) by ID
    pub fn get_entry(&self, id: &str) -> Option<PluginEntry> {
        let plugins = self.plugins.read();
        plugins.get(id).cloned()
    }

    /// Check if a plugin is loaded (installed)
    pub fn is_loaded(&self, id: &str) -> bool {
        let plugins = self.plugins.read();
        plugins.contains_key(id)
    }

    /// Get the current state of a plugin
    pub fn get_state(&self, id: &str) -> Option<PluginState> {
        let plugins = self.plugins.read();
        plugins.get(id).map(|e| e.state.clone())
    }

    /// List all loaded plugin IDs
    pub fn list_plugins(&self) -> Vec<String> {
        let plugins = self.plugins.read();
        plugins.keys().cloned().collect()
    }

    /// List all plugins with their states
    pub fn list_all_with_state(&self) -> Vec<(String, PluginState)> {
        let plugins = self.plugins.read();
        plugins
            .iter()
            .map(|(id, entry)| (id.clone(), entry.state.clone()))
            .collect()
    }

    /// Get all plugin capabilities (commands and routes) for activated plugins only
    pub fn get_capabilities(&self) -> Vec<(String, PluginCapabilities)> {
        let plugins = self.plugins.read();
        let active = self.active_plugins.read();

        plugins
            .iter()
            .filter(|(id, _)| active.contains(*id))
            .map(|(id, entry)| {
                (
                    id.clone(),
                    PluginCapabilities::new(entry.plugin.commands(), entry.plugin.routes()),
                )
            })
            .collect()
    }

    /// Get all commands from activated plugins
    pub fn get_all_commands(&self) -> HashMap<String, (String, String)> {
        let plugins = self.plugins.read();
        let active = self.active_plugins.read();
        let mut commands = HashMap::new();

        for (plugin_id, entry) in plugins.iter() {
            if !active.contains(plugin_id) {
                continue;
            }
            for cmd in entry.plugin.commands() {
                commands.insert(
                    cmd.name.clone(),
                    (plugin_id.clone(), cmd.description.clone()),
                );
            }
        }

        commands
    }

    /// Get all routes from activated plugins
    pub fn get_all_routes(&self) -> Vec<(String, RouteDefinition)> {
        let plugins = self.plugins.read();
        let active = self.active_plugins.read();
        let mut routes = Vec::new();

        for (plugin_id, entry) in plugins.iter() {
            if !active.contains(plugin_id) {
                continue;
            }
            for route in entry.plugin.routes() {
                routes.push((plugin_id.clone(), route));
            }
        }

        routes
    }

    /// Register a command handler for a given command name.
    pub fn register_command_handler(&self, name: impl Into<String>, handler: Arc<dyn CommandHandler>) {
        self.command_handlers.write().insert(name.into(), handler);
    }

    /// Register a route handler for a given HTTP method + path pair.
    pub fn register_route_handler(
        &self,
        method: impl Into<String>,
        path: impl Into<String>,
        handler: Arc<dyn RouteHandler>,
    ) {
        self.route_handlers
            .write()
            .insert((method.into(), path.into()), handler);
    }

    /// Dispatch a command by name.
    ///
    /// Returns `PluginError::NotFound` when no handler is registered for the command.
    pub async fn dispatch_command(
        &self,
        command: &str,
        args: serde_json::Value,
    ) -> Result<CommandOutput> {
        let handler = self
            .command_handlers
            .read()
            .get(command)
            .cloned()
            .ok_or_else(|| PluginError::NotFound(format!("command '{}'", command)))?;

        handler.handle(args).await.map_err(|e| {
            PluginError::ExecutionFailed(format!("Command '{}' failed: {}", command, e))
        })
    }

    /// Dispatch an HTTP-style route request.
    ///
    /// Returns `PluginError::NotFound` when no handler is registered for the
    /// `(method, path)` pair.
    pub async fn dispatch_route(&self, request: RouteRequest) -> Result<RouteResponse> {
        let key = (request.method.clone(), request.path.clone());
        let handler = self
            .route_handlers
            .read()
            .get(&key)
            .cloned()
            .ok_or_else(|| {
                PluginError::NotFound(format!("route '{} {}'", request.method, request.path))
            })?;

        handler.handle(request).await.map_err(|e| {
            PluginError::ExecutionFailed(format!("Route handler failed: {}", e))
        })
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

// Manual Clone implementation since PluginEntry doesn't implement Clone
impl Clone for PluginEntry {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            state: self.state.clone(),
            plugin: self.plugin.clone(),
            installed_at: self.installed_at,
            activated_at: self.activated_at,
            cached_path: self.cached_path.clone(),
        }
    }
}