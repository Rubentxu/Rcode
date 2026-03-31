//! Configuration file loading

use anyhow::{Context, Result};
use std::path::PathBuf;

use super::OpencodeConfig;

/// Load configuration from file or return default
pub async fn load_config(config_path: Option<PathBuf>, no_config: bool) -> Result<OpencodeConfig> {
    if no_config {
        return Ok(OpencodeConfig::default());
    }

    // Try explicit path first
    if let Some(path) = config_path {
        if path.exists() {
            return load_from_file(&path).context(format!("Failed to load config from {:?}", path));
        }
        tracing::warn!("Config file {:?} not found, using defaults", path);
        return Ok(OpencodeConfig::default());
    }

    // Try current directory
    let local_path = PathBuf::from("opencode.json");
    if local_path.exists() {
        return load_from_file(&local_path).context("Failed to load config from ./opencode.json");
    }

    // Try ~/.config/opencode/
    if let Some(config_dir) = dirs::config_dir() {
        let global_path = config_dir.join("opencode").join("opencode.json");
        if global_path.exists() {
            return load_from_file(&global_path)
                .context(format!("Failed to load config from {:?}", global_path));
        }
    }

    tracing::debug!("No config file found, using defaults");
    Ok(OpencodeConfig::default())
}

fn load_from_file(path: &PathBuf) -> Result<OpencodeConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: OpencodeConfig = serde_json::from_str(&content)
        .context("Failed to parse config JSON")?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_load_config_from_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("opencode.json");
        
        let config_content = r#"{
            "providers": {
                "default": "anthropic",
                "anthropic": {
                    "api_key": "test-key",
                    "model": "claude-3"
                }
            },
            "session": {
                "history_limit": 50,
                "token_budget": 50000
            },
            "tools": {
                "timeout_seconds": 60,
                "allowed_paths": ["/tmp"]
            },
            "server": {
                "host": "0.0.0.0",
                "port": 8080,
                "cors": {
                    "allow_origin": "*",
                    "allow_methods": "GET,POST",
                    "allow_headers": "Content-Type"
                }
            }
        }"#;
        
        std::fs::write(&config_path, config_content).unwrap();
        let config = load_config(Some(config_path), false).await.unwrap();
        
        assert_eq!(config.providers.default, "anthropic");
        assert_eq!(config.session.history_limit, 50);
        assert_eq!(config.tools.timeout_seconds, 60);
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.cors.allow_origin, "*");
    }

    #[tokio::test]
    async fn test_no_config_returns_default() {
        let config = load_config(None, true).await.unwrap();
        assert_eq!(config.providers.default, "anthropic");
        assert_eq!(config.session.history_limit, 100);
    }
}
