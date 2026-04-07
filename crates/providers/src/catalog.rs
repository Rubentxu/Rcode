//! Model catalog service — discovers available models from provider APIs
//! and falls back to a curated static manifest.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A model entry in the catalog.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CatalogModel {
    /// Full model ID, e.g. "anthropic/claude-sonnet-4-5"
    pub id: String,
    /// Provider identifier, e.g. "anthropic"
    pub provider: String,
    /// Human-readable display name
    pub display_name: String,
    /// Whether this provider has credentials configured
    pub has_credentials: bool,
    /// Where this model entry came from
    pub source: ModelSource,
    /// Whether the model is usable (has creds + not disabled)
    pub enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    /// Discovered via provider API
    Api,
    /// From curated fallback list
    Fallback,
    /// Explicitly configured by user in config
    Configured,
}

/// Response for GET /models
#[derive(Debug, serde::Serialize)]
pub struct ListModelsResponse {
    pub models: Vec<CatalogModel>,
}

/// Trait for provider-specific model discovery.
#[async_trait::async_trait]
pub trait ModelDiscovery: Send + Sync {
    fn provider_id(&self) -> &str;
    fn provider_name(&self) -> &str;
    /// Discover models from the provider API. Returns empty vec on failure (never errors).
    async fn discover(&self, api_key: Option<&str>, base_url: Option<&str>) -> Vec<String>;
}

/// Curated fallback models per provider. Updated regularly.
/// Used when API discovery is unavailable (no key, API failure, offline).
pub const FALLBACK_MODELS: &[(&str, &str, &[&str])] = &[
    ("anthropic", "Anthropic", &[
        "claude-sonnet-4-5-20250514",
        "claude-opus-4-5-20250514",
        "claude-haiku-3-5-20241022",
    ]),
    ("openai", "OpenAI", &[
        "gpt-4o-2024-11-20",
        "gpt-4o-mini-2024-07-18",
        "o3-mini-2025-01-31",
        "o4-mini-2025-04-16",
    ]),
    ("google", "Google", &[
        "gemini-2.5-pro-preview-05-06",
        "gemini-2.5-flash-preview-05-20",
        "gemini-2.0-flash",
    ]),
    ("openrouter", "OpenRouter", &[
        "anthropic/claude-sonnet-4",
        "openai/gpt-4o",
        "google/gemini-2.5-pro",
    ]),
    ("minimax", "MiniMax", &[
        "MiniMax-M2.7",
        "MiniMax-M2.5",
        "MiniMax-M2.1",
    ]),
    ("zai", "ZAI", &[
        "zai-coding-plan",
        "zai-coding-standard",
        "zai-coding-premium",
    ]),
];

// ============================================================================
// Provider Discovery Adapters
// ============================================================================

/// OpenAI-compatible discovery adapter
struct OpenAiDiscovery;

#[async_trait::async_trait]
impl ModelDiscovery for OpenAiDiscovery {
    fn provider_id(&self) -> &str { "openai" }
    fn provider_name(&self) -> &str { "OpenAI" }

