//! OpenCode-style configuration file loading
//!
//! Implements cascading config merge from multiple sources:
//! 1. ~/.config/opencode/config.json
//! 2. ~/.config/opencode/opencode.json
//! 3. ~/.config/opencode/opencode.jsonc
//! 4. Project .opencode/ configs (opencode.jsonc, opencode.json)
//! 5. OPENCODE_CONFIG file (CLI flag)
//! 6. OPENCODE_CONFIG_CONTENT env var
//!
//! Supports JSONC (JSON with comments) via json_comments crate.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use super::{OpencodeConfig, AgentConfig};

fn strip_json_comments(input: &str) -> String {
    use std::io::Read;
    let mut stripped = json_comments::StripComments::new(input.as_bytes());
    let mut result = String::new();
    stripped.read_to_string(&mut result).unwrap_or_default();
    result
}

fn parse_jsonc<T: serde::de::DeserializeOwned>(content: &str) -> Result<T> {
    let stripped = strip_json_comments(content);
    serde_json::from_str(&stripped).context("Failed to parse JSON")
}

fn deep_merge(target: &mut serde_json::Value, source: &serde_json::Value) {
    match source {
        serde_json::Value::Object(source_map) => {
            if !target.is_object() {
                *target = serde_json::Value::Object(serde_json::Map::new());
            }
            let target_map = target.as_object_mut().unwrap();
            for (key, value) in source_map {
                if value.is_null() {
                    continue;
                }
                if value.is_array() {
                    let target_array = target_map.entry(key).or_insert_with(|| serde_json::Value::Array(vec![]));
                    if let serde_json::Value::Array(source_vec) = value {
                        if let serde_json::Value::Array(target_vec) = target_array {
                            let combined: Vec<_> = target_vec.iter().chain(source_vec.iter()).cloned().collect();
                            let unique: Vec<_> = combined.into_iter().collect::<std::collections::HashSet<_>>().into_iter().collect();
                            *target_array = serde_json::Value::Array(unique);
                        } else {
                            *target_array = value.clone();
                        }
                    } else {
                        *target_array = value.clone();
                    }
                } else if value.is_string() || value.is_number() || value.is_boolean() {
                    target_map.insert(key.clone(), value.clone());
                } else {
                    deep_merge(target_map.entry(key).or_insert_with(|| serde_json::Value::Object(serde_json::Map::new())), value);
                }
            }
        }
        _ => {
            if !source.is_null() {
                *target = source.clone();
            }
        }
    }
}

fn merge_configs(base: &OpencodeConfig, overlay: &OpencodeConfig) -> OpencodeConfig {
    let base_json = serde_json::to_value(base).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let overlay_json = serde_json::to_value(overlay).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    
    let mut merged = base_json;
    deep_merge(&mut merged, &overlay_json);
    
    serde_json::from_value(merged).unwrap_or_else(|_| overlay.clone())
}

fn parse_config_file(content: &str) -> Result<OpencodeConfig> {
    parse_jsonc(content)
}

fn load_config_file(path: &Path) -> Result<OpencodeConfig> {
    let content = std::fs::read_to_string(path).context(format!("Failed to read config file: {:?}", path))?;
    parse_config_file(&content)
}

fn config_file_exists(path: &Path) -> bool {
    path.exists()
}

fn get_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("opencode"))
}

pub fn get_managed_config_dir() -> Option<PathBuf> {
    get_config_dir().map(|p| p.join("managed"))
}

fn get_project_config_dirs(work_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![];
    
    for ancestor in work_dir.ancestors() {
        let opencode_dir = ancestor.join(".opencode");
        if opencode_dir.is_dir() {
            dirs.push(opencode_dir);
        }
        if ancestor.join(".git").is_dir() || ancestor.join(".git").exists() {
            break;
        }
    }
    
    dirs
}

