//! Unified provider resolution for api_key, base_url, and auth_token.
//!
//! This module provides a single source of truth for resolving provider
//! configuration, consumed by both discovery and runtime factory paths.

use rcode_core::RcodeConfig;

use super::registry::lookup;

/// Source of authentication credentials
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthSource {
    /// Credentials stored in auth.json (primary OpenCode credential store)
    AuthJson,
    /// Credentials from environment variables
    Env,
    /// Credentials from config file (deprecated)
    Config,
    /// No credentials configured
    None,
}

/// Kind of authentication credential
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    /// API key credential
    ApiKey,
    /// OAuth token credential
    OAuth,
    /// Environment variable (treated as api_key equivalent)
    Env,
    /// No credential kind
    None,
}

/// Unified authentication state for a provider.
///
/// This is the single source of truth for determining provider authentication status,
/// consumed by both API routes and UI displays.
#[derive(Debug, Clone)]
pub struct AuthState {
    /// Whether the provider has valid credentials configured
    pub connected: bool,
    /// Where the credentials come from
    pub source: AuthSource,
    /// What kind of credential it is
    pub kind: AuthKind,
    /// Human-readable label for the auth source
    pub label: &'static str,
    /// The environment variable name (only set when source == Env)
    pub env_key: Option<String>,
    /// Whether the user can disconnect/remove this credential via the app
    pub can_disconnect: bool,
    /// The resolved API key (if available) - NEVER exposed via API
    pub api_key: Option<String>,
}

/// Serializable DTO for AuthState - omits api_key for security.
///
/// Used in all API responses to prevent credential leakage.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthStateDto {
    pub connected: bool,
    pub source: AuthSource,
    pub kind: AuthKind,
    pub label: &'static str,
    pub env_key: Option<String>,
    pub can_disconnect: bool,
}

impl From<&AuthState> for AuthStateDto {
    fn from(state: &AuthState) -> Self {
        AuthStateDto {
            connected: state.connected,
            source: state.source,
            kind: state.kind,
            label: state.label,
            env_key: state.env_key.clone(),
            can_disconnect: state.can_disconnect,
        }
    }
}

