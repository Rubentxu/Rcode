//! Credentials handling for AI providers

use std::env;

use opencode_core::error::{OpenCodeError, Result};

/// Load API key from environment variable
/// Format: {PROVIDER}_API_KEY (e.g., ANTHROPIC_API_KEY, OPENAI_API_KEY)
pub fn load_api_key(provider: &str) -> Result<String> {
    let env_var = format!("{}_API_KEY", provider.to_uppercase());
    env::var(&env_var).map_err(|_| {
        OpenCodeError::Config(format!(
            "Missing {}. Set the {} environment variable.",
            env_var, env_var
        ))
    })
}

/// Load API key with fallback to config value
pub fn resolve_api_key(provider: &str, config_key: Option<&str>) -> Result<String> {
    // First try environment variable
    if let Ok(key) = load_api_key(provider) {
        return Ok(key);
    }

    // Then try config value
    if let Some(key) = config_key {
        if !key.is_empty() {
            return Ok(key.to_string());
        }
    }

    // Return error with helpful message
    let env_var = format!("{}_API_KEY", provider.to_uppercase());
    Err(OpenCodeError::Config(format!(
        "No API key found for {}. Set the {} environment variable or provide api_key in config.",
        provider, env_var
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_api_key_from_env() {
        std::env::set_var("TEST_PROVIDER_API_KEY", "test-key-123");
        let result = load_api_key("test_provider");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-key-123");
        std::env::remove_var("TEST_PROVIDER_API_KEY");
    }

    #[test]
    fn test_load_api_key_missing() {
        std::env::remove_var("NONEXISTENT_API_KEY");
        let result = load_api_key("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_api_key_env_first() {
        std::env::set_var("TEST_RESOLVE_API_KEY", "env-key");
        let result = resolve_api_key("test_resolve", Some("config-key"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "env-key");
        std::env::remove_var("TEST_RESOLVE_API_KEY");
    }

    #[test]
    fn test_resolve_api_key_fallback_to_config() {
        std::env::remove_var("TEST_FALLBACK_API_KEY");
        let result = resolve_api_key("test_fallback", Some("config-key"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "config-key");
    }
}
