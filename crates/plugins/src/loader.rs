//! Local plugin loader for discovering and loading plugins from directories

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn, debug};

use super::{Plugin, PluginError, PluginMetadata, Result};
use super::manifest::PluginManifest;

/// Loader for plugins from local directories
pub struct LocalPluginLoader {
    plugin_dirs: Vec<PathBuf>,
}

impl LocalPluginLoader {
    /// Create a new loader with the given plugin directories
    pub fn new(plugin_dirs: Vec<PathBuf>) -> Self {
        Self { plugin_dirs }
    }
    
    /// Add a directory to scan for plugins
    pub fn add_plugin_dir(&mut self, dir: PathBuf) {
        self.plugin_dirs.push(dir);
    }
    
    /// Discover all plugins in configured directories
    pub async fn discover_plugins(&self) -> Result<Vec<PluginMetadata>> {
        let mut discovered = Vec::new();
        
        for dir in &self.plugin_dirs {
            match self.discover_in_dir(dir).await {
                Ok(plugins) => discovered.extend(plugins),
                Err(e) => {
                    warn!("Failed to discover plugins in {}: {}", dir.display(), e);
                }
            }
        }
        
        Ok(discovered)
    }
    
    /// Discover plugins in a specific directory
    async fn discover_in_dir(&self, dir: &Path) -> Result<Vec<PluginMetadata>> {
        let mut plugins = Vec::new();
        
        if !dir.exists() {
            return Ok(plugins);
        }
        
        let entries = std::fs::read_dir(dir)
            .map_err(|e| PluginError::DiscoveryFailed(format!("Failed to read {}: {}", dir.display(), e)))?;
        
        for entry in entries.flatten() {
            let path = entry.path();
            
            if path.is_dir() {
                // Try to load manifest
                match PluginManifest::from_dir(&path) {
                    Ok(manifest) => {
                        info!("Discovered plugin: {} at {}", manifest.id, path.display());
                        plugins.push(manifest.to_metadata(path));
                    }
                    Err(e) => {
                        // Not a plugin directory, skip
                        debug!("Skipping {}: not a plugin (error: {})", path.display(), e);
                    }
                }
            }
        }
        
        Ok(plugins)
    }
    
    /// Load a plugin from a directory
    pub async fn load_from_dir(&self, dir: &Path) -> Result<Arc<dyn Plugin>> {
        let manifest = PluginManifest::from_dir(dir)?;
        
        // Resolve the main library path relative to the plugin directory
        let main_path = dir.join(&manifest.main);
        
        PluginLibraryLoader::load_from_file(&main_path).await
    }
}

/// Type alias for the plugin factory function
type PluginFactory = Box<dyn Fn() -> Result<Arc<dyn Plugin>> + Send + Sync>;

/// Generic plugin library loader using dynamic library loading
pub struct PluginLibraryLoader;

impl PluginLibraryLoader {
    /// Load a plugin from a shared library file
    pub async fn load_from_file(path: &Path) -> Result<Arc<dyn Plugin>> {
        let path_str = path.to_string_lossy().to_string();
        
        let library = unsafe {
            libloading::Library::new(&path_str)
                .map_err(|e| PluginError::LoadFailed(format!("Failed to load library {}: {}", path_str, e)))?
        };
        
        // Get the plugin factory symbol
        let factory: libloading::Symbol<PluginFactory> = unsafe {
            library.get(b"opencode_plugin_create")
                .map_err(|e| PluginError::SymbolNotFound(format!("Failed to get factory symbol: {}", e)))?
        };
        
        // Call the factory to create the plugin instance
        let plugin = factory()
            .map_err(|e| PluginError::InitFailed(format!("Factory call failed: {}", e)))?;
        
        info!("Loaded plugin from: {}", path_str);
        
        Ok(plugin)
    }
}
