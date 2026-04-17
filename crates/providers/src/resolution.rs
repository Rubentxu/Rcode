//! Unified provider resolution for api_key, base_url, and auth_token.
//!
//! This module provides a single source of truth for resolving provider
//! configuration, consumed by both discovery and runtime factory paths.

use rcode_core::RcodeConfig;

use super::registry::lookup;

/// Known provider default base URLs
const KNOWN_DEFAULT_BASE_URLS: &[(&str, &str)] = &[
    ("openai", "https://api.openai.com/v1"),
    ("anthropic", "https://api.anthropic.com"),
    ("google", "https://generativelanguage.googleapis.com"),
    ("openrouter", "https://openrouter.ai/api/v1"),
    ("minimax", "https://api.minimax.chat/v1"),
    ("zai", "https://api.zai.chat/v1"),
    ("github-copilot", "https://api.githubcopilot.com"),
];

/// Unified resolution result for a provider's credentials and endpoint.
#[derive(Debug, Clone)]
pub struct ProviderResolution {
    /// Resolved API key (if any)
    pub api_key: Option<String>,
    /// Resolved base URL (if any)
    pub base_url: Option<String>,
}

impl ProviderResolution {
    /// Resolve api_key, base_url, and auth_token for a provider.
    ///
    /// Resolution order:
    /// - api_key: auth.json → registry credential_aliases → env var ({PROVIDER}_API_KEY, {PROVIDER}_AUTH_TOKEN) → config
    /// - base_url: env var ({PROVIDER}_BASE_URL) → config → provider-known default
    pub fn for_provider(provider_id: &str, config: Option<&RcodeConfig>) -> Self {
        let env_provider_id = provider_id.to_uppercase().replace('-', "_");

        // Resolve api_key: auth.json → registry aliases → env → config
        let api_key = resolve_api_key_internal(provider_id, config);

        // Resolve base_url: env → config → known default
        let base_url = resolve_base_url_internal(provider_id, &env_provider_id, config);

        Self { api_key, base_url }
    }

    /// Returns the known default base URL for a provider, if any.
    pub fn known_default_base_url(provider_id: &str) -> Option<&'static str> {
        KNOWN_DEFAULT_BASE_URLS
            .iter()
            .find(|(id, _)| *id == provider_id)
            .map(|(_, url)| *url)
    }
}

/// Known alternate credential key names for providers whose auth.json key
/// differs from the canonical provider_id (e.g. "minimax-coding-plan" for "minimax").
/// Kept for backward compatibility when registry lookup fails.
pub const ALTERNATE_CREDENTIAL_KEYS: &[(&str, &[&str])] = &[("minimax", &["minimax-coding-plan"])];

fn resolve_api_key_internal(provider_id: &str, config: Option<&RcodeConfig>) -> Option<String> {
    // 1. auth.json (primary credential store)
    if let Some(key) = rcode_core::auth::get_api_key(provider_id) {
        return Some(key);
    }

    // 1b. Try credential aliases from registry (if provider is registered)
    // e.g. "minimax-coding-plan" for "minimax"
    if let Some(def) = lookup(provider_id) {
        for alt_key in def.credential_aliases {
            if let Some(key) = rcode_core::auth::get_api_key(alt_key) {
                return Some(key);
            }
        }
    } else {
        // Fallback to hardcoded ALTERNATE_CREDENTIAL_KEYS for backward compat
        // when provider is not in registry
        if let Some((_, alt_keys)) = ALTERNATE_CREDENTIAL_KEYS
            .iter()
            .find(|(id, _)| *id == provider_id)
        {
            for alt_key in *alt_keys {
                if let Some(key) = rcode_core::auth::get_api_key(alt_key) {
                    return Some(key);
                }
            }
        }
    }

    // 2. Environment variables
    let env_provider_id = provider_id.to_uppercase().replace('-', "_");

    let env_key = format!("{}_API_KEY", env_provider_id);
    if let Ok(key) = std::env::var(&env_key) {
        if !key.is_empty() {
            return Some(key);
        }
    }

    let auth_key = format!("{}_AUTH_TOKEN", env_provider_id);
    if let Ok(key) = std::env::var(&auth_key) {
        if !key.is_empty() {
            return Some(key);
        }
    }

    // 3. Config file (typed + extra JSON fallback)
    if let Some(cfg) = config {
        // Try typed field first
        if let Some(provider) = cfg.providers.get(provider_id) {
            if let Some(ref key) = provider.api_key {
                if !key.is_empty() {
                    return Some(key.clone());
                }
            }
        }
        // Fallback to extra JSON
        if let Some(val) = cfg
            .extra
            .get("providers")
            .and_then(|p| p.get(provider_id))
            .and_then(|p| p.get("api_key"))
            .and_then(|v| v.as_str())
        {
            return Some(val.to_string());
        }
    }

    None
}

