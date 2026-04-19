//! Provider factory for building LLM providers from model strings
//!
//! This module provides a centralized way to create providers with proper
//! configuration and API key resolution.
use std::sync::Arc;

use rcode_core::error::{RCodeError, Result};
use rcode_core::provider::ProviderProtocol;
use rcode_core::RcodeConfig;

use super::anthropic::AnthropicProvider;
use super::google::GoogleProvider;
use super::minimax::MiniMaxProvider;
use super::openai::OpenAIProvider;
use super::openrouter::OpenRouterProvider;
use super::zai::ZaiProvider;
use super::registry;
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
/// NOTE: This will be replaced by registry-based model listing in Phase 3.
/// Keeping for backward compatibility with list_models() until Phase 3.
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
    (
        "github-copilot",
        &["gpt-4o", "gpt-4o-mini", "claude-sonnet-4-5", "claude-3.5-sonnet", "o3-mini"],
    ),
];

/// Check if a provider has credentials configured (api_key in config or env var).
/// Returns true if the provider can be used.
fn provider_has_credentials(provider_id: &str, config: Option<&RcodeConfig>) -> bool {
    // Check env var
    let env_key = format!("{}_API_KEY", provider_id.to_uppercase().replace('-', "_"));
    if std::env::var(&env_key).is_ok() {
        return true;
    }
    let auth_key = format!("{}_AUTH_TOKEN", provider_id.to_uppercase().replace('-', "_"));
    if std::env::var(&auth_key).is_ok() {
        return true;
    }
    // Check auth.json
    if rcode_core::auth::has_credential(provider_id) {
        return true;
    }
    // Check config api_key
    config
        .and_then(|c| c.providers.get(provider_id))
        .and_then(|p| p.api_key.as_deref())
        .map(|k| !k.is_empty())
        .unwrap_or(false)
}

/// Find the single configured provider when model ID has no provider prefix.
/// If exactly one provider has credentials, returns that provider_id.
/// Otherwise returns None.
fn find_single_configured_provider(config: Option<&RcodeConfig>) -> Option<String> {
    let mut configured_providers: Vec<&str> = Vec::new();
    
    for def in registry::registry().values() {
        if provider_has_credentials(def.id, config) {
            configured_providers.push(def.id);
        }
    }
    
    // Also check providers in config that are not in registry
    if let Some(cfg) = config {
        for provider_id in cfg.providers.keys() {
            if !configured_providers.contains(&provider_id.as_str()) {
                if provider_has_credentials(provider_id, config) {
                    configured_providers.push(provider_id);
                }
            }
        }
    }
    
    if configured_providers.len() == 1 {
        Some(configured_providers[0].to_string())
    } else {
        None
    }
}

/// Factory for creating LLM providers from model identifiers
pub struct ProviderFactory;

impl ProviderFactory {
    /// List all available models with their enabled status based on config.
    ///
    /// Returns a list of ModelInfo for all known models, with `enabled` indicating
    /// whether the model is available given the current config (provider not disabled,
    /// or provider is in enabled_providers list if specified).
    ///
    /// NOTE: This will be refactored in Phase 3 to use registry-based model listing.
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
        let (mut provider_id, model_name) = parse_model_id(model);

        // Handle "unknown" provider - try to find a single configured provider
        if provider_id == "unknown" {
            if let Some(single_provider) = find_single_configured_provider(config) {
                provider_id = single_provider;
            } else {
                return Err(RCodeError::Config(
                    "Ambiguous model ID '{}': no provider prefix. Use 'provider/model' format.".to_string()
                ));
            }
        }

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

        // Look up provider in registry
        let def = registry::lookup(&provider_id);

        // Use resolve_auth() for unified auth resolution (single source of truth)
        // resolve_auth needs &RcodeConfig, so we pass a default if config is None
        let default_config = RcodeConfig::default();
        let config_ref = config.as_ref().map_or(&default_config, |v| *v);
        let auth_state = crate::resolution::resolve_auth(&provider_id, def, config_ref);