pub async fn load_config(
    config_path: Option<PathBuf>,
    no_config: bool,
    work_dir: Option<PathBuf>,
) -> Result<OpencodeConfig> {
    if no_config {
        return Ok(OpencodeConfig::default());
    }

    let mut result = OpencodeConfig::default();
    let work_directory = work_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    if let Some(config_dir) = get_config_dir() {
        let global_files = [
            config_dir.join("config.json"),
            config_dir.join("opencode.json"),
            config_dir.join("opencode.jsonc"),
        ];
        
        for file in &global_files {
            if config_file_exists(file) {
                tracing::debug!("Loading global config from {:?}", file);
                match load_config_file(file) {
                    Ok(cfg) => result = merge_configs(&result, &cfg),
                    Err(e) => tracing::warn!("Failed to load config from {:?}: {}", file, e),
                }
            }
        }
    }

    let project_dirs = get_project_config_dirs(&work_directory);
    for dir in &project_dirs {
        for file in &[dir.join("opencode.jsonc"), dir.join("opencode.json")] {
            if config_file_exists(file) {
                tracing::debug!("Loading project config from {:?}", file);
                match load_config_file(file) {
                    Ok(cfg) => result = merge_configs(&result, &cfg),
                    Err(e) => tracing::warn!("Failed to load config from {:?}: {}", file, e),
                }
            }
        }
    }

    if let Some(path) = config_path {
        if path.exists() {
            tracing::debug!("Loading config from CLI path {:?}", path);
            match load_config_file(&path) {
                Ok(cfg) => result = merge_configs(&result, &cfg),
                Err(e) => tracing::warn!("Failed to load config from {:?}: {}", path, e),
            }
        }
    }

    if let Ok(content) = std::env::var("OPENCODE_CONFIG_CONTENT") {
        if !content.is_empty() {
            tracing::debug!("Loading config from OPENCODE_CONFIG_CONTENT env var");
            match parse_config_file(&content) {
                Ok(cfg) => result = merge_configs(&result, &cfg),
                Err(e) => tracing::warn!("Failed to parse OPENCODE_CONFIG_CONTENT: {}", e),
            }
        }
    }

    if let Some(managed_dir) = get_managed_config_dir() {
        if managed_dir.exists() {
            for file in &[managed_dir.join("opencode.jsonc"), managed_dir.join("opencode.json")] {
                if config_file_exists(file) {
                    tracing::debug!("Loading managed config from {:?}", file);
                    match load_config_file(file) {
                        Ok(cfg) => result = merge_configs(&result, &cfg),
                        Err(e) => tracing::warn!("Failed to load config from {:?}: {}", file, e),
                    }
                }
            }
        }
    }

    if let Some(ref agent_map) = result.agent {
        if let Some(ref mode_map) = result.mode {
            let mut merged_agent = agent_map.clone();
            for (name, mode_config) in mode_map {
                if !merged_agent.contains_key(name) {
                    merged_agent.insert(name.clone(), mode_config.clone());
                }
            }
            result.agent = Some(merged_agent);
        }
    } else if let Some(ref mode_map) = result.mode {
        result.agent = Some(mode_map.clone());
    }

    Ok(result)
}

pub fn resolve_model_from_config(config: &OpencodeConfig, cli_model: Option<&str>, agent_name: Option<&str>) -> Option<String> {
    if let Some(model) = cli_model.filter(|m| !m.is_empty()) {
        return Some(model.to_string());
    }

    if let Some(agent) = agent_name {
        if let Some(agent_model) = config.model_for_agent(agent) {
            return Some(agent_model.to_string());
        }
    }

    config.effective_model().map(|s| s.to_string())
}

pub fn parse_model_id(model: &str) -> (String, String) {
    let parts: Vec<&str> = model.splitn(2, '/').collect();
    if parts.len() == 2 {
        (parts[0].to_string(), parts[1].to_string())
    } else {
        let model_lower = model.to_lowercase();
        if model_lower.starts_with("gpt-") || model_lower.starts_with("o1") || model_lower.starts_with("o3") {
            ("openai".to_string(), model.to_string())
        } else {
            ("anthropic".to_string(), model.to_string())
        }
    }
}

/// Resolve the config path to save to disk.
///
/// Priority (first existing path wins):
///   1. Project-level .opencode/opencode.json
///   2. Global ~/.config/opencode/opencode.json  (same dir load_config reads)
///
/// Falls back to creating ~/.config/opencode/opencode.json when none exist.
fn resolve_config_path() -> Option<PathBuf> {
    // Project-level first
    let candidates = [
        ".opencode/opencode.json",
        ".opencode/opencode.jsonc",
    ];
    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return Some(PathBuf::from(candidate));
        }
    }

    // Global config — use the same directory that load_config reads from so
    // saved keys are picked up on the next server restart.
    if let Some(config_dir) = dirs::config_dir() {
        let global = config_dir.join("opencode").join("opencode.json");
        if global.exists() {
            return Some(global);
        }
    }

    None
}

