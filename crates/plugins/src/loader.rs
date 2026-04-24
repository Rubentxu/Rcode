//! Local plugin loader for discovering and loading plugins from directories

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn, debug};

use super::{Plugin, PluginError, PluginMetadata, Result};
use super::manifest::PluginManifest;
use super::types::{CommandDefinition, RouteDefinition};

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

/// Wrapper that keeps the dynamic library alive alongside the plugin.
///
/// When a plugin is loaded from a shared library (`dlopen`/`LoadLibrary`), the
/// vtable pointers inside the `Arc<dyn Plugin>` point into the library's text
/// segment. If the `Library` handle is dropped, those pointers become dangling
/// and any call through the trait object is UB. This wrapper co-owns both the
/// plugin and the library so they are dropped together.
struct LibraryBoundPlugin {
    inner: Arc<dyn Plugin>,
    /// Kept alive purely for its `Drop` impl — must outlive `inner`.
    _lib: Arc<libloading::Library>,
}

impl LibraryBoundPlugin {
    fn new(inner: Arc<dyn Plugin>, lib: libloading::Library) -> Self {
        Self {
            inner,
            _lib: Arc::new(lib),
        }
    }
}

#[async_trait::async_trait]
impl Plugin for LibraryBoundPlugin {
    fn id(&self) -> &str { self.inner.id() }
    fn name(&self) -> &str { self.inner.name() }
    fn version(&self) -> &str { self.inner.version() }
    fn description(&self) -> Option<&str> { self.inner.description() }
    async fn on_load(&self) -> rcode_core::error::Result<()> { self.inner.on_load().await }
    async fn on_unload(&self) -> rcode_core::error::Result<()> { self.inner.on_unload().await }
    fn commands(&self) -> Vec<CommandDefinition> { self.inner.commands() }
    fn routes(&self) -> Vec<RouteDefinition> { self.inner.routes() }
}

/// Generic plugin library loader using dynamic library loading
pub struct PluginLibraryLoader;

impl PluginLibraryLoader {
    /// Load a plugin from a shared library file.
    ///
    /// The returned `Arc<dyn Plugin>` keeps the `Library` alive so that
    /// vtable pointers remain valid for the entire lifetime of the plugin.
    pub async fn load_from_file(path: &Path) -> Result<Arc<dyn Plugin>> {
        let path_str = path.to_string_lossy().to_string();
        
        // SAFETY: We immediately wrap the Library in LibraryBoundPlugin so
        // its lifetime is tied to the returned Arc — no dangling vtables.
        let library = unsafe {
            libloading::Library::new(&path_str)
                .map_err(|e| PluginError::LoadFailed(format!("Failed to load library {}: {}", path_str, e)))?
        };
        
        // Get the plugin factory symbol — borrow from `library` before we move it.
        let plugin = {
            let factory: libloading::Symbol<PluginFactory> = unsafe {
                library.get(b"rcode_plugin_create")
                    .map_err(|e| PluginError::SymbolNotFound(format!("Failed to get factory symbol: {}", e)))?
            };
            
            // Call the factory to create the plugin instance
            factory()
                .map_err(|e| PluginError::InitFailed(format!("Factory call failed: {}", e)))?
        };
        
        info!("Loaded plugin from: {}", path_str);
        
        // Wrap the plugin so the Library stays alive
        Ok(Arc::new(LibraryBoundPlugin::new(plugin, library)))
    }
}

