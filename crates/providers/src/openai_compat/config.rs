//! OpenAI-compatible adapter configuration
//!
//! This module provides the `OpenAiCompatConfig` struct that holds all static
//! configuration for an OpenAI-compatible provider. Rate limiting is attached
//! separately via `OpenAiCompatTransport::with_rate_limit()`.

use rcode_core::provider::ProviderCapabilities;

/// Configuration for an OpenAI-compatible provider.
///
/// This struct is intentionally plain (no behavior, no async) — it holds only
/// static configuration. The actual HTTP client, streaming, and rate limiting
/// are owned by `OpenAiCompatTransport`.
///
/// Each façade (OpenAI, MiniMax, OpenRouter, ZAI) constructs its own config
/// with provider-specific defaults. The adapter itself reads no env vars.
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    /// API key for authentication
    pub api_key: String,
    /// Base URL for the API endpoint (e.g., "https://api.openai.com" or "https://api.minimax.chat/v1")
    pub base_url: String,
    /// Provider identifier (e.g., "openai", "minimax", "openrouter", "zai")
    pub provider_id: String,
    /// Custom headers to include with each request
    pub custom_headers: Vec<(String, String)>,
    /// Optional model info lookup function
    pub model_info_fn: Option<fn(&str) -> Option<rcode_core::ModelInfo>>,
    /// Provider capabilities
    pub capabilities: ProviderCapabilities,
    /// When true, skip the automatic `/v1` prefix insertion in the chat completions URL.
    /// Required for providers like GitHub Copilot whose endpoint is `/chat/completions`
    /// (no `/v1/` segment) rather than the standard `/v1/chat/completions`.
    pub no_v1_prefix: bool,
}

impl OpenAiCompatConfig {
    /// Create a new config with required fields
    pub fn new(api_key: String, base_url: String, provider_id: String) -> Self {
        Self {
            api_key,
            base_url,
            provider_id,
            custom_headers: Vec::new(),
            model_info_fn: None,
            capabilities: ProviderCapabilities::all(),
            no_v1_prefix: false,
        }
    }

    /// Set custom headers
    pub fn with_custom_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.custom_headers = headers;
        self
    }

    /// Set the model info lookup function
    pub fn with_model_info_fn(mut self, f: fn(&str) -> Option<rcode_core::ModelInfo>) -> Self {
        self.model_info_fn = Some(f);
        self
    }

    /// Set provider capabilities
    pub fn with_capabilities(mut self, capabilities: ProviderCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Skip the automatic `/v1` prefix when building the chat completions URL.
    /// Use this for providers like GitHub Copilot whose endpoint does not include `/v1/`.
    pub fn with_no_v1_prefix(mut self) -> Self {
        self.no_v1_prefix = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_new() {
        let config = OpenAiCompatConfig::new(
            "test-api-key".to_string(),
            "https://api.openai.com".to_string(),
            "openai".to_string(),
        );

        assert_eq!(config.api_key, "test-api-key");
        assert_eq!(config.base_url, "https://api.openai.com");
        assert_eq!(config.provider_id, "openai");
        assert!(config.custom_headers.is_empty());
        assert!(config.model_info_fn.is_none());
        assert_eq!(config.capabilities, ProviderCapabilities::all());
    }

    #[test]
    fn test_config_with_custom_headers() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.test.com".to_string(),
            "test".to_string(),
        )
        .with_custom_headers(vec![("X-Custom".to_string(), "value".to_string())]);

        assert_eq!(config.custom_headers.len(), 1);
        assert_eq!(config.custom_headers[0].0, "X-Custom");
    }

    #[test]
    fn test_config_with_model_info_fn() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.test.com".to_string(),
            "test".to_string(),
        )
        .with_model_info_fn(|_| None);

        assert!(config.model_info_fn.is_some());
    }

    #[test]
    fn test_config_with_capabilities() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.test.com".to_string(),
            "test".to_string(),
        )
        .with_capabilities(ProviderCapabilities::chat_only());

        assert_eq!(config.capabilities, ProviderCapabilities::chat_only());
    }

    #[test]
    fn test_config_clone_is_independent() {
        let config = OpenAiCompatConfig::new(
            "original".to_string(),
            "https://api.test.com".to_string(),
            "test".to_string(),
        );

        let mut cloned = config.clone();
        cloned.api_key = "modified".to_string();

        assert_eq!(config.api_key, "original");
        assert_eq!(cloned.api_key, "modified");
    }
}