        // Resolve base_url using ProviderResolution (separate from auth)
        let resolution = crate::resolution::ProviderResolution::for_provider(&provider_id, config);
        let base_url = resolution.base_url;

        // Dispatch by protocol if provider is known
        if let Some(definition) = def {
            // For known providers, we must have an api_key
            let api_key = auth_state.api_key.ok_or_else(|| {
                let env_var = format!("{}_API_KEY", definition.env_key_prefix);
                let alt_token = format!("{}_AUTH_TOKEN", definition.env_key_prefix);
                RCodeError::Config(format!(
                    "No API key found for {}. Set the {} or {} environment variable, or provide api_key in config.",
                    provider_id, env_var, alt_token
                ))
            })?;
            return match definition.protocol {
                ProviderProtocol::OpenAiCompat => {
                    Self::build_openai_compat(&provider_id, api_key, base_url, model_name)
                }
                ProviderProtocol::AnthropicCompat => {
                    Self::build_anthropic_compat(&provider_id, api_key, base_url, model_name)
                }
                ProviderProtocol::Google => {
                    Self::build_google(api_key, base_url, model_name)
                }
            };
        }

        // Unknown provider - check if config has explicit protocol
        if let Some(cfg) = config {
            if let Some(provider_config) = cfg.providers.get(&provider_id) {
                if let Some(protocol) = &provider_config.protocol {
                    // For providers with explicit protocol, we must have an api_key
                    let api_key = auth_state.api_key.ok_or_else(|| {
                        let env_provider_id = provider_id.to_uppercase().replace('-', "_");
                        RCodeError::Config(format!(
                            "No API key found for {}. Set the {}_API_KEY environment variable, or provide api_key in config.",
                            provider_id, env_provider_id
                        ))
                    })?;
                    return match protocol {
                        ProviderProtocol::OpenAiCompat => {
                            Self::build_openai_compat(&provider_id, api_key, base_url, model_name)
                        }
                        ProviderProtocol::AnthropicCompat => {
                            Self::build_anthropic_compat(&provider_id, api_key, base_url, model_name)
                        }
                        ProviderProtocol::Google => {
                            Self::build_google(api_key, base_url, model_name)
                        }
                    };
                }
            }
        }

        // Unknown provider with no explicit protocol - check if we have both api_key and base_url
        // This is the fallback for custom providers that provide both credentials
        if let (Some(api_key), Some(base_url)) = (auth_state.api_key, base_url) {
            return Self::build_openai_compat(&provider_id, api_key, Some(base_url), model_name);
        }

