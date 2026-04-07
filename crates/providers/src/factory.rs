//! Provider factory for building LLM providers from model strings
//!
//! This module provides a centralized way to create providers with proper
/// configuration and API key resolution.
use std::sync::Arc;

use rcode_core::error::{RCodeError, Result};
use rcode_core::RcodeConfig;

use super::anthropic::AnthropicProvider;
use super::google::GoogleProvider;
use super::minimax::MiniMaxProvider;
use super::openai::OpenAIProvider;
use super::openrouter::OpenRouterProvider;
use super::zai::ZaiProvider;
use super::{parse_model_id, LlmProvider};

/// Information about an available model
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    /// Full model identifier (e.g., "anthropic/claude-3-5-sonnet-20241022")
    pub id: String,
    /// Provider name (e.g., "anthropic", "openai")
    pub provider: String,
    /// Whether this model is enabled based on config
    pub enabled: bool,
}

/// Known providers and their supported models
const KNOWN_PROVIDERS: &[(&str, &[&str])] = &[
    (
        "anthropic",
        &[
            "claude-3-5-sonnet-20241022",
            "claude-3-5-haiku-20241022",
            "claude-3-opus-20240229",
            "claude-3-sonnet-20240229",
            "claude-3-haiku-20240307",
        ],
    ),
    (
        "openai",
        &[
            "gpt-4o-2024-11-20",
            "gpt-4o-mini-2024-07-18",
            "gpt-4-turbo-2024-04-09",
            "gpt-4o",
            "gpt-4o-mini",
            "o1-preview",
            "o1-mini",
            "o3",
        ],
    ),
    (
        "google",
        &[
            "gemini-1.5-pro-002",
            "gemini-1.5-flash-002",
            "gemini-1.5-pro",
            "gemini-1.5-flash",
            "gemini-2.0-flash-exp",
            "gemini-exp-1206",
        ],
    ),
    (
        "openrouter",
        &[
            "anthropic/claude-3.5-sonnet",
            "openai/gpt-4o",
            "google/gemini-pro",
        ],
    ),
    ("minimax", &["MiniMax-Text-01", "minimax-01"]),
    (
        "zai",
        &[
            "zai-coding-plan",
            "zai-coding-standard",
            "zai-coding-premium",
        ],
    ),
];

/// Factory for creating LLM providers from model identifiers
pub struct ProviderFactory;

impl ProviderFactory {
    /// List all available models with their enabled status based on config.
    ///
    /// Returns a list of ModelInfo for all known models, with `enabled` indicating
    /// whether the model is available given the current config (provider not disabled,
    /// or provider is in enabled_providers list if specified).
    pub fn list_models(config: &RcodeConfig) -> Vec<ModelInfo> {
        let mut models = Vec::new();

        // Check if enabled_providers is set
        let enabled_providers: Option<&[String]> = config.enabled_providers.as_deref();

        // Check disabled providers
        let disabled_providers: Option<&[String]> = config.disabled_providers.as_deref();

        for (provider, model_list) in KNOWN_PROVIDERS {
            // Skip if provider is disabled
            if let Some(disabled) = disabled_providers
                && disabled.contains(&provider.to_string())
            {
                continue;
            }

            // Skip if enabled_providers is set and this provider is not in it
            if let Some(enabled) = enabled_providers
                && !enabled.is_empty() && !enabled.contains(&provider.to_string())
            {
                continue;
            }

            for model in *model_list {
                let model_id = format!("{}/{}", provider, model);
                let enabled = true; // If we got here, the provider is enabled
                models.push(ModelInfo {
                    id: model_id,
                    provider: provider.to_string(),
                    enabled,
                });
            }
        }

        models
    }