fn resolve_base_url_internal(
    provider_id: &str,
    env_provider_id: &str,
    config: Option<&RcodeConfig>,
) -> Option<String> {
    // 1. Environment variable
    let env_key = format!("{}_BASE_URL", env_provider_id);
    if let Ok(url) = std::env::var(&env_key) {
        if !url.is_empty() {
            return Some(url);
        }
    }

    // 2. Config file (typed + extra JSON fallback)
    if let Some(cfg) = config {
        // Try typed field first
        if let Some(provider) = cfg.providers.get(provider_id) {
            if let Some(ref url) = provider.base_url {
                if !url.is_empty() {
                    return Some(url.clone());
                }
            }
        }
        // Fallback to extra JSON
        if let Some(val) = cfg
            .extra
            .get("providers")
            .and_then(|p| p.get(provider_id))
            .and_then(|p| p.get("base_url"))
            .and_then(|v| v.as_str())
        {
            return Some(val.to_string());
        }
    }

    // 3. Known default
    ProviderResolution::known_default_base_url(provider_id).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_config_with_provider(
        provider_id: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
    ) -> RcodeConfig {
        use serde_json::json;
        let mut providers = serde_json::Map::new();
        if api_key.is_some() || base_url.is_some() {
            let mut provider_map = serde_json::Map::new();
            if let Some(key) = api_key {
                provider_map.insert("api_key".to_string(), serde_json::json!(key));
            }
            if let Some(url) = base_url {
                provider_map.insert("base_url".to_string(), serde_json::json!(url));
            }
            providers.insert(provider_id.to_string(), serde_json::json!(provider_map));
        }
        RcodeConfig {
            extra: json!({ "providers": providers }),
            ..Default::default()
        }
    }

    // =============================================================================
    // Known-default base URL tests
    // =============================================================================

    #[test]
    fn test_known_default_openai() {
        assert_eq!(
            ProviderResolution::known_default_base_url("openai"),
            Some("https://api.openai.com/v1")
        );
    }

    #[test]
    fn test_known_default_anthropic() {
        assert_eq!(
            ProviderResolution::known_default_base_url("anthropic"),
            Some("https://api.anthropic.com")
        );
    }

    #[test]
    fn test_known_default_google() {
        assert_eq!(
            ProviderResolution::known_default_base_url("google"),
            Some("https://generativelanguage.googleapis.com")
        );
    }

    #[test]
    fn test_known_default_openrouter() {
        assert_eq!(
            ProviderResolution::known_default_base_url("openrouter"),
            Some("https://openrouter.ai/api/v1")
        );
    }

    #[test]
    fn test_known_default_minimax() {
        assert_eq!(
            ProviderResolution::known_default_base_url("minimax"),
            Some("https://api.minimax.chat/v1")
        );
    }

    #[test]
    fn test_known_default_zai() {
        assert_eq!(
            ProviderResolution::known_default_base_url("zai"),
            Some("https://api.zai.chat/v1")
        );
    }

    #[test]
    fn test_known_default_github_copilot() {
        assert_eq!(
            ProviderResolution::known_default_base_url("github-copilot"),
            Some("https://api.githubcopilot.com")
        );
    }

    #[test]
    fn test_known_default_unknown_provider() {
        assert_eq!(ProviderResolution::known_default_base_url("unknown"), None);
    }

    // =============================================================================
    // for_provider: known providers with no config/env
    // =============================================================================

    #[test]
    fn test_for_provider_uses_known_default_base_url_for_openai() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("OPENAI_AUTH_TOKEN");
            std::env::remove_var("OPENAI_BASE_URL");

            let res = ProviderResolution::for_provider("openai", None);
            assert!(
                res.api_key.is_none(),
                "No API key should be resolved without env/config"
            );
            assert_eq!(
                res.base_url,
                Some("https://api.openai.com/v1".to_string()),
                "Should use known default base URL"
            );
        }
    }

    #[test]
    fn test_for_provider_uses_known_default_base_url_for_minimax() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("MINIMAX_API_KEY");
            std::env::remove_var("MINIMAX_AUTH_TOKEN");
            std::env::remove_var("MINIMAX_BASE_URL");

            let res = ProviderResolution::for_provider("minimax", None);
            // Note: api_key may still be Some if auth.json has minimax creds (system-dependent)
            assert_eq!(
                res.base_url,
                Some("https://api.minimax.chat/v1".to_string()),
                "Should use known default base URL for minimax"
            );
        }
    }

    #[test]
    fn test_for_provider_uses_known_default_base_url_for_zai() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("ZAI_API_KEY");
            std::env::remove_var("ZAI_BASE_URL");

            let res = ProviderResolution::for_provider("zai", None);
            assert!(res.api_key.is_none());
            assert_eq!(
                res.base_url,
                Some("https://api.zai.chat/v1".to_string()),
                "Should use known default base URL for zai"
            );
        }
    }

    #[test]
    fn test_for_provider_uses_known_default_base_url_for_openrouter() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("OPENROUTER_API_KEY");
            std::env::remove_var("OPENROUTER_BASE_URL");

            let res = ProviderResolution::for_provider("openrouter", None);
            assert!(res.api_key.is_none());
            assert_eq!(
                res.base_url,
                Some("https://openrouter.ai/api/v1".to_string()),
                "Should use known default base URL for openrouter"
            );
        }
    }

    #[test]
    fn test_for_provider_uses_known_default_base_url_for_anthropic() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
            std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
            std::env::remove_var("ANTHROPIC_BASE_URL");

            let res = ProviderResolution::for_provider("anthropic", None);
            assert!(res.api_key.is_none());
            assert_eq!(
                res.base_url,
                Some("https://api.anthropic.com".to_string()),
                "Should use known default base URL for anthropic"
            );
        }
    }

    #[test]
    fn test_for_provider_uses_known_default_base_url_for_google() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("GOOGLE_API_KEY");
            std::env::remove_var("GOOGLE_BASE_URL");

            let res = ProviderResolution::for_provider("google", None);
            assert!(res.api_key.is_none());
            assert_eq!(
                res.base_url,
                Some("https://generativelanguage.googleapis.com".to_string()),
                "Should use known default base URL for google"
            );
        }
    }

    // =============================================================================
    // for_provider: unknown provider returns None/None
    // =============================================================================

    #[test]
    fn test_for_provider_unknown_provider_returns_none() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("MYCUSTOM_API_KEY");
            std::env::remove_var("MYCUSTOM_BASE_URL");

            let res = ProviderResolution::for_provider("mycustom", None);
            assert!(
                res.api_key.is_none(),
                "Unknown provider should have no API key"
            );
            assert!(
                res.base_url.is_none(),
                "Unknown provider should have no default base URL"
            );
        }
    }

    // =============================================================================
    // for_provider: env var resolution
    // =============================================================================

    #[test]
    fn test_for_provider_resolves_api_key_from_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TESTRES_PROVIDER_API_KEY", "env-api-key");
            std::env::remove_var("TESTRES_PROVIDER_BASE_URL");

            let res = ProviderResolution::for_provider("testres_provider", None);
            assert_eq!(res.api_key, Some("env-api-key".to_string()));
            assert!(res.base_url.is_none());

            std::env::remove_var("TESTRES_PROVIDER_API_KEY");
        }
    }

    #[test]
    fn test_for_provider_resolves_base_url_from_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("TESTURL_PROVIDER_API_KEY");
            std::env::set_var("TESTURL_PROVIDER_BASE_URL", "https://custom.example.com/v1");

            let res = ProviderResolution::for_provider("testurl_provider", None);
            assert!(res.api_key.is_none());
            assert_eq!(
                res.base_url,
                Some("https://custom.example.com/v1".to_string())
            );

            std::env::remove_var("TESTURL_PROVIDER_BASE_URL");
        }
    }

    // =============================================================================
    // for_provider: config resolution
    // =============================================================================

    #[test]
    fn test_for_provider_resolves_api_key_from_config() {
        let config = create_config_with_provider("testcfg", Some("config-api-key"), None);
        let res = ProviderResolution::for_provider("testcfg", Some(&config));
        assert_eq!(res.api_key, Some("config-api-key".to_string()));
        assert!(res.base_url.is_none());
    }

    #[test]
    fn test_for_provider_resolves_base_url_from_config() {
        let config =
            create_config_with_provider("testcfg", None, Some("https://config.example.com/v1"));
        let res = ProviderResolution::for_provider("testcfg", Some(&config));
        assert!(res.api_key.is_none());
        assert_eq!(
            res.base_url,
            Some("https://config.example.com/v1".to_string())
        );
    }

    #[test]
    fn test_for_provider_config_overrides_known_default() {
        let config =
            create_config_with_provider("openai", None, Some("https://custom.openai.dev/v1"));
        let res = ProviderResolution::for_provider("openai", Some(&config));
        // Config base_url should win over known default
        assert_eq!(
            res.base_url,
            Some("https://custom.openai.dev/v1".to_string()),
            "Config base_url should override known default"
        );
    }

    // =============================================================================
    // for_provider: env overrides config for base_url
    // =============================================================================

    #[test]
    fn test_for_provider_env_overrides_config_base_url() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TESTENVCFG_BASE_URL", "https://env.example.com/v1");

            let config = create_config_with_provider(
                "testenvcfg",
                None,
                Some("https://config.example.com/v1"),
            );
            let res = ProviderResolution::for_provider("testenvcfg", Some(&config));

            // Env should win over config
            assert_eq!(res.base_url, Some("https://env.example.com/v1".to_string()));

            std::env::remove_var("TESTENVCFG_BASE_URL");
        }
    }
}