        // No api_key or base_url available - error with helpful message
        let env_provider_id = provider_id.to_uppercase().replace('-', "_");
        Err(RCodeError::Config(format!(
            "Unknown provider '{}'. Configure providers.{}.api_key and providers.{}.base_url in config, or set {}_API_KEY and {}_BASE_URL environment variables.",
            provider_id, provider_id, provider_id, env_provider_id, env_provider_id
        )))
    }

    /// Build an OpenAI-compatible provider.
    fn build_openai_compat(
        provider_id: &str,
        api_key: String,
        base_url: Option<String>,
        model: String,
    ) -> Result<(Arc<dyn LlmProvider>, String)> {
        match provider_id {
            "openai" => {
                match base_url {
                    Some(url) => Ok((Arc::new(OpenAIProvider::new_with_base_url(api_key, url)), model)),
                    None => Ok((Arc::new(OpenAIProvider::new(api_key)), model)),
                }
            }
            "openrouter" => {
                match base_url {
                    Some(url) => Ok((Arc::new(OpenRouterProvider::new_with_base_url(api_key, url)), model)),
                    None => Ok((Arc::new(OpenRouterProvider::new(api_key)), model)),
                }
            }
            "minimax" => {
                match base_url {
                    Some(url) => Ok((Arc::new(MiniMaxProvider::new_with_base_url(api_key, url)), model)),
                    None => Ok((Arc::new(MiniMaxProvider::new(api_key)), model)),
                }
            }
            "zai" => {
                match base_url {
                    Some(url) => Ok((Arc::new(ZaiProvider::new_with_base_url(api_key, url)), model)),
                    None => Ok((Arc::new(ZaiProvider::new(api_key)), model)),
                }
            }
            "github-copilot" => {
                // GitHub Copilot uses an OpenAI-compatible API but its endpoint is
                // /chat/completions (no /v1/ segment). Use the dedicated constructor.
                let base_url = base_url.unwrap_or_else(|| "https://api.githubcopilot.com".to_string());
                Ok((Arc::new(OpenAIProvider::new_with_base_url_no_v1(api_key, base_url)), model))
            }
            other => {
                // Generic OpenAI-compatible provider (custom providers)
                match base_url {
                    Some(url) => Ok((Arc::new(OpenAIProvider::new_with_base_url(api_key, url)), model)),
                    None => {
                        let env_provider_id = other.to_uppercase().replace('-', "_");
                        Err(RCodeError::Config(format!(
                            "Custom provider '{}' requires a base_url. Set providers.{}.base_url in config or {}_BASE_URL environment variable.",
                            other, other, env_provider_id
                        )))
                    }
                }
            }
        }
    }

    /// Build an Anthropic-compatible provider.
    fn build_anthropic_compat(
        provider_id: &str,
        api_key: String,
        base_url: Option<String>,
        model: String,
    ) -> Result<(Arc<dyn LlmProvider>, String)> {
        // Anthropic requires a base_url
        let base_url = base_url.ok_or_else(|| {
            RCodeError::Config(format!(
                "No base_url found for {}. Set providers.{}.base_url in config or {}_BASE_URL environment variable.",
                provider_id, provider_id, provider_id.to_uppercase().replace('-', "_")
            ))
        })?;

        // Use new_with_base_url to override any env var
        Ok((Arc::new(AnthropicProvider::new_with_base_url(api_key, base_url)), model))
    }

    /// Build a Google provider, honouring an optional custom base URL.
    fn build_google(api_key: String, base_url: Option<String>, model: String) -> Result<(Arc<dyn LlmProvider>, String)> {
        let provider: Arc<dyn LlmProvider> = match base_url {
            Some(url) => Arc::new(GoogleProvider::new_with_base_url(api_key, url)),
            None => Arc::new(GoogleProvider::new(api_key)),
        };
        Ok((provider, model))
    }
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
#[allow(dead_code)]
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
    use rcode_core::ProviderProtocol;
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

    /// Helper to create a config with typed provider config (for protocol tests)
    fn create_config_with_typed_provider(
        provider_id: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        protocol: Option<ProviderProtocol>,
    ) -> RcodeConfig {
        use std::collections::HashMap;
        let mut providers = HashMap::new();
        
        if api_key.is_some() || base_url.is_some() || protocol.is_some() {
            providers.insert(
                provider_id.to_string(),
                rcode_core::ProviderConfig {
                    api_key: api_key.map(String::from),
                    base_url: base_url.map(String::from),
                    protocol,
                    enabled: true,
                    disabled: false,
                    display_name: None,
                    models: None,
                },
            );
        }
        
        RcodeConfig {
            providers,
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

    // ============ GitHub Copilot Provider Tests ============

    #[test]
    fn test_factory_github_copilot_provider_with_config() {
        let config = create_config_with_providers(
            json!({
                "github-copilot": {
                    "api_key": "gho_test_token"
                }
            }),
            None,
            None,
        );
        let result = ProviderFactory::build("github-copilot/gpt-4o", Some(&config));
        assert!(
            result.is_ok(),
            "github-copilot provider should build successfully: {:?}",
            result.err()
        );
        let (provider, model_name) = result.unwrap();
        // github-copilot uses OpenAI-compatible backend
        assert_eq!(provider.provider_id(), "openai");
        assert_eq!(model_name, "gpt-4o");
    }

    #[test]
    fn test_factory_github_copilot_disabled() {
        let config = RcodeConfig {
            disabled_providers: Some(vec!["github-copilot".to_string()]),
            ..Default::default()
        };
        let result = ProviderFactory::build("github-copilot/gpt-4o", Some(&config));
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("disabled"));
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

    // ============ Protocol Dispatch Tests (T-09) ============

    #[test]
    fn test_factory_protocol_dispatch_openai_compat() {
        // Test minimax (an OpenAI-compatible provider) using protocol dispatch
        let config = create_config_with_providers(
            json!({
                "minimax": {
                    "api_key": "test-key",
                    "base_url": "https://custom.minimax.io/v1"
                }
            }),
            None,
            None,
        );
        let result = ProviderFactory::build("minimax/MiniMax-Text-01", Some(&config));
        assert!(result.is_ok(), "minimax should dispatch via OpenAiCompat protocol: {:?}", result.err());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "minimax");
        assert_eq!(model_name, "MiniMax-Text-01");
    }

    #[test]
    fn test_factory_unknown_provider_with_protocol() {
        // Test custom provider with explicit protocol in typed config
        let config = create_config_with_typed_provider(
            "my-anthropic-proxy",
            Some("test-key"),
            Some("https://my-proxy.example.com/anthropic"),
            Some(ProviderProtocol::AnthropicCompat),
        );
        
        let result = ProviderFactory::build("my-anthropic-proxy/claude-3-5-sonnet", Some(&config));
        assert!(result.is_ok(), "Custom provider with explicit protocol should work: {:?}", result.err());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "anthropic");
        assert_eq!(model_name, "claude-3-5-sonnet");
    }

    #[test]
    fn test_factory_anthropic_compat_with_base_url() {
        // Test anthropic-compatible provider with custom base_url
        let config = create_config_with_typed_provider(
            "minimax-via-anthropic",
            Some("test-key"),
            Some("https://api.minimax.io/anthropic"),
            Some(ProviderProtocol::AnthropicCompat),
        );
        
        let result = ProviderFactory::build("minimax-via-anthropic/claude-3-5-sonnet", Some(&config));
        assert!(result.is_ok(), "Anthropic-compat with base_url should work: {:?}", result.err());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "anthropic");
        assert_eq!(model_name, "claude-3-5-sonnet");
    }

    #[test]
    fn test_factory_anthropic_compat_without_base_url_fails() {
        // Test anthropic-compatible provider WITHOUT base_url fails
        let config = create_config_with_typed_provider(
            "my-anthropic-proxy",
            Some("test-key"),
            None, // no base_url
            Some(ProviderProtocol::AnthropicCompat),
        );
        
        let result = ProviderFactory::build("my-anthropic-proxy/claude-3-5-sonnet", Some(&config));
        assert!(result.is_err(), "Anthropic-compat without base_url should fail");
        let err = result.err().unwrap();
        assert!(err.to_string().contains("base_url"), "Error should mention base_url");
    }

    #[test]
    fn test_factory_openai_compat_with_base_url() {
        // Test openai-compatible provider with custom base_url
        let config = create_config_with_typed_provider(
            "my-openai-proxy",
            Some("test-key"),
            Some("https://my-proxy.example.com/v1"),
            Some(ProviderProtocol::OpenAiCompat),
        );
        
        let result = ProviderFactory::build("my-openai-proxy/gpt-4o", Some(&config));
        assert!(result.is_ok(), "OpenAI-compat with base_url should work: {:?}", result.err());
        let (provider, model_name) = result.unwrap();
        assert_eq!(provider.provider_id(), "openai");
        assert_eq!(model_name, "gpt-4o");
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
