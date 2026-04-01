//! Configuration file loading

use std::path::PathBuf;

use super::Result;
use rcode_core::OpencodeConfig;

/// Load configuration from file or return default
pub fn load_config(config_path: Option<PathBuf>, no_config: bool) -> Result<OpencodeConfig> {
    if no_config {
        return Ok(OpencodeConfig::default());
    }

    // Try explicit path first
    if let Some(path) = config_path {
        if path.exists() {
            return load_from_file(&path);
        }
        tracing::warn!("Config file {:?} not found, using defaults", path);
        return Ok(OpencodeConfig::default());
    }

    // Try current directory
    let local_path = PathBuf::from("opencode.json");
    if local_path.exists() {
        return load_from_file(&local_path);
    }

    // Try ~/.config/opencode/
    if let Some(config_dir) = dirs::config_dir() {
        let global_path = config_dir.join("opencode").join("opencode.json");
        if global_path.exists() {
            return load_from_file(&global_path);
        }
    }

    tracing::debug!("No config file found, using defaults");
    Ok(OpencodeConfig::default())
}

fn load_from_file(path: &PathBuf) -> Result<OpencodeConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: OpencodeConfig = serde_json::from_str(&content)?;
    Ok(config)
}

/// Substitute environment variables in config values
/// Supports ${VAR_NAME} syntax
pub fn substitute_env_vars(value: &str) -> String {
    let mut result = value.to_string();
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            if let Ok(var_value) = std::env::var(var_name) {
                result = format!(
                    "{}{}{}",
                    &result[..start],
                    var_value,
                    &result[start + end + 1..]
                );
            } else {
                // Keep the original if env var not found
                break;
            }
        } else {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_env_vars() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TEST_API_KEY", "secret123");
            assert_eq!(substitute_env_vars("${TEST_API_KEY}"), "secret123");
            std::env::remove_var("TEST_API_KEY");
        }
    }

    #[test]
    fn test_substitute_env_vars_not_found() {
        assert_eq!(
            substitute_env_vars("${NONEXISTENT_VAR}"),
            "${NONEXISTENT_VAR}"
        );
    }
}
