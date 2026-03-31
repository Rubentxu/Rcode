//! Plugin manifest handling

use serde::{Deserialize, Serialize};
use std::path::Path;

use super::{PluginError, PluginMetadata, Result};

/// Plugin manifest as defined in plugin.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    pub main: String,
}

impl PluginManifest {
    /// Load a plugin manifest from a directory
    pub fn from_dir(dir: &Path) -> Result<Self> {
        let manifest_path = dir.join("plugin.json");
        Self::from_path(&manifest_path)
    }

    /// Load a plugin manifest from a specific path
    pub fn from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            PluginError::InvalidManifest(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let manifest: PluginManifest = serde_json::from_str(&content).map_err(|e| {
            PluginError::InvalidManifest(format!("Failed to parse {}: {}", path.display(), e))
        })?;

        manifest.validate()?;

        Ok(manifest)
    }

    /// Validate the manifest contents
    fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(PluginError::InvalidManifest(
                "Plugin id cannot be empty".to_string(),
            ));
        }
        if self.name.is_empty() {
            return Err(PluginError::InvalidManifest(
                "Plugin name cannot be empty".to_string(),
            ));
        }
        if self.version.is_empty() {
            return Err(PluginError::InvalidManifest(
                "Plugin version cannot be empty".to_string(),
            ));
        }
        if self.main.is_empty() {
            return Err(PluginError::InvalidManifest(
                "Plugin main entry cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    /// Convert manifest to metadata
    pub fn to_metadata(&self, path: std::path::PathBuf) -> PluginMetadata {
        PluginMetadata {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            description: self.description.clone(),
            main: self.main.clone(),
            path,
        }
    }
}