    /// Build a provider from a model string like "anthropic/claude-sonnet-4-5"
    ///
    /// Optionally passes RcodeConfig to read credentials and enabled/disabled providers.
    ///
    /// # Arguments
    /// * `model` - Model identifier in format "provider/model" or just "model"
    /// * `config` - Optional RcodeConfig for reading API keys and provider settings
    ///
    /// # Returns
    /// A tuple of (provider, model_name) where provider is an Arc<dyn LlmProvider>
    pub fn build(
        model: &str,
        config: Option<&RcodeConfig>,
    ) -> Result<(Arc<dyn LlmProvider>, String)> {
        let (provider_id, model_name) = parse_model_id(model);

        // Check if provider is disabled
        if let Some(cfg) = config {
            if let Some(disabled) = cfg.disabled_providers.as_ref()
                && disabled.contains(&provider_id)
            {
                return Err(RCodeError::Config(format!(
                    "Provider '{}' is disabled",
                    provider_id
                )));
            }

            // Check if enabled_providers is set and this provider is not in it
            if let Some(enabled) = cfg.enabled_providers.as_ref()
                && !enabled.is_empty() && !enabled.contains(&provider_id)
            {
                return Err(RCodeError::Config(format!(
                    "Provider '{}' is not enabled",
                    provider_id
                )));
            }
        }

        // Extract API key from config if available
        let config_api_key = get_api_key_from_config(config, &provider_id);

        match provider_id.as_str() {
            "anthropic" => {
                let api_key = resolve_api_key("anthropic", config_api_key)?;
                Ok((Arc::new(AnthropicProvider::new(api_key)), model_name))
            }
            "openai" => {
                let api_key = resolve_api_key("openai", config_api_key)?;
                let base_url = get_provider_config_string(config, "openai", "base_url");
                match base_url {
                    Some(url) => Ok((
                        Arc::new(OpenAIProvider::new_with_base_url(api_key, url)),
                        model_name,
                    )),
                    None => Ok((Arc::new(OpenAIProvider::new(api_key)), model_name)),
                }
            }
            "google" => {
                let api_key = resolve_api_key("google", config_api_key)?;
                Ok((Arc::new(GoogleProvider::new(api_key)), model_name))
            }
            "openrouter" => {
                let api_key = resolve_api_key("openrouter", config_api_key)?;
                let base_url = get_provider_config_string(config, "openrouter", "base_url");
                match base_url {
                    Some(url) => Ok((
                        Arc::new(OpenRouterProvider::new_with_base_url(api_key, url)),
                        model_name,
                    )),
                    None => Ok((Arc::new(OpenRouterProvider::new(api_key)), model_name)),
                }
            }
            "minimax" => {
                let api_key = resolve_api_key("minimax", config_api_key)?;
                let base_url = get_provider_config_string(config, "minimax", "base_url");
                match base_url {
                    Some(url) => Ok((
                        Arc::new(MiniMaxProvider::new_with_base_url(api_key, url)),
                        model_name,
                    )),
                    None => Ok((Arc::new(MiniMaxProvider::new(api_key)), model_name)),
                }
            }
            "zai" => {
                let api_key = resolve_api_key("zai", config_api_key)?;
                let base_url = get_provider_config_string(config, "zai", "base_url");
                match base_url {
                    Some(url) => Ok((
                        Arc::new(ZaiProvider::new_with_base_url(api_key, url)),
                        model_name,
                    )),
                    None => Ok((Arc::new(ZaiProvider::new(api_key)), model_name)),
                }
            }
            other => {
                // CUSTOM PROVIDER SUPPORT
                // Check if config has providers.<unknown_id>.api_key and providers.<unknown_id>.base_url
                // Env var names replace hyphens with underscores
                let env_provider_id = other.to_uppercase().replace('-', "_");
                // First check auth.json (primary credential store)
                let custom_api_key = rcode_core::auth::get_api_key(other)
                    .or_else(|| get_provider_config_string(config, other, "api_key"))
                    .or_else(|| std::env::var(format!("{}_API_KEY", env_provider_id)).ok());
                let custom_base_url = get_provider_config_string(config, other, "base_url")
                    .or_else(|| std::env::var(format!("{}_BASE_URL", env_provider_id)).ok());

                match (custom_api_key, custom_base_url) {
                    (Some(key), Some(url)) => {
                        Ok((Arc::new(OpenAIProvider::new_with_base_url(key, url)), model_name))
                    }
                    _ => Err(RCodeError::Config(format!(
                        "Unknown provider '{}'. Configure providers.{}.api_key and providers.{}.base_url in config, or set {}_API_KEY and {}_BASE_URL environment variables.",
                        other, other, other, env_provider_id, env_provider_id
                    ))),
                }
            }
        }
    }
}