/// Save config to disk at the resolved config path.
///
/// Only writes the `providers` section (api_key / base_url) on top of the
/// existing file so we don't overwrite unrelated settings (agents, models…).
pub fn save_config(config: &OpencodeConfig) -> Result<(), String> {
    let config_path = resolve_config_path()
        .unwrap_or_else(|| {
            // Default: create ~/.config/opencode/opencode.json
            let dir = dirs::config_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
                .join("opencode");
            let _ = std::fs::create_dir_all(&dir);
            dir.join("opencode.json")
        });

    // Read the existing file so we don't clobber unrelated fields
    let mut existing: serde_json::Value = if config_path.exists() {
        let raw = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read existing config {:?}: {}", config_path, e))?;
        let stripped = {
            use std::io::Read;
            let mut s = json_comments::StripComments::new(raw.as_bytes());
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap_or_default();
            out
        };
        serde_json::from_str(&stripped).unwrap_or(serde_json::Value::Object(Default::default()))
    } else {
        serde_json::Value::Object(Default::default())
    };

    // Merge only the providers section from the in-memory config
    if !config.providers.is_empty() {
        let providers_json = serde_json::to_value(&config.providers)
            .map_err(|e| format!("Failed to serialize providers: {}", e))?;
        if let Some(obj) = existing.as_object_mut() {
            obj.insert("providers".to_string(), providers_json);
        }
    }

    let json = serde_json::to_string_pretty(&existing)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    std::fs::write(&config_path, json)
        .map_err(|e| format!("Failed to write config to {:?}: {}", config_path, e))?;

    tracing::info!("Config saved to {:?}", config_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_strip_json_comments() {
        let jsonc = r#"{
            // This is a comment
            "model": "anthropic/claude-3-5-sonnet",
            /* Block comment */
            "server": {
                "port": 4096
            }
        }"#;
        
        let stripped = strip_json_comments(jsonc);
        let config: OpencodeConfig = serde_json::from_str(&stripped).unwrap();
        assert_eq!(config.model, Some("anthropic/claude-3-5-sonnet".to_string()));
    }

    #[tokio::test]
    async fn test_load_config_from_jsonc_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("opencode.jsonc");
        
        let config_content = r#"{
            // Comment test
            "model": "anthropic/claude-3-5-sonnet",
            "server": {
                "port": 8080
            }
        }"#;
        
        fs::write(&config_path, config_content).unwrap();
        let config = load_config(Some(config_path), false, Some(temp.path().to_path_buf())).await.unwrap();
        
        assert_eq!(config.model, Some("anthropic/claude-3-5-sonnet".to_string()));
    }

    #[tokio::test]
    async fn test_merge_configs_arrays_concat() {
        let temp = TempDir::new().unwrap();
        
        let config1_path = temp.path().join("config1.json");
        let config2_path = temp.path().join("config2.json");
        
        fs::write(&config1_path, r#"{
            "plugin": ["plugin-a", "plugin-b"],
            "model": "anthropic/claude-3-5-sonnet"
        }"#).unwrap();
        
        fs::write(&config2_path, r#"{
            "plugin": ["plugin-b", "plugin-c"],
            "default_agent": "build"
        }"#).unwrap();
        
        let cfg1 = load_config(Some(config1_path.clone()), false, Some(temp.path().to_path_buf())).await.unwrap();
        let cfg2 = load_config(Some(config2_path.clone()), false, Some(temp.path().to_path_buf())).await.unwrap();
        
        let merged = merge_configs(&cfg1, &cfg2);
        assert_eq!(merged.model, Some("anthropic/claude-3-5-sonnet".to_string()));
        assert_eq!(merged.default_agent, Some("build".to_string()));
    }

    #[tokio::test]
    async fn test_parse_model_id() {
        assert_eq!(parse_model_id("anthropic/claude-3-5-sonnet"), ("anthropic".to_string(), "claude-3-5-sonnet".to_string()));
        assert_eq!(parse_model_id("gpt-4o"), ("openai".to_string(), "gpt-4o".to_string()));
        assert_eq!(parse_model_id("openai/gpt-4o"), ("openai".to_string(), "gpt-4o".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_model_prefers_cli() {
        let mut config = OpencodeConfig::default();
        config.model = Some("anthropic/claude-3-5-sonnet".to_string());
        
        let resolved = resolve_model_from_config(&config, Some("openai/gpt-4o"), None);
        assert_eq!(resolved, Some("openai/gpt-4o".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_model_prefers_agent_specific() {
        let mut config = OpencodeConfig::default();
        config.model = Some("anthropic/claude-3-5-sonnet".to_string());
        
        let mut agent_config = AgentConfig::default();
        agent_config.model = Some("openai/gpt-4o".to_string());
        
        let mut agents = HashMap::new();
        agents.insert("custom".to_string(), agent_config);
        config.agent = Some(agents);
        
        let resolved = resolve_model_from_config(&config, None, Some("custom"));
        assert_eq!(resolved, Some("openai/gpt-4o".to_string()));
    }

    #[tokio::test]
    async fn test_cascade_merging() {
        let temp = TempDir::new().unwrap();
        let global_path = temp.path().join("opencode.json");
        
        fs::write(&global_path, r#"{
            "model": "anthropic/claude-base",
            "log_level": "INFO"
        }"#).unwrap();
        
        let result = load_config(Some(global_path), false, Some(temp.path().to_path_buf())).await.unwrap();
        assert_eq!(result.model, Some("anthropic/claude-base".to_string()));
    }
}
