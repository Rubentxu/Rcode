//! Credentials handling for AI providers

use std::env;

use rcode_core::error::{RCodeError, Result};

/// Load API key from environment variable
/// Format: {PROVIDER}_API_KEY (e.g., ANTHROPIC_API_KEY, OPENAI_API_KEY)
/// Also checks {PROVIDER}_AUTH_TOKEN for MiniMax compatibility
pub fn load_api_key(provider: &str) -> Result<String> {
    let env_var_api_key = format!("{}_API_KEY", provider.to_uppercase());
    if let Ok(key) = env::var(&env_var_api_key) {
        return Ok(key);
    }

    let env_var_auth_token = format!("{}_AUTH_TOKEN", provider.to_uppercase());
    if let Ok(key) = env::var(&env_var_auth_token) {
        return Ok(key);
    }

    Err(RCodeError::Config(format!(
        "Missing {}_API_KEY or {}_AUTH_TOKEN. Set one of these environment variables.",
        provider.to_uppercase(),
        provider.to_uppercase()
    )))
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
    let provider_upper = provider.to_uppercase();
    Err(RCodeError::Config(format!(
        "No API key found for {}. Set the {}_API_KEY or {}_AUTH_TOKEN environment variable, or provide api_key in config.",
        provider, provider_upper, provider_upper
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_api_key_from_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TEST_PROVIDER_API_KEY", "test-key-123");
            let result = load_api_key("test_provider");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "test-key-123");
            std::env::remove_var("TEST_PROVIDER_API_KEY");
        }
    }

    #[test]
    fn test_load_api_key_missing() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("NONEXISTENT_API_KEY");
            let result = load_api_key("nonexistent");
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_resolve_api_key_env_first() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TEST_RESOLVE_API_KEY", "env-key");
            let result = resolve_api_key("test_resolve", Some("config-key"));
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "env-key");
            std::env::remove_var("TEST_RESOLVE_API_KEY");
        }
    }

    #[test]
    fn test_resolve_api_key_fallback_to_config() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("TEST_FALLBACK_API_KEY");
            let result = resolve_api_key("test_fallback", Some("config-key"));
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "config-key");
        }
    }

    #[test]
    fn test_load_api_key_falls_back_to_auth_token() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("TEST_AUTHTOKEN_API_KEY");
            std::env::set_var("TEST_AUTHTOKEN_AUTH_TOKEN", "jwt-token-123");
            let result = load_api_key("test_authtoken");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "jwt-token-123");
            std::env::remove_var("TEST_AUTHTOKEN_AUTH_TOKEN");
        }
    }

    #[test]
    fn test_load_api_key_prefers_api_key_over_auth_token() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TEST_PREAUTH_API_KEY", "api-key-456");
            std::env::set_var("TEST_PREAUTH_AUTH_TOKEN", "jwt-token-789");
            let result = load_api_key("test_preauth");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "api-key-456");
            std::env::remove_var("TEST_PREAUTH_API_KEY");
            std::env::remove_var("TEST_PREAUTH_AUTH_TOKEN");
        }
    }
}