/// Resolve the authentication state for a provider.
///
/// This is the SINGLE mechanism for determining provider authentication state.
/// All code paths (catalog, factory, routes) MUST use this function.
///
/// Precedence: auth.json (including aliases) → env vars → config.api_key → none
///
/// # Arguments
/// * `provider_id` - The provider identifier (e.g., "openai", "minimax")
/// * `registry_def` - Optional provider definition from the registry (for credential aliases)
/// * `config` - The RcodeConfig to check for config-based credentials
pub fn resolve_auth(
    provider_id: &str,
    registry_def: Option<&super::registry::ProviderDefinition>,
    config: &RcodeConfig,
) -> AuthState {
    let env_provider_id = provider_id.to_uppercase().replace('-', "_");

    // 1. auth.json (primary credential store) — including aliases
    if rcode_core::auth::has_credential(provider_id) {
        let kind = match rcode_core::auth::get_credential_type(provider_id) {
            Some("oauth") => AuthKind::OAuth,
            _ => AuthKind::ApiKey,
        };
        let api_key = rcode_core::auth::get_api_key(provider_id);
        return AuthState {
            connected: true,
            source: AuthSource::AuthJson,
            kind,
            label: "auth.json",
            env_key: None,
            can_disconnect: true,
            api_key,
        };
    }

    // 1b. Try registry credential aliases (e.g. "minimax-coding-plan" for "minimax")
    if let Some(def) = registry_def {
        for alias in def.credential_aliases {
            if rcode_core::auth::has_credential(alias) {
                let api_key = rcode_core::auth::get_api_key(alias);
                return AuthState {
                    connected: true,
                    source: AuthSource::AuthJson,
                    kind: AuthKind::ApiKey,
                    label: "auth.json",
                    env_key: None,
                    can_disconnect: true,
                    api_key,
                };
            }
        }
    }

    // 2. Environment variables
    let env_key = format!("{}_API_KEY", env_provider_id);
    if let Ok(api_key) = std::env::var(&env_key) {
        if !api_key.is_empty() {
            return AuthState {
                connected: true,
                source: AuthSource::Env,
                kind: AuthKind::Env,
                label: "Environment Variable",
                env_key: Some(env_key.clone()),
                can_disconnect: false,
                api_key: Some(api_key),
            };
        }
    }

    let auth_key = format!("{}_AUTH_TOKEN", env_provider_id);
    if let Ok(api_key) = std::env::var(&auth_key) {
        if !api_key.is_empty() {
            return AuthState {
                connected: true,
                source: AuthSource::Env,
                kind: AuthKind::Env,
                label: "Environment Variable",
                env_key: Some(auth_key.clone()),
                can_disconnect: false,
                api_key: Some(api_key),
            };
        }
    }

    // 3. Config file (deprecated — api_key should be in auth.json)
    // Try typed providers first, then fall back to extra JSON
    if let Some(provider_config) = config.providers.get(provider_id) {
        if let Some(ref api_key) = provider_config.api_key {
            if !api_key.is_empty() {
                return AuthState {
                    connected: true,
                    source: AuthSource::Config,
                    kind: AuthKind::ApiKey,
                    label: "Config file",
                    env_key: None,
                    can_disconnect: false,
                    api_key: Some(api_key.clone()),
                };
            }
        }
    }
    
    // Fallback to extra JSON (for legacy config format compatibility)
    if let Some(val) = config
        .extra
        .get("providers")
        .and_then(|p| p.get(provider_id))
        .and_then(|p| p.get("api_key"))
        .and_then(|v| v.as_str())
    {
        if !val.is_empty() {
            return AuthState {
                connected: true,
                source: AuthSource::Config,
                kind: AuthKind::ApiKey,
                label: "Config file",
                env_key: None,
                can_disconnect: false,
                api_key: Some(val.to_string()),
            };
        }
    }

    // 4. No credentials
    AuthState {
        connected: false,
        source: AuthSource::None,
        kind: AuthKind::None,
        label: "Not configured",
        env_key: None,
        can_disconnect: false,
        api_key: None,
    }
}

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

    // =============================================================================
    // resolve_auth tests
    // =============================================================================

    #[test]
    fn test_resolve_auth_precedence_env_wins_over_config() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            // Set env var - should win over config
            std::env::set_var("TESTPREENV_API_KEY", "env-key-from-test");
            std::env::remove_var("TESTPREENV_AUTH_TOKEN");

            let config = create_config_with_provider("testpreenv", Some("config-key"), None);
            let auth_state = resolve_auth("testpreenv", None, &config);

            assert!(auth_state.connected, "Should be connected via env");
            assert_eq!(auth_state.source, AuthSource::Env);
            assert_eq!(auth_state.api_key, Some("env-key-from-test".to_string()));

            std::env::remove_var("TESTPREENV_API_KEY");
        }
    }

    #[test]
    fn test_resolve_auth_precedence_config_when_no_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("TESTPRECFIG_API_KEY");
            std::env::remove_var("TESTPRECFIG_AUTH_TOKEN");

            // Use typed provider config (providers HashMap) so resolve_auth can find it
            let mut providers = std::collections::HashMap::new();
            providers.insert("testprecfig".to_string(), rcode_core::ProviderConfig {
                api_key: Some("config-key".to_string()),
                ..Default::default()
            });
            let config = rcode_core::RcodeConfig {
                providers,
                ..Default::default()
            };
            let auth_state = resolve_auth("testprecfig", None, &config);

            assert!(auth_state.connected, "Should be connected via config");
            assert_eq!(auth_state.source, AuthSource::Config);
            assert_eq!(auth_state.api_key, Some("config-key".to_string()));
        }
    }

    #[test]
    fn test_resolve_auth_no_credentials() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("TESTNOCRED_API_KEY");
            std::env::remove_var("TESTNOCRED_AUTH_TOKEN");

            let config = RcodeConfig::default();
            let auth_state = resolve_auth("testnocred", None, &config);

            assert!(!auth_state.connected, "Should not be connected");
            assert_eq!(auth_state.source, AuthSource::None);
            assert_eq!(auth_state.api_key, None);
        }
    }

    #[test]
    fn test_resolve_auth_label_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TESTLABELENV_API_KEY", "test-key");
            std::env::remove_var("TESTLABELENV_AUTH_TOKEN");

            let config = RcodeConfig::default();
            let auth_state = resolve_auth("testlabelenv", None, &config);

            assert_eq!(auth_state.label, "Environment Variable");
            assert!(auth_state.env_key.is_some());
            assert_eq!(auth_state.env_key.as_deref(), Some("TESTLABELENV_API_KEY"));

            std::env::remove_var("TESTLABELENV_API_KEY");
        }
    }

    #[test]
    fn test_resolve_auth_label_config() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("TESTLBL_CFG_API_KEY");
            std::env::remove_var("TESTLBL_CFG_AUTH_TOKEN");

            // Use typed provider config (providers HashMap) so resolve_auth can find it
            let mut providers = std::collections::HashMap::new();
            providers.insert("testlbl_cfg".to_string(), rcode_core::ProviderConfig {
                api_key: Some("config-key".to_string()),
                ..Default::default()
            });
            let config = rcode_core::RcodeConfig {
                providers,
                ..Default::default()
            };
            let auth_state = resolve_auth("testlbl_cfg", None, &config);

            assert_eq!(auth_state.label, "Config file");
        }
    }

    #[test]
    fn test_resolve_auth_minimax_alias_resolution() {
        // minimax has credential_aliases = ["minimax-coding-plan"]
        // When checking "minimax", it should also check "minimax-coding-plan" alias
        // Note: This test verifies the alias lookup logic works, but actual auth.json
        // resolution depends on system state. We test the registry lookup path.
        
        let minimax_def = crate::registry::lookup("minimax");
        assert!(minimax_def.is_some(), "minimax should be in registry");
        let def = minimax_def.unwrap();
        assert!(def.credential_aliases.contains(&"minimax-coding-plan"), 
            "minimax should have minimax-coding-plan alias");
        
        // The actual alias resolution is tested by checking that the registry
        // returns the correct definition with aliases
        assert_eq!(def.id, "minimax");
    }

    #[test]
    fn test_resolve_auth_can_disconnect_auth_json() {
        // can_disconnect should be true when source is AuthJson
        // Note: This test verifies the logic path, actual auth.json depends on system state
        let config = RcodeConfig::default();
        
        // For a provider with no credentials, can_disconnect should be false
        unsafe {
            std::env::remove_var("TESTNODISCONNECT_API_KEY");
            std::env::remove_var("TESTNODISCONNECT_AUTH_TOKEN");
            
            let auth_state = resolve_auth("testnodisconnect", None, &config);
            assert!(!auth_state.can_disconnect, "No creds = cannot disconnect");
        }
    }

    #[test]
    fn test_resolve_auth_cannot_disconnect_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TESTNODISCENV_API_KEY", "test-key");
            std::env::remove_var("TESTNODISCENV_AUTH_TOKEN");

            let config = RcodeConfig::default();
            let auth_state = resolve_auth("testnodiscenv", None, &config);

            assert!(!auth_state.can_disconnect, "Env var creds = cannot disconnect (not in auth.json)");

            std::env::remove_var("TESTNODISCENV_API_KEY");
        }
    }

    #[test]
    fn test_resolve_auth_api_key_none_when_not_connected() {
        // When not connected, api_key should be None even if some fields are populated
        unsafe {
            std::env::remove_var("TESTAPIKEYNONE_API_KEY");
            std::env::remove_var("TESTAPIKEYNONE_AUTH_TOKEN");

            let config = create_config_with_provider("testapikeynone", None, None);
            let auth_state = resolve_auth("testapikeynone", None, &config);

            assert!(!auth_state.connected);
            assert_eq!(auth_state.api_key, None);

            std::env::remove_var("TESTAPIKEYNONE_API_KEY");
        }
    }

    #[test]
    fn test_resolve_auth_api_key_present_when_connected() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TESTAPIKEYPRES_API_KEY", "test-key-value");
            std::env::remove_var("TESTAPIKEYPRES_AUTH_TOKEN");

            let config = RcodeConfig::default();
            let auth_state = resolve_auth("testapikeypres", None, &config);

            assert!(auth_state.connected);
            assert_eq!(auth_state.api_key, Some("test-key-value".to_string()));

            std::env::remove_var("TESTAPIKEYPRES_API_KEY");
        }
    }
}
