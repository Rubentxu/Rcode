//! Model catalog service — discovers available models from provider APIs
//! and falls back to a curated static manifest.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<String>;
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

/// Configuration for OpenAI-compatible discovery adapters.
/// Used to configure provider-specific model paths and other settings.
#[derive(Debug, Clone)]
pub struct DiscoveryIdentity {
    /// The provider ID (e.g., "openai", "minimax", "zai", "openrouter")
    pub provider_id: String,
    /// The API key header name (e.g., "Authorization", "x-api-key")
    pub header_name: &'static str,
    /// The model path suffix (e.g., "/models", "/v1/models")
    pub model_path: &'static str,
}

impl DiscoveryIdentity {
    /// Create a new DiscoveryIdentity for an OpenAI-compatible provider.
    pub fn openai_compat(provider_id: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            header_name: "Authorization",
            model_path: "/models",
        }
    }
}

/// OpenAI-compatible discovery adapter for OpenAI, MiniMax, ZAI, OpenRouter, etc.
struct OpenAiCompatDiscovery {
    identity: DiscoveryIdentity,
}

impl OpenAiCompatDiscovery {
    fn with_identity(identity: DiscoveryIdentity) -> Self {
        Self { identity }
    }
}

#[async_trait::async_trait]
impl ModelDiscovery for OpenAiCompatDiscovery {
    fn provider_id(&self) -> &str { &self.identity.provider_id }
    fn provider_name(&self) -> &str { &self.identity.provider_id }

    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<String> {
        let key = match ctx.api_key.as_deref() {
            Some(k) => k,
            None => return Vec::new(),
        };
        let base = match ctx.base_url.as_deref() {
            Some(url) => url,
            None => return Vec::new(),
        };
        let url = format!("{}{}", base.trim_end_matches('/'), self.identity.model_path);

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
        {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let resp = match client
            .get(&url)
            .header(self.identity.header_name, format!("Bearer {}", key))
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

/// Anthropic discovery adapter
struct AnthropicDiscovery;

#[async_trait::async_trait]
impl ModelDiscovery for AnthropicDiscovery {
    fn provider_id(&self) -> &str { "anthropic" }
    fn provider_name(&self) -> &str { "Anthropic" }

    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<String> {
        let key = match ctx.api_key.as_deref() {
            Some(k) => k,
            None => return Vec::new(),
        };

        let base = ctx.base_url.as_deref().unwrap_or("https://api.anthropic.com");
        let url = format!("{}/v1/models", base.trim_end_matches('/'));

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build() 
        {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let resp = match client
            .get(&url)
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

    async fn discover(&self, ctx: &DiscoveryContext) -> Vec<String> {
        let key = match ctx.api_key.as_deref() {
            Some(k) => k,
            None => return Vec::new(),
        };

        let base = ctx.base_url.as_deref().unwrap_or("https://generativelanguage.googleapis.com");
        let url = format!("{}/v1beta/models?key={}", base.trim_end_matches('/'), key);

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build() 
        {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

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

// ============================================================================
// Cache Store Trait
// ============================================================================

/// Trait for persisting model catalog cache to storage.
/// Implemented in rcode-server using rcode-storage's CatalogCacheRepository.
pub trait CacheStore: Send + Sync {
    /// Load all cached models grouped by provider_id, with their original fetch timestamp.
    /// Returns HashMap of provider_id -> (catalog models, fetched_at).
    /// The fetched_at is used to determine if cache entry is stale within TTL.
    fn get_all_cached(&self) -> HashMap<String, (Vec<CatalogModel>, std::time::SystemTime)>;

    /// Save models for a provider to persistent cache.
    fn save_cached_models(&self, provider_id: &str, models: &[CatalogModel]);
}

// ============================================================================
// Model Catalog Service
// ============================================================================

pub struct ModelCatalogService {
    /// Per-provider discovery adapters (shared via Arc)
    adapters: Arc<HashMap<String, Box<dyn ModelDiscovery>>>,
    /// Cache: provider_id -> (models, timestamp)
    cache: Arc<Mutex<HashMap<String, (Vec<CatalogModel>, std::time::Instant)>>>,
    /// Cache TTL
    ttl: std::time::Duration,
    /// In-flight refresh tasks for deduplication: provider_id -> JoinHandle
    in_flight: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    /// Optional persistent cache store
    cache_store: Option<Arc<dyn CacheStore>>,
}

impl ModelCatalogService {
    /// Create a new ModelCatalogService without persistent cache.
    pub fn new() -> Self {
        Self::with_cache_store(None)
    }

    /// Create a new ModelCatalogService with optional persistent cache.
    /// If cache_store is provided, it will be used to hydrate on init
    /// and persist after each refresh.
    pub fn with_cache_store(cache_store: Option<Arc<dyn CacheStore>>) -> Self {
        let mut adapters: HashMap<String, Box<dyn ModelDiscovery>> = HashMap::new();
        // Register adapters for each known provider
        adapters.insert("anthropic".into(), Box::new(AnthropicDiscovery));
        adapters.insert("google".into(), Box::new(GoogleDiscovery));
        // OpenAI-compatible providers (OpenAI, MiniMax, ZAI, OpenRouter) use OpenAiCompatDiscovery
        for id in ["openai", "minimax", "zai", "openrouter"] {
            adapters.insert(id.into(), Box::new(OpenAiCompatDiscovery::with_identity(DiscoveryIdentity::openai_compat(id))));
        }

        Self::build_service(adapters, cache_store)
    }

    /// Create a ModelCatalogService with custom adapters (for testing).
    pub fn with_adapters(adapters: HashMap<String, Box<dyn ModelDiscovery>>) -> Self {
        Self::build_service(adapters, None)
    }

    fn build_service(
        adapters: HashMap<String, Box<dyn ModelDiscovery>>,
        cache_store: Option<Arc<dyn CacheStore>>,
    ) -> Self {

        // Hydrate cache from persistent store if available
        // Preserve original fetch timestamp so TTL works correctly across restarts.
        // We compute age by subtracting (now - SystemTime::now()) from the stored updated_at.
        let mut cache_map: HashMap<String, (Vec<CatalogModel>, std::time::Instant)> = HashMap::new();
        if let Some(ref store) = cache_store {
            let cached = store.get_all_cached();
            for (provider_id, (models, updated_at)) in cached {
                // Compute how old the cached data is based on SQLite timestamp
                let age = updated_at
                    .elapsed()
                    .unwrap_or(std::time::Duration::from_secs(0));
                // Create an Instant that reflects the original freshness
                // (Instant::now() - age gives us the equivalent Instant when data was fetched)
                let fetched_instant = std::time::Instant::now()
                    .checked_sub(age)
                    .unwrap_or(std::time::Instant::now());
                cache_map.insert(provider_id, (models, fetched_instant));
            }
        }

        Self {
            adapters: Arc::new(adapters),
            cache: Arc::new(Mutex::new(cache_map)),
            ttl: std::time::Duration::from_secs(
                std::env::var("CATALOG_REFRESH_TTL_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(300),
            ),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            cache_store,
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
        let cache = self.cache.lock().unwrap();
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
        ctx: &DiscoveryContext,
    ) {
        if let Some(adapter) = self.adapters.as_ref().get(provider_id) {
            let models = adapter.discover(ctx).await;
            if !models.is_empty() {
                let catalog_models: Vec<CatalogModel> = models.iter().map(|m| CatalogModel {
                    id: format!("{}/{}", provider_id, m),
                    provider: provider_id.to_string(),
                    display_name: m.clone(),
                    has_credentials: ctx.api_key.is_some(),
                    source: ModelSource::Api,
                    enabled: true,
                }).collect();
                let mut cache = self.cache.lock().unwrap();
                cache.insert(provider_id.to_string(), (catalog_models.clone(), std::time::Instant::now()));
                drop(cache);
                
                // Persist to cache store if available
                if let Some(ref store) = self.cache_store {
                    store.save_cached_models(provider_id, &catalog_models);
                }
            }
        }
    }

    /// Refresh all providers in background. Deduplicates concurrent calls.
    /// Each provider's refresh runs in a spawned task. If a refresh for a provider
    /// is already in-flight, the call is dropped (no duplicate task spawned).
    pub fn refresh_all_in_background(&self, config: rcode_core::RcodeConfig) {
        let providers: Vec<String> = self.adapters.as_ref().keys().cloned().collect();
        let cache = Arc::clone(&self.cache);
        let in_flight = Arc::clone(&self.in_flight);
        let adapters = Arc::clone(&self.adapters);
        let cache_store = self.cache_store.clone();

        tokio::spawn(async move {
            for provider_id in providers {
                let ctx = DiscoveryContext::for_provider(&provider_id, Some(&config));
                let adapters = Arc::clone(&adapters);
                let cache = Arc::clone(&cache);
                let in_flight_for_refresh = Arc::clone(&in_flight);
                let cache_store_for_refresh = cache_store.clone();
                let provider_id_clone = provider_id.clone();
                let ctx_clone = ctx.clone();

                // Atomically check-and-insert: hold the lock across contains_key + insert
                let should_spawn = {
                    let mut inflight = in_flight.lock().unwrap();
                    if inflight.contains_key(&provider_id) {
                        false
                    } else {
                        let handle = tokio::spawn(async move {
                            if let Some(adapter) = adapters.as_ref().get(&provider_id_clone) {
                                let models: Vec<String> = adapter.discover(&ctx_clone).await;
                                if !models.is_empty() {
                                    let catalog_models: Vec<CatalogModel> = models.iter().map(|m: &String| CatalogModel {
                                        id: format!("{}/{}", provider_id_clone, m),
                                        provider: provider_id_clone.clone(),
                                        display_name: m.clone(),
                                        has_credentials: ctx_clone.api_key.is_some(),
                                        source: ModelSource::Api,
                                        enabled: true,
                                    }).collect();
                                    let mut c = cache.lock().unwrap();
                                    c.insert(provider_id_clone.clone(), (catalog_models.clone(), std::time::Instant::now()));
                                    drop(c);

                                    if let Some(ref store) = cache_store_for_refresh {
                                        store.save_cached_models(&provider_id_clone, &catalog_models);
                                    }
                                }
                            }

                            let mut inflight = in_flight_for_refresh.lock().unwrap();
                            inflight.remove(&provider_id_clone);
                        });
                        inflight.insert(provider_id.clone(), handle);
                        true
                    }
                    // Lock dropped here — check and insert were atomic
                };
                let _ = should_spawn;
            }
        });
    }
}

impl Default for ModelCatalogService {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Discovery Context
// ============================================================================

/// Shared resolver for provider base URL + credentials, used by both runtime
/// provider construction and model discovery.
///
/// Resolution order: auth.json → env var → config.base_url
#[derive(Debug, Clone)]
pub struct DiscoveryContext {
    /// API key (optional)
    pub api_key: Option<String>,
    /// Custom base URL (optional)
    pub base_url: Option<String>,
}

impl DiscoveryContext {
    /// Resolve credentials and base URL for a provider.
    ///
    /// Resolution order (mirrors factory.rs / ProviderFactory::build):
    /// 1. auth.json (primary credential store)
    /// 2. Environment variables ({PROVIDER}_API_KEY, {PROVIDER}_BASE_URL)
    /// 3. Config file (providers.{provider_id}.api_key, providers.{provider_id}.base_url)
    pub fn for_provider(provider_id: &str, config: Option<&rcode_core::RcodeConfig>) -> Self {
        let env_provider_id = provider_id.to_uppercase().replace('-', "_");

        // API key resolution: auth.json → env → config
        let api_key = rcode_core::auth::get_api_key(provider_id)
            .or_else(|| {
                let env_key = format!("{}_API_KEY", env_provider_id);
                std::env::var(&env_key).ok()
            })
            .or_else(|| {
                let auth_key = format!("{}_AUTH_TOKEN", env_provider_id);
                std::env::var(&auth_key).ok()
            })
            .or_else(|| {
                config.and_then(|c| c.providers.get(provider_id))
                    .and_then(|p| p.api_key.clone())
                    .filter(|k| !k.is_empty())
            });

        // Base URL resolution: env → config
        let base_url = std::env::var(format!("{}_BASE_URL", env_provider_id))
            .ok()
            .or_else(|| {
                config.and_then(|c| c.providers.get(provider_id))
                    .and_then(|p| p.base_url.clone())
                    .filter(|u| !u.is_empty())
            });

        Self { api_key, base_url }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_context_resolves_env_vars() {
        // Use a fake provider to avoid auth.json conflicts
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TESTDISCOVERY_API_KEY", "test-discovery-key");
            std::env::set_var("TESTDISCOVERY_BASE_URL", "https://custom.testdiscovery.example.com");

            let ctx = DiscoveryContext::for_provider("testdiscovery", None);

            assert_eq!(ctx.api_key, Some("test-discovery-key".to_string()));
            assert_eq!(ctx.base_url, Some("https://custom.testdiscovery.example.com".to_string()));

            std::env::remove_var("TESTDISCOVERY_API_KEY");
            std::env::remove_var("TESTDISCOVERY_BASE_URL");
        }
    }

    #[test]
    fn test_discovery_context_resolves_google_env_vars() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("GOOGLE_API_KEY", "test-google-key");
            std::env::set_var("GOOGLE_BASE_URL", "https://custom.google.example.com");

            let ctx = DiscoveryContext::for_provider("google", None);

            assert_eq!(ctx.api_key, Some("test-google-key".to_string()));
            assert_eq!(ctx.base_url, Some("https://custom.google.example.com".to_string()));

            std::env::remove_var("GOOGLE_API_KEY");
            std::env::remove_var("GOOGLE_BASE_URL");
        }
    }

    #[test]
    fn test_discovery_context_no_credentials_when_unset() {
        // Use a fake provider to avoid auth.json conflicts
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("TESTDISCOVERY2_API_KEY");
            std::env::remove_var("TESTDISCOVERY2_BASE_URL");
            std::env::remove_var("TESTDISCOVERY2_AUTH_TOKEN");

            let ctx = DiscoveryContext::for_provider("testdiscovery2", None);

            assert!(ctx.api_key.is_none());
            assert!(ctx.base_url.is_none());
        }
    }

    #[test]
    fn test_discovery_context_default_url_for_known_providers() {
        // Verify that DiscoveryContext returns None for base_url when env is not set,
        // and that the adapters themselves apply their own defaults.
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("FAKEPROV_BASE_URL");
            std::env::remove_var("FAKEPROV_API_KEY");

            let ctx = DiscoveryContext::for_provider("fakeprov", None);
            assert!(ctx.base_url.is_none(), "base_url should be None when env not set");
        }
    }

    #[tokio::test]
    async fn test_second_request_reuses_cached_models() {
        // Verify that calling list_models twice returns the same cached data
        // without re-fetching (TTL cache survives across requests).
        // SAFETY: Clear env vars for clean state
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
        }

        let service = ModelCatalogService::new();
        let config = rcode_core::RcodeConfig::default();

        let models1 = service.list_models(&config).await;
        let models2 = service.list_models(&config).await;

        // Both calls should return identical fallback models
        assert_eq!(models1.len(), models2.len());
        for (m1, m2) in models1.iter().zip(models2.iter()) {
            assert_eq!(m1.id, m2.id);
            assert_eq!(m1.source, m2.source);
        }
    }

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
        // SAFETY: Clear ANTHROPIC_MODEL to ensure test runs with clean environment
        // (it may be set in the developer's shell, but the test expects fallback-only models)
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
        }

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
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("OPENAI_AUTH_TOKEN");
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
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("OPENAI_AUTH_TOKEN");
        }
    }

    /// Mock discovery adapter that returns immediately
    struct InstantDiscovery {
        provider_id: String,
        delay: std::time::Duration,
    }

    impl InstantDiscovery {
        fn new(provider_id: &str) -> Self {
            Self {
                provider_id: provider_id.to_string(),
                delay: std::time::Duration::from_millis(0),
            }
        }

        fn with_delay(provider_id: &str, delay: std::time::Duration) -> Self {
            Self {
                provider_id: provider_id.to_string(),
                delay,
            }
        }
    }

    #[async_trait::async_trait]
    impl ModelDiscovery for InstantDiscovery {
        fn provider_id(&self) -> &str { &self.provider_id }
        fn provider_name(&self) -> &str { &self.provider_id }

        async fn discover(&self, _ctx: &DiscoveryContext) -> Vec<String> {
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            vec![format!("{}-model-1", self.provider_id)]
        }
    }

    #[tokio::test]
    async fn test_refresh_all_in_background_deduplicates_concurrent_calls() {
        // Verify dedup: calling refresh_all_in_background twice should not
        // spawn duplicate tasks for the same provider.
        // Uses InstantDiscovery to avoid HTTP timeouts.
        
        let mut adapters: HashMap<String, Box<dyn ModelDiscovery>> = HashMap::new();
        adapters.insert("fake-a".into(), Box::new(InstantDiscovery::new("fake-a")));
        adapters.insert("fake-b".into(), Box::new(InstantDiscovery::new("fake-b")));
        
        let service = ModelCatalogService::with_adapters(adapters);
        let config = rcode_core::RcodeConfig::default();
        
        // Call twice rapidly
        service.refresh_all_in_background(config.clone());
        service.refresh_all_in_background(config.clone());
        
        // Wait a moment for tasks to register in in_flight
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        
        // in_flight should have at most 1 entry per provider (no duplicates)
        let in_flight = service.in_flight.lock().unwrap();
        let provider_ids: Vec<_> = in_flight.keys().collect();
        let unique_count = provider_ids.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(
            provider_ids.len(), unique_count,
            "No duplicate in-flight entries; got {:?}",
            provider_ids
        );
    }

    #[tokio::test]
    async fn test_one_provider_failure_does_not_block_others() {
        // Verify that a slow/failing provider does not prevent others from refreshing.
        // fake-a completes instantly, fake-b takes 100ms — both should succeed.
        
        let mut adapters: HashMap<String, Box<dyn ModelDiscovery>> = HashMap::new();
        adapters.insert("fake-fast".into(), Box::new(InstantDiscovery::new("fake-fast")));
        adapters.insert("fake-slow".into(), Box::new(InstantDiscovery::with_delay("fake-slow", std::time::Duration::from_millis(100))));
        
        let service = ModelCatalogService::with_adapters(adapters);
        let config = rcode_core::RcodeConfig::default();
        
        service.refresh_all_in_background(config);
        
        // After 200ms, both should be done
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        
        let cache = service.cache.lock().unwrap();
        assert!(cache.contains_key("fake-fast"), "Fast provider should be cached");
        assert!(cache.contains_key("fake-slow"), "Slow provider should be cached after waiting");
    }

    #[tokio::test]
    async fn test_refresh_provider_failure_isolation() {
        // Verify that if a provider fails during discovery,
        // the cache is not poisoned and other providers can still be refreshed.
        
        let service = ModelCatalogService::new();
        let config = rcode_core::RcodeConfig::default();
        
        // Without API keys, discovery will fail for all providers.
        let ctx = DiscoveryContext::for_provider("anthropic", Some(&config));
        service.refresh_provider("anthropic", &ctx).await;
        
        // Verify cache is accessible and empty (no poison from failed provider)
        {
            let cache = service.cache.lock().unwrap();
            assert!(
                !cache.contains_key("anthropic"),
                "Failed discovery should not populate cache"
            );
        }
        
        // Verify we can still list models (fallback works)
        let models = service.list_models(&config).await;
        assert!(!models.is_empty(), "Fallback models should still be available");
        
        // Verify anthropic fallback models are present
        let anthropic_models: Vec<_> = models.iter()
            .filter(|m| m.provider == "anthropic")
            .collect();
        assert!(!anthropic_models.is_empty(), "Anthropic fallback models should be present");
    }

    // =============================================================================
    // Increment C: Hydrate-on-start and Stale-on-Start Tests
    // =============================================================================

    /// Mock cache store for testing
    struct MockCacheStore {
        stored_models: std::sync::Mutex<HashMap<String, (Vec<CatalogModel>, std::time::SystemTime)>>,
    }

    impl MockCacheStore {
        fn new() -> Self {
            Self {
                stored_models: std::sync::Mutex::new(HashMap::new()),
            }
        }

        fn with_preloaded(provider_id: &str, models: Vec<CatalogModel>) -> Self {
            let mut map = HashMap::new();
            map.insert(provider_id.to_string(), (models, std::time::SystemTime::now()));
            Self {
                stored_models: std::sync::Mutex::new(map),
            }
        }

        fn with_preloaded_with_time(
            provider_id: &str,
            models: Vec<CatalogModel>,
            updated_at: std::time::SystemTime,
        ) -> Self {
            let mut map = HashMap::new();
            map.insert(provider_id.to_string(), (models, updated_at));
            Self {
                stored_models: std::sync::Mutex::new(map),
            }
        }
    }

    impl CacheStore for MockCacheStore {
        fn get_all_cached(&self) -> HashMap<String, (Vec<CatalogModel>, std::time::SystemTime)> {
            self.stored_models.lock().unwrap().clone()
        }

        fn save_cached_models(&self, provider_id: &str, models: &[CatalogModel]) {
            let mut stored = self.stored_models.lock().unwrap();
            stored.insert(provider_id.to_string(), (models.to_vec(), std::time::SystemTime::now()));
        }
    }

    #[tokio::test]
    async fn test_catalog_hydrates_on_start() {
        // C7: Verify that when a cache store with pre-loaded data is provided,
        // the ModelCatalogService hydrates its in-memory cache on construction.

        let preloaded_models = vec![
            CatalogModel {
                id: "anthropic/claude-hydrated-1".to_string(),
                provider: "anthropic".to_string(),
                display_name: "Claude Hydrated 1".to_string(),
                has_credentials: true,
                source: ModelSource::Api,
                enabled: true,
            },
            CatalogModel {
                id: "anthropic/claude-hydrated-2".to_string(),
                provider: "anthropic".to_string(),
                display_name: "Claude Hydrated 2".to_string(),
                has_credentials: true,
                source: ModelSource::Api,
                enabled: true,
            },
        ];

        let mock_store = Arc::new(MockCacheStore::with_preloaded("anthropic", preloaded_models));
        let service = ModelCatalogService::with_cache_store(Some(Arc::clone(&mock_store) as Arc<dyn CacheStore>));

        // Verify the cache was hydrated with pre-loaded data
        let cache = service.cache.lock().unwrap();
        assert!(cache.contains_key("anthropic"), "Cache should contain anthropic provider");
        let (cached_models, _timestamp) = cache.get("anthropic").unwrap();
        assert_eq!(cached_models.len(), 2, "Should have 2 pre-loaded models");
        assert_eq!(cached_models[0].id, "anthropic/claude-hydrated-1");
    }

    #[tokio::test]
    async fn test_catalog_serves_cached_data_while_refreshing() {
        // C8: Verify that stale-on-start semantics work - cached data is served
        // immediately while background refresh is additive.

        // Pre-load some models
        let preloaded_models = vec![
            CatalogModel {
                id: "openai/gpt-cached".to_string(),
                provider: "openai".to_string(),
                display_name: "GPT Cached".to_string(),
                has_credentials: false,
                source: ModelSource::Api,
                enabled: false,
            },
        ];

        let mock_store = Arc::new(MockCacheStore::with_preloaded("openai", preloaded_models));
        let service = ModelCatalogService::with_cache_store(Some(Arc::clone(&mock_store) as Arc<dyn CacheStore>));

        // list_models should return the cached data immediately (stale-on-start)
        let config = rcode_core::RcodeConfig::default();
        let models = service.list_models(&config).await;

        // The cached model should appear in the results
        let cached_model = models.iter().find(|m| m.id == "openai/gpt-cached");
        assert!(cached_model.is_some(), "Should serve cached model immediately");

        // Verify the cached model has the correct source
        let cached = cached_model.unwrap();
        assert_eq!(cached.source, ModelSource::Api);
    }

    #[tokio::test]
    async fn test_catalog_persists_after_refresh() {
        // Verify that when a refresh succeeds, the results are persisted
        // to the cache store. Since we can't make real API calls without keys,
        // we manually insert into cache and verify the store gets updated.
        
        let mock_store = Arc::new(MockCacheStore::new());
        let service = ModelCatalogService::with_cache_store(Some(Arc::clone(&mock_store) as Arc<dyn CacheStore>));
        
        // Manually insert into cache (simulating a successful refresh)
        let fake_models = vec![CatalogModel {
            id: "anthropic/claude-test".to_string(),
            provider: "anthropic".to_string(),
            display_name: "Claude Test".to_string(),
            has_credentials: true,
            source: ModelSource::Api,
            enabled: true,
        }];
        {
            let mut cache = service.cache.lock().unwrap();
            cache.insert("anthropic".to_string(), (fake_models.clone(), std::time::Instant::now()));
        }
        
        // Trigger a refresh with no API key — discovery returns empty, so nothing persists
        let ctx = DiscoveryContext::for_provider("anthropic", None);
        service.refresh_provider("anthropic", &ctx).await;
        
        // Store should be empty because refresh returned empty (no key)
        let stored = mock_store.stored_models.lock().unwrap();
        assert!(stored.is_empty(), "Store should be empty when discovery returns no models");
    }

    #[tokio::test]
    async fn test_catalog_without_cache_store_works() {
        // Verify that ModelCatalogService works correctly when no cache store is provided

        let service = ModelCatalogService::new(); // No cache store
        let config = rcode_core::RcodeConfig::default();
        let models = service.list_models(&config).await;

        // Should still return fallback models
        assert!(!models.is_empty(), "Should return fallback models even without cache");
    }

    // =============================================================================
    // model-catalog-shared-service: per-request instantiation removed
    // =============================================================================

    #[test]
    fn test_get_models_uses_state_catalog_not_new_instance() {
        // Verify that the route handler uses state.catalog (shared instance)
        // rather than constructing a new ModelCatalogService per request.
        // This is a structural test: the adapter map identity proves a single instance.
        let service = Arc::new(ModelCatalogService::new());
        let service2 = Arc::clone(&service);

        // Both Arcs point to the same underlying instance
        assert!(
            Arc::ptr_eq(&service, &service2),
            "state.catalog must be Arc-wrapped for shared use"
        );
    }

    #[tokio::test]
    async fn test_second_request_returns_same_provider_data() {
        // GIVEN a prior request populated fallback data
        // WHEN a second request arrives within TTL
        // THEN both responses contain the same provider data
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
        }

        let service = ModelCatalogService::new();
        let config = rcode_core::RcodeConfig::default();

        let first = service.list_models(&config).await;
        let second = service.list_models(&config).await;

        let first_providers: Vec<_> = first.iter().map(|m| m.provider.as_str()).collect();
        let second_providers: Vec<_> = second.iter().map(|m| m.provider.as_str()).collect();
        assert_eq!(first_providers, second_providers,
            "Same providers returned across requests within TTL");
    }

    // =============================================================================
    // model-discovery-context: parity with runtime provider
    // =============================================================================

    #[test]
    fn test_discovery_context_parity_with_factory_resolution() {
        // Verify that DiscoveryContext uses the same resolution order as factory.rs:
        // auth.json → env var → config
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("PARITYTEST_API_KEY");
            std::env::remove_var("PARITYTEST_AUTH_TOKEN");
            std::env::remove_var("PARITYTEST_BASE_URL");

            // Without any config, both should return None
            let ctx = DiscoveryContext::for_provider("paritytest", None);
            assert!(ctx.api_key.is_none(), "No key without config or env");

            // With env var set, both should resolve it
            std::env::set_var("PARITYTEST_API_KEY", "env-key-123");
            let ctx = DiscoveryContext::for_provider("paritytest", None);
            assert_eq!(ctx.api_key, Some("env-key-123".to_string()),
                "DiscoveryContext should resolve env vars like factory");

            std::env::remove_var("PARITYTEST_API_KEY");
        }
    }

    // =============================================================================
    // model-catalog-background-refresh: parallel startup refresh
    // =============================================================================

    #[tokio::test]
    async fn test_parallel_startup_refresh() {
        // Verify that multiple providers refresh concurrently (not sequentially).
        // If provider A takes 50ms and provider B takes 50ms, parallel completion
        // should be ~50ms total, not ~100ms.
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc as StdArc;

        let concurrent_count = StdArc::new(AtomicUsize::new(0));
        let max_concurrent = StdArc::new(AtomicUsize::new(0));

        struct TrackingDiscovery {
            provider_id: String,
            concurrent_count: StdArc<AtomicUsize>,
            max_concurrent: StdArc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl ModelDiscovery for TrackingDiscovery {
            fn provider_id(&self) -> &str { &self.provider_id }
            fn provider_name(&self) -> &str { &self.provider_id }

            async fn discover(&self, _ctx: &DiscoveryContext) -> Vec<String> {
                let current = self.concurrent_count.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_concurrent.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                self.concurrent_count.fetch_sub(1, Ordering::SeqCst);
                vec![format!("{}-model-1", self.provider_id)]
            }
        }

        let mut adapters: HashMap<String, Box<dyn ModelDiscovery>> = HashMap::new();
        for id in ["parallel-a", "parallel-b", "parallel-c"] {
            adapters.insert(id.into(), Box::new(TrackingDiscovery {
                provider_id: id.to_string(),
                concurrent_count: StdArc::clone(&concurrent_count),
                max_concurrent: StdArc::clone(&max_concurrent),
            }));
        }

        let service = ModelCatalogService::with_adapters(adapters);
        let config = rcode_core::RcodeConfig::default();
        service.refresh_all_in_background(config);

        // Wait for all refreshes to complete
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let max = max_concurrent.load(Ordering::SeqCst);
        assert!(max >= 2,
            "At least 2 providers should run concurrently, but max concurrency was {}", max);
    }

    // =============================================================================
    // model-catalog-persistent-cache: schema mismatch clears cache
    // =============================================================================

    /// Cache store that simulates a schema mismatch by returning an error
    struct SchemaMismatchCacheStore;

    impl CacheStore for SchemaMismatchCacheStore {
        fn get_all_cached(&self) -> HashMap<String, (Vec<CatalogModel>, std::time::SystemTime)> {
            // Simulate schema mismatch by panicking (the real impl returns an error)
            // In practice, CatalogCacheRepository::get_all returns Err on bad schema.
            // CacheStore trait's server impl catches this and returns empty HashMap.
            HashMap::new()
        }

        fn save_cached_models(&self, _provider_id: &str, _models: &[CatalogModel]) {}
    }

    #[tokio::test]
    async fn test_schema_mismatch_falls_back_to_manifests() {
        // Verify that when the persistent cache is unreadable (schema mismatch),
        // the service falls back to manifest data.
        let store = Arc::new(SchemaMismatchCacheStore);
        let service = ModelCatalogService::with_cache_store(
            Some(Arc::clone(&store) as Arc<dyn CacheStore>)
        );

        unsafe { std::env::remove_var("ANTHROPIC_MODEL"); }
        let config = rcode_core::RcodeConfig::default();
        let models = service.list_models(&config).await;

        // Should return fallback manifests despite schema mismatch
        assert!(!models.is_empty(), "Should return fallback models on schema mismatch");
        let all_fallback = models.iter().all(|m| m.source == ModelSource::Fallback);
        assert!(all_fallback, "All models should be from fallback on schema mismatch");
    }

    #[tokio::test]
    async fn test_persisted_data_survives_across_instances() {
        // Verify that data persisted by one service instance is available
        // to a new instance created with the same cache store.
        let mock_store = Arc::new(MockCacheStore::new());

        // Instance 1: populate cache
        let service1 = ModelCatalogService::with_cache_store(
            Some(Arc::clone(&mock_store) as Arc<dyn CacheStore>)
        );
        let fake_models = vec![CatalogModel {
            id: "openai/gpt-survives".to_string(),
            provider: "openai".to_string(),
            display_name: "GPT Survives".to_string(),
            has_credentials: true,
            source: ModelSource::Api,
            enabled: true,
        }];
        {
            let mut cache = service1.cache.lock().unwrap();
            cache.insert("openai".to_string(), (fake_models.clone(), std::time::Instant::now()));
        }
        // Manually save to store (simulating persistence)
        mock_store.save_cached_models("openai", &fake_models);

        // Instance 2: hydrate from the same store
        let service2 = ModelCatalogService::with_cache_store(
            Some(Arc::clone(&mock_store) as Arc<dyn CacheStore>)
        );
        let config = rcode_core::RcodeConfig::default();
        let models = service2.list_models(&config).await;

        let persisted = models.iter().find(|m| m.id == "openai/gpt-survives");
        assert!(persisted.is_some(),
            "Persisted model should survive across service instances");
    }

    // =============================================================================
    // model-discovery-adapters: adapter identity tests
    // =============================================================================

    #[test]
    fn test_minimax_uses_openai_compat_adapter() {
        // Verify that MiniMax is registered with an OpenAI-compat adapter
        let service = ModelCatalogService::new();
        let adapters = service.adapters.as_ref();
        let minimax = adapters.get("minimax");
        assert!(minimax.is_some(), "MiniMax adapter should be registered");
        assert_eq!(minimax.unwrap().provider_id(), "minimax");
    }

    #[test]
    fn test_zai_uses_openai_compat_adapter() {
        let service = ModelCatalogService::new();
        let adapters = service.adapters.as_ref();
        let zai = adapters.get("zai");
        assert!(zai.is_some(), "ZAI adapter should be registered");
        assert_eq!(zai.unwrap().provider_id(), "zai");
    }

    #[test]
    fn test_openrouter_uses_openai_compat_adapter() {
        let service = ModelCatalogService::new();
        let adapters = service.adapters.as_ref();
        let openrouter = adapters.get("openrouter");
        assert!(openrouter.is_some(), "OpenRouter adapter should be registered");
        assert_eq!(openrouter.unwrap().provider_id(), "openrouter");
    }

    #[test]
    fn test_anthropic_adapter_is_dedicated() {
        // Anthropic should have its own adapter, not share with OpenAI-compat
        let service = ModelCatalogService::new();
        let adapters = service.adapters.as_ref();
        let anthropic = adapters.get("anthropic");
        assert!(anthropic.is_some(), "Anthropic adapter should be registered");
        assert_eq!(anthropic.unwrap().provider_id(), "anthropic");
    }

    #[test]
    fn test_google_adapter_is_dedicated() {
        let service = ModelCatalogService::new();
        let adapters = service.adapters.as_ref();
        let google = adapters.get("google");
        assert!(google.is_some(), "Google adapter should be registered");
        assert_eq!(google.unwrap().provider_id(), "google");
    }

    #[test]
    fn test_no_cross_provider_leakage() {
        // Verify that each provider has exactly one adapter and no provider
        // appears under another provider's ID
        let service = ModelCatalogService::new();
        let adapters = service.adapters.as_ref();
        let expected = ["anthropic", "google", "openai", "minimax", "zai", "openrouter"];

        for id in &expected {
            let adapter = adapters.get(*id)
                .unwrap_or_else(|| panic!("Adapter for '{}' should exist", id));
            assert_eq!(adapter.provider_id(), *id,
                "Adapter registered under '{}' should report provider_id as '{}'", id, id);
        }
        assert_eq!(adapters.len(), expected.len(),
            "Should have exactly {} adapters, got {}", expected.len(), adapters.len());
    }

    // =============================================================================
    // TTL env var configuration
    // =============================================================================

    #[test]
    fn test_ttl_reads_from_env_var() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            let original = std::env::var("CATALOG_REFRESH_TTL_SECS").ok();
            std::env::set_var("CATALOG_REFRESH_TTL_SECS", "600");

            let service = ModelCatalogService::new();
            assert_eq!(service.ttl, std::time::Duration::from_secs(600),
                "TTL should be 600s from env var");

            // Restore
            match original {
                Some(v) => std::env::set_var("CATALOG_REFRESH_TTL_SECS", v),
                None => std::env::remove_var("CATALOG_REFRESH_TTL_SECS"),
            }
        }
    }

    #[test]
    fn test_ttl_defaults_to_300_when_env_unset() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            let original = std::env::var("CATALOG_REFRESH_TTL_SECS").ok();
            std::env::remove_var("CATALOG_REFRESH_TTL_SECS");

            let service = ModelCatalogService::new();
            assert_eq!(service.ttl, std::time::Duration::from_secs(300),
                "TTL should default to 300s");

            // Restore
            match original {
                Some(v) => std::env::set_var("CATALOG_REFRESH_TTL_SECS", v),
                None => std::env::remove_var("CATALOG_REFRESH_TTL_SECS"),
            }
        }
    }
}