/// Resolve API key from environment or config
fn resolve_api_key(provider: &str, config_key: Option<String>) -> Result<String> {
    use super::load_api_key;

    // First try environment variable
    if let Ok(key) = load_api_key(provider) {
        return Ok(key);
    }

    // Then try config value
    if let Some(key) = config_key {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    // Return error with helpful message
    let provider_upper = provider.to_uppercase();
    Err(RCodeError::Config(format!(
        "No API key found for {}. Set the {}_API_KEY or {}_AUTH_TOKEN environment variable, or provide api_key in config.",
        provider, provider_upper, provider_upper
    )))
}

/// Get a string config value from nested provider config
///
/// Extracts values from config.extra like:
/// ```json
/// {
///   "providers": {
///     "provider_name": {
///       "key": "value"
///     }
///   }
/// }
/// ```
fn get_provider_config_string(
    config: Option<&RcodeConfig>,
    provider_id: &str,
    key: &str,
) -> Option<String> {
    // Try typed field first
    if let Some(provider) = config.and_then(|c| c.providers.get(provider_id)) {
        match key {
            "api_key" => return provider.api_key.clone(),
            "base_url" => return provider.base_url.clone(),
            _ => {}
        }
    }
    // Fallback to extra JSON
    config.and_then(|cfg| {
        cfg.extra
            .get("providers")
            .and_then(|p| p.get(provider_id))
            .and_then(|p| p.get(key))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    })
}

/// Get API key from config, checking auth.json first (OpenCode-compatible)
///
/// Resolution order:
/// 1. auth.json (primary - rcode_core::auth::get_api_key)
/// 2. Typed providers config
/// 3. Legacy extra JSON config
fn get_api_key_from_config(config: Option<&RcodeConfig>, provider_id: &str) -> Option<String> {
    // First check auth.json (OpenCode's primary credential store)
    if let Some(key) = rcode_core::auth::get_api_key(provider_id) {
        return Some(key);
    }

    // Then try typed field
    if let Some(provider) = config.and_then(|c| c.providers.get(provider_id)) {
        if provider.api_key.is_some() {
            return provider.api_key.clone();
        }
    }
    // Fallback to extra JSON
    config.and_then(|cfg| {
        cfg.extra
            .get("providers")
            .and_then(|p| p.get(provider_id))
            .and_then(|p| p.get("api_key"))
            .and_then(|k| k.as_str())
            .map(|s| s.to_string())
    })
}

/// Build a provider from a model string (deprecated, use ProviderFactory::build instead)
///
/// This function is kept for backward compatibility.
#[deprecated(note = "Use ProviderFactory::build instead")]
pub fn build_provider_from_model(
    model: &str,
    config: Option<&RcodeConfig>,
) -> Result<(Arc<dyn LlmProvider>, String)> {
    ProviderFactory::build(model, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_config_with_providers(
        api_keys: serde_json::Value,
        disabled: Option<Vec<&str>>,
        enabled: Option<Vec<&str>>,
    ) -> RcodeConfig {
        RcodeConfig {
            extra: json!({ "providers": api_keys }),
            disabled_providers: disabled.map(|v| v.into_iter().map(String::from).collect()),
            enabled_providers: enabled.map(|v| v.into_iter().map(String::from).collect()),
            ..Default::default()
        }
    }

    #[test]
    fn test_factory_disabled_provider() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }

        let config = create_config_with_providers(json!({}), Some(vec!["anthropic"]), None);

        let result = ProviderFactory::build("anthropic/claude-3-5-sonnet", Some(&config));
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("disabled"));
    }

    #[test]
    fn test_factory_enabled_providers_filters() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("OPENAI_API_KEY", "test-key");
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }

        let config = create_config_with_providers(json!({}), None, Some(vec!["openai"]));

        // Anthropic should fail because it's not in enabled_providers
        let result = ProviderFactory::build("anthropic/claude-3-5-sonnet", Some(&config));
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("not enabled"));

        // OpenAI should work because it's in enabled_providers
        let result = ProviderFactory::build("openai/gpt-4o", Some(&config));
        assert!(result.is_ok());
    }

    #[test]
    fn test_factory_unsupported_provider() {
        let config = RcodeConfig::default();
        let result = ProviderFactory::build("fakeprovider/some-model", Some(&config));
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("Unknown provider"));
    }

    #[test]
    fn test_factory_google_provider() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("GOOGLE_API_KEY", "test-key");
        }
        let config = RcodeConfig::default();
        let result = ProviderFactory::build("google/gemini-2.0-flash", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "google");
        assert_eq!(model_name, "gemini-2.0-flash");
    }

    #[test]
    fn test_factory_openrouter_provider() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("OPENROUTER_API_KEY", "test-key");
        }
        let config = RcodeConfig::default();
        let result =
            ProviderFactory::build("openrouter/anthropic/claude-3-5-sonnet", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "openrouter");
        assert_eq!(model_name, "anthropic/claude-3-5-sonnet");
    }

    #[test]
    fn test_factory_anthropic_provider() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }
        let config = RcodeConfig::default();
        let result = ProviderFactory::build("anthropic/claude-3-5-sonnet", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "anthropic");
        assert_eq!(model_name, "claude-3-5-sonnet");
    }

    #[test]
    fn test_factory_openai_provider() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("OPENAI_API_KEY", "test-key");
        }
        let config = RcodeConfig::default();
        let result = ProviderFactory::build("openai/gpt-4o", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "openai");
        assert_eq!(model_name, "gpt-4o");
    }

    // ============ MiniMax Provider Tests ============

    #[test]
    fn test_factory_minimax_provider() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("MINIMAX_API_KEY", "test-minimax-key");
        }
        let config = RcodeConfig::default();
        let result = ProviderFactory::build("minimax/MiniMax-Text-01", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "minimax");
        assert_eq!(model_name, "MiniMax-Text-01");
    }

    #[test]
    fn test_factory_minimax_provider_with_config_api_key() {
        let config = create_config_with_providers(
            json!({
                "minimax": {
                    "api_key": "config-minimax-key"
                }
            }),
            None,
            None,
        );
        let result = ProviderFactory::build("minimax/minimax-01", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "minimax");
        assert_eq!(model_name, "minimax-01");
    }

    #[test]
    fn test_factory_minimax_provider_with_custom_base_url() {
        let config = create_config_with_providers(
            json!({
                "minimax": {
                    "api_key": "test-key",
                    "base_url": "https://custom.minimax.example.com/v1"
                }
            }),
            None,
            None,
        );
        let result = ProviderFactory::build("minimax/MiniMax-Text-01", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "minimax");
        assert_eq!(model_name, "MiniMax-Text-01");
    }

    // ============ ZAI Provider Tests ============

    #[test]
    fn test_factory_zai_provider() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("ZAI_API_KEY", "test-zai-key");
        }
        let config = RcodeConfig::default();
        let result = ProviderFactory::build("zai/zai-coding-plan", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "zai");
        assert_eq!(model_name, "zai-coding-plan");
    }

    #[test]
    fn test_factory_zai_provider_with_config_api_key() {
        let config = create_config_with_providers(
            json!({
                "zai": {
                    "api_key": "config-zai-key"
                }
            }),
            None,
            None,
        );
        let result = ProviderFactory::build("zai/zai-coding-standard", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "zai");
        assert_eq!(model_name, "zai-coding-standard");
    }

    #[test]
    fn test_factory_zai_provider_with_custom_base_url() {
        let config = create_config_with_providers(
            json!({
                "zai": {
                    "api_key": "test-key",
                    "base_url": "https://custom.zai.example.com/v1"
                }
            }),
            None,
            None,
        );
        let result = ProviderFactory::build("zai/zai-coding-premium", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "zai");
        assert_eq!(model_name, "zai-coding-premium");
    }

    // ============ Custom Provider Tests ============

    #[test]
    fn test_factory_custom_provider_with_config() {
        let config = create_config_with_providers(
            json!({
                "my-custom-llm": {
                    "api_key": "custom-api-key",
                    "base_url": "https://my-llm.example.com/v1"
                }
            }),
            None,
            None,
        );
        let result = ProviderFactory::build("my-custom-llm/my-model", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        // Custom providers use OpenAIProvider internally
        assert_eq!(provider.provider_id(), "openai");
        assert_eq!(model_name, "my-model");
    }

    #[test]
    fn test_factory_custom_provider_with_env_vars() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("MY_CUSTOM_LLM_API_KEY", "env-api-key");
            std::env::set_var("MY_CUSTOM_LLM_BASE_URL", "https://env-llm.example.com/v1");
        }
        let config = RcodeConfig::default();
        let result = ProviderFactory::build("my-custom-llm/my-model", Some(&config));
        assert!(result.is_ok());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "openai");
        assert_eq!(model_name, "my-model");
    }

    #[test]
    fn test_factory_custom_provider_missing_config() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("UNKNOWN_PROVIDER_API_KEY");
            std::env::remove_var("UNKNOWN_PROVIDER_BASE_URL");
        }
        let config = RcodeConfig::default();
        let result = ProviderFactory::build("unknown-provider/some-model", Some(&config));
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err
            .to_string()
            .contains("Unknown provider 'unknown-provider'"));
    }

    #[test]
    fn test_factory_custom_provider_partial_config() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("HALF_CONFIG_API_KEY");
            std::env::remove_var("HALF_CONFIG_BASE_URL");
        }
        // Only api_key, no base_url
        let config = create_config_with_providers(
            json!({
                "half-config": {
                    "api_key": "only-api-key"
                }
            }),
            None,
            None,
        );
        let result = ProviderFactory::build("half-config/my-model", Some(&config));
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("Unknown provider 'half-config'"));
    }

    // ============ get_provider_config_string Tests ============

    #[test]
    fn test_get_provider_config_string_exists() {
        let config = create_config_with_providers(
            json!({
                "test-provider": {
                    "api_key": "secret-key",
                    "base_url": "https://test.example.com"
                }
            }),
            None,
            None,
        );
        let result = get_provider_config_string(Some(&config), "test-provider", "api_key");
        assert_eq!(result, Some("secret-key".to_string()));

        let result = get_provider_config_string(Some(&config), "test-provider", "base_url");
        assert_eq!(result, Some("https://test.example.com".to_string()));
    }

    #[test]
    fn test_get_provider_config_string_missing_key() {
        let config = create_config_with_providers(
            json!({
                "test-provider": {
                    "api_key": "secret-key"
                }
            }),
            None,
            None,
        );
        let result = get_provider_config_string(Some(&config), "test-provider", "nonexistent");
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_provider_config_string_missing_provider() {
        let config = create_config_with_providers(
            json!({
                "other-provider": {
                    "api_key": "secret-key"
                }
            }),
            None,
            None,
        );
        let result = get_provider_config_string(Some(&config), "nonexistent-provider", "api_key");
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_provider_config_string_no_config() {
        let result = get_provider_config_string(None, "any-provider", "any-key");
        assert_eq!(result, None);
    }

    // ============ Example Config Documentation Test ============
    // This test documents the expected config format for custom providers

    #[test]
    fn test_example_config_format() {
        // Example config showing how to configure MiniMax, ZAI, and custom providers:
        // {
        //   "model": "zai/zai-coding-plan",
        //   "providers": {
        //     "minimax": {
        //       "api_key": "${MINIMAX_API_KEY}",
        //       "base_url": "https://api.minimax.chat/v1"
        //     },
        //     "zai": {
        //       "api_key": "${ZAI_API_KEY}",
        //       "base_url": "https://api.zai.chat/v1"
        //     },
        //     "my-custom-llm": {
        //       "api_key": "sk-xxx",
        //       "base_url": "https://my-llm.example.com/v1"
        //     }
        //   }
        // }

        let config = create_config_with_providers(
            json!({
                "minimax": {
                    "api_key": "minimax-key",
                    "base_url": "https://api.minimax.chat/v1"
                },
                "zai": {
                    "api_key": "zai-key",
                    "base_url": "https://api.zai.chat/v1"
                },
                "my-custom-llm": {
                    "api_key": "custom-key",
                    "base_url": "https://my-llm.example.com/v1"
                }
            }),
            None,
            None,
        );

        // Verify MiniMax config
        assert_eq!(
            get_provider_config_string(Some(&config), "minimax", "api_key"),
            Some("minimax-key".to_string())
        );
        assert_eq!(
            get_provider_config_string(Some(&config), "minimax", "base_url"),
            Some("https://api.minimax.chat/v1".to_string())
        );

        // Verify ZAI config
        assert_eq!(
            get_provider_config_string(Some(&config), "zai", "api_key"),
            Some("zai-key".to_string())
        );
        assert_eq!(
            get_provider_config_string(Some(&config), "zai", "base_url"),
            Some("https://api.zai.chat/v1".to_string())
        );

        // Verify custom provider config
        assert_eq!(
            get_provider_config_string(Some(&config), "my-custom-llm", "api_key"),
            Some("custom-key".to_string())
        );
        assert_eq!(
            get_provider_config_string(Some(&config), "my-custom-llm", "base_url"),
            Some("https://my-llm.example.com/v1".to_string())
        );
    }
}
