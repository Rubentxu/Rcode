//! OpenAI provider implementation
//!
//! This module provides a thin façade over the OpenAI-compatible transport layer.
//! All protocol encoding/decoding and HTTP transport is handled by `OpenAiCompatTransport`.

use async_trait::async_trait;

use rcode_core::{
    CompletionRequest, CompletionResponse, ModelInfo,
    StreamingResponse,
};
use rcode_core::provider::ProviderCapabilities;

use super::LlmProvider;
use super::openai_compat::{OpenAiCompatConfig, OpenAiCompatTransport};

/// OpenAI provider façade
///
/// This provider delegates all HTTP communication to the shared `OpenAiCompatTransport`.
/// The façade is responsible for:
/// - Reading OpenAI-specific environment variables
/// - Constructing the `OpenAiCompatConfig` with OpenAI-specific defaults
/// - Owning provider identity (model_info, provider_id, capabilities)
pub struct OpenAIProvider {
    config: OpenAiCompatConfig,
    transport: OpenAiCompatTransport,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider with the given API key
    pub fn new(api_key: String) -> Self {
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com".to_string());
        
        let custom_headers = std::env::var("OPENAI_CUSTOM_HEADERS")
            .map(|h| {
                serde_json::from_str::<Vec<(String, String)>>(&h)
                    .unwrap_or_else(|_| vec![])
            })
            .unwrap_or_default();
        
        let config = OpenAiCompatConfig::new(api_key, base_url, "openai".to_string())
            .with_custom_headers(custom_headers);
        
        let transport = OpenAiCompatTransport::new(config.clone());
        Self { config, transport }
    }

    /// Create a new OpenAI provider with a custom base URL
    /// This is useful for providers like OpenRouter that use OpenAI-compatible APIs
    pub fn new_with_base_url(api_key: String, base_url: String) -> Self {
        let custom_headers = std::env::var("OPENAI_CUSTOM_HEADERS")
            .map(|h| {
                serde_json::from_str::<Vec<(String, String)>>(&h)
                    .unwrap_or_else(|_| vec![])
            })
            .unwrap_or_default();

        let config = OpenAiCompatConfig::new(api_key, base_url, "openai".to_string())
            .with_custom_headers(custom_headers);
        
        let transport = OpenAiCompatTransport::new(config.clone());
        Self { config, transport }
    }

    /// Create a new OpenAI provider with a custom base URL and no `/v1/` prefix.
    /// Required for providers like GitHub Copilot whose endpoint is `/chat/completions`
    /// (without the `/v1/` segment).
    pub fn new_with_base_url_no_v1(api_key: String, base_url: String) -> Self {
        let config = OpenAiCompatConfig::new(api_key, base_url, "github-copilot".to_string())
            .with_no_v1_prefix();
        let transport = OpenAiCompatTransport::new(config.clone());
        Self { config, transport }
    }

    /// Attach rate limiting to this provider
    pub fn with_rate_limit(self, capacity: u64, refill_rate: f64) -> Self {
        let transport = OpenAiCompatTransport::new(self.config.clone())
            .with_rate_limit(capacity, refill_rate);
        Self { config: self.config, transport }
    }

    fn model_info_internal(model_id: &str) -> Option<ModelInfo> {
        match model_id {
            "gpt-4o" => Some(ModelInfo {
                id: "gpt-4o".into(),
                name: "GPT-4o".into(),
                provider: "openai".into(),
                context_window: 128000,
                max_output_tokens: Some(16384),
            }),
            _ => None,
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn complete(&self, req: CompletionRequest) -> rcode_core::error::Result<CompletionResponse> {
        self.transport.post(req).await
    }
    
    async fn stream(&self, req: CompletionRequest) -> rcode_core::error::Result<StreamingResponse> {
        self.transport.post_streaming(req).await
    }
    
    fn model_info(&self, model_id: &str) -> Option<ModelInfo> {
        Self::model_info_internal(model_id)
    }
    
    fn provider_id(&self) -> &str {
        "openai"
    }

    fn abort(&self) {
        self.transport.abort();
    }
    
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_new() {
        let provider = OpenAIProvider::new("test-api-key".to_string());
        assert_eq!(provider.provider_id(), "openai");
    }

    #[test]
    fn test_provider_id() {
        let provider = OpenAIProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "openai");
    }

    #[test]
    fn test_model_info_gpt_4o() {
        let provider = OpenAIProvider::new("test".to_string());
        let info = provider.model_info("gpt-4o").unwrap();
        assert_eq!(info.id, "gpt-4o");
        assert_eq!(info.name, "GPT-4o");
        assert_eq!(info.provider, "openai");
        assert_eq!(info.context_window, 128000);
        assert_eq!(info.max_output_tokens, Some(16384));
    }

    #[test]
    fn test_model_info_unknown() {
        let provider = OpenAIProvider::new("test".to_string());
        let info = provider.model_info("unknown-model");
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_abort_no_panic() {
        let provider = OpenAIProvider::new("test".to_string());
        // abort() should not panic even with no active stream
        provider.abort();
    }
}