    async fn discover(&self, api_key: Option<&str>, base_url: Option<&str>) -> Vec<String> {
        let key = match api_key {
            Some(k) => k,
            None => return Vec::new(),
        };
        let base = base_url.unwrap_or("https://api.openai.com/v1");
        let url = format!("{}/models", base.trim_end_matches('/'));

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build() 
        {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let resp = match client
            .get(&url)
            .header("Authorization", format!("Bearer {}", key))
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        if !resp.status().is_success() {
            return Vec::new();
        }

        #[derive(serde::Deserialize)]
        struct OpenAiModelsResponse {
            data: Vec<OpenAiModel>,
        }
        #[derive(serde::Deserialize)]
        struct OpenAiModel {
            id: String,
        }

        let models: OpenAiModelsResponse = match resp.json().await {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        models.data.into_iter().map(|m| m.id).collect()
    }
}

/// Anthropic discovery adapter
struct AnthropicDiscovery;

#[async_trait::async_trait]
impl ModelDiscovery for AnthropicDiscovery {
    fn provider_id(&self) -> &str { "anthropic" }
    fn provider_name(&self) -> &str { "Anthropic" }

    async fn discover(&self, api_key: Option<&str>, _base_url: Option<&str>) -> Vec<String> {
        let key = match api_key {
            Some(k) => k,
            None => return Vec::new(),
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build() 
        {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let resp = match client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        if !resp.status().is_success() {
            return Vec::new();
        }

        #[derive(serde::Deserialize)]
        struct AnthropicModelsResponse {
            data: Vec<AnthropicModel>,
        }
        #[derive(serde::Deserialize)]
        struct AnthropicModel {
            id: String,
        }

        let models: AnthropicModelsResponse = match resp.json().await {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        models.data.into_iter().map(|m| m.id).collect()
    }
}

/// Google discovery adapter
struct GoogleDiscovery;

#[async_trait::async_trait]
impl ModelDiscovery for GoogleDiscovery {
    fn provider_id(&self) -> &str { "google" }
    fn provider_name(&self) -> &str { "Google" }

    async fn discover(&self, api_key: Option<&str>, _base_url: Option<&str>) -> Vec<String> {
        let key = match api_key {
            Some(k) => k,
            None => return Vec::new(),
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build() 
        {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let url = format!("https://generativelanguage.googleapis.com/v1beta/models?key={}", key);

        let resp = match client
            .get(&url)
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        if !resp.status().is_success() {
            return Vec::new();
        }

        #[derive(serde::Deserialize)]
        struct GoogleModelsResponse {
            models: Vec<GoogleModel>,
        }
        #[derive(serde::Deserialize)]
        struct GoogleModel {
            name: String,
        }

        let models: GoogleModelsResponse = match resp.json().await {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        models.models
            .into_iter()
            .map(|m| m.name.strip_prefix("models/").unwrap_or(&m.name).to_string())
            .collect()
    }
}

/// OpenAI-compatible discovery for third-party providers (MiniMax, ZAI, OpenRouter, etc.)
struct OpenAiCompatibleDiscovery {
    provider_id: String,
}

impl OpenAiCompatibleDiscovery {
    fn new(provider_id: &str) -> Self {
        Self { provider_id: provider_id.to_string() }
    }
}

#[async_trait::async_trait]
impl ModelDiscovery for OpenAiCompatibleDiscovery {
    fn provider_id(&self) -> &str { &self.provider_id }
    fn provider_name(&self) -> &str { &self.provider_id }

    async fn discover(&self, api_key: Option<&str>, base_url: Option<&str>) -> Vec<String> {
        let key = match api_key {
            Some(k) => k,
            None => return Vec::new(),
        };
        let base = match base_url {
            Some(url) => url,
            None => return Vec::new(),
        };
        let url = format!("{}/models", base.trim_end_matches('/'));

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build() 
        {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let resp = match client
            .get(&url)
            .header("Authorization", format!("Bearer {}", key))
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        if !resp.status().is_success() {
            return Vec::new();
        }

        #[derive(serde::Deserialize)]
        struct ModelsResponse {
            data: Vec<Model>,
        }
        #[derive(serde::Deserialize)]
        struct Model {
            id: String,
        }

        let models: ModelsResponse = match resp.json().await {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        models.data.into_iter().map(|m| m.id).collect()
    }
}

// ============================================================================
// Model Catalog Service
// ============================================================================

pub struct ModelCatalogService {
    /// Per-provider discovery adapters
    adapters: HashMap<String, Box<dyn ModelDiscovery>>,
    /// Cache: provider_id -> (models, timestamp)
    cache: Arc<Mutex<HashMap<String, (Vec<CatalogModel>, std::time::Instant)>>>,
    /// Cache TTL
    ttl: std::time::Duration,
}

impl ModelCatalogService {
    pub fn new() -> Self {
        let mut adapters: HashMap<String, Box<dyn ModelDiscovery>> = HashMap::new();
        // Register adapters for each known provider
        adapters.insert("anthropic".into(), Box::new(AnthropicDiscovery));
        adapters.insert("openai".into(), Box::new(OpenAiDiscovery));
        adapters.insert("google".into(), Box::new(GoogleDiscovery));
        // OpenAI-compatible providers reuse the OpenAI adapter
        for id in ["minimax", "zai", "openrouter"] {
            adapters.insert(id.into(), Box::new(OpenAiCompatibleDiscovery::new(id)));
        }
        Self {
            adapters,
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl: std::time::Duration::from_secs(300), // 5 min
        }
    }

    /// List all models. Returns immediately with cached/fallback data.
    /// Spawns background discovery for providers with credentials.
    pub async fn list_models(
        &self,
        config: &rcode_core::RcodeConfig,
    ) -> Vec<CatalogModel> {
        let mut all = Vec::new();
        let disabled_models = config
            .extra
            .get("disabled_models")
            .and_then(|value| value.as_array())
            .map(|items| {
                items.iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();

        if let Ok(model_id) = std::env::var("ANTHROPIC_MODEL") {
            let configured_id = format!("anthropic/{model_id}");
            all.push(CatalogModel {
                id: configured_id.clone(),
                provider: "anthropic".to_string(),
                display_name: model_id,
                has_credentials: std::env::var("ANTHROPIC_API_KEY").is_ok() || std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok(),
                source: ModelSource::Configured,
                enabled: (std::env::var("ANTHROPIC_API_KEY").is_ok() || std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok())
                    && !disabled_models.contains(&configured_id),
            });
        }

        // 1. Check if provider has credentials (auth.json → env var → config)
        // Mirrors OpenCode's credential resolution order.
        // Note: in OpenCode, the user picks a provider ID via /connect which may
        // differ from the canonical provider name (e.g. "zai-coding-plan" vs "zai").
        // We check the provider_id first, then each model_id as a fallback.
        let has_creds = |provider_id: &str, model_ids: &[&str]| -> bool {
            // Primary: auth.json (OpenCode's canonical credential store)
            if rcode_core::auth::has_credential(provider_id) { return true; }
            // Also try each model name as a credential key (e.g. "zai-coding-plan")
            for model_id in model_ids {
                if rcode_core::auth::has_credential(model_id) { return true; }
            }
            // Fallback 1: environment variables
            let env_key = format!("{}_API_KEY", provider_id.to_uppercase().replace('-', "_"));
            if std::env::var(&env_key).is_ok() { return true; }
            let auth_key = format!("{}_AUTH_TOKEN", provider_id.to_uppercase().replace('-', "_"));
            if std::env::var(&auth_key).is_ok() { return true; }
            // Fallback 2: config file (deprecated for secrets, but still supported)
            config.providers.get(provider_id)
                .and_then(|p| p.api_key.as_deref())
                .map(|k| !k.is_empty())
                .unwrap_or(false)
        };

        // 2. Check disabled/enabled providers
        let is_enabled = |provider_id: &str| -> bool {
            if let Some(ref disabled) = config.disabled_providers {
                if disabled.contains(&provider_id.to_string()) { return false; }
            }
            if let Some(ref enabled) = config.enabled_providers {
                if !enabled.is_empty() && !enabled.contains(&provider_id.to_string()) { return false; }
            }
            true
        };

        // 3. For each provider, check cache → fallback
        let cache = self.cache.lock().await;
        for (provider_id, _provider_name, model_ids) in FALLBACK_MODELS {
            if !is_enabled(provider_id) { continue; }
            let creds = has_creds(provider_id, model_ids);

            if let Some((cached_models, ts)) = cache.get(*provider_id) {
                if ts.elapsed() < self.ttl {
                    all.extend(cached_models.clone());
                    continue;
                }
            }

            // Return fallback models immediately
            for model_id in *model_ids {
                let full_id = format!("{}/{}", provider_id, model_id);
                if all.iter().any(|existing| existing.id == full_id) {
                    continue;
                }
                all.push(CatalogModel {
                    id: full_id.clone(),
                    provider: provider_id.to_string(),
                    display_name: model_id.to_string(),
                    has_credentials: creds,
                    source: ModelSource::Fallback,
                    enabled: creds && !disabled_models.contains(&full_id),
                });
            }
        }
        drop(cache);

        all
    }

    /// Refresh models for a specific provider (call API).
    /// Call this in background after initial render.
    pub async fn refresh_provider(
        &self,
        provider_id: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
    ) {
        if let Some(adapter) = self.adapters.get(provider_id) {
            let models = adapter.discover(api_key, base_url).await;
            if !models.is_empty() {
                let catalog_models: Vec<CatalogModel> = models.iter().map(|m| CatalogModel {
                    id: format!("{}/{}", provider_id, m),
                    provider: provider_id.to_string(),
                    display_name: m.clone(),
                    has_credentials: api_key.is_some(),
                    source: ModelSource::Api,
                    enabled: true,
                }).collect();
                let mut cache = self.cache.lock().await;
                cache.insert(provider_id.to_string(), (catalog_models, std::time::Instant::now()));
            }
        }
    }
}

impl Default for ModelCatalogService {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_models_returns_all_providers() {
        let providers: Vec<&str> = FALLBACK_MODELS.iter().map(|(id, _, _)| *id).collect();
        assert!(providers.contains(&"anthropic"), "Should have anthropic");
        assert!(providers.contains(&"openai"), "Should have openai");
        assert!(providers.contains(&"google"), "Should have google");
        assert!(providers.contains(&"openrouter"), "Should have openrouter");
        assert!(providers.contains(&"minimax"), "Should have minimax");
        assert!(providers.contains(&"zai"), "Should have zai");
    }

    #[tokio::test]
    async fn test_catalog_service_list_models_returns_fallback() {
        let service = ModelCatalogService::new();
        let config = rcode_core::RcodeConfig::default();
        let models = service.list_models(&config).await;
        
        // Should have fallback models for all enabled providers
        assert!(!models.is_empty(), "Should return fallback models");
        
        // All models should be from fallback source
        for model in &models {
            assert_eq!(model.source, ModelSource::Fallback, "All should be fallback source");
        }
    }

    #[test]
    fn test_has_credentials_from_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }
        
        let config = rcode_core::RcodeConfig::default();
        let has_creds = |provider_id: &str| -> bool {
            // auth.json first (won't have test creds, so falls through)
            if rcode_core::auth::has_credential(provider_id) { return true; }
            // env vars second
            let env_key = format!("{}_API_KEY", provider_id.to_uppercase().replace('-', "_"));
            if std::env::var(&env_key).is_ok() { return true; }
            let auth_key = format!("{}_AUTH_TOKEN", provider_id.to_uppercase().replace('-', "_"));
            if std::env::var(&auth_key).is_ok() { return true; }
            // config fallback
            config.providers.get(provider_id)
                .and_then(|p| p.api_key.as_deref())
                .map(|k| !k.is_empty())
                .unwrap_or(false)
        };
        
        assert!(has_creds("anthropic"));
        assert!(!has_creds("openai")); // Not set
        
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }
    }
}
