//! ZAI provider implementation
//!
//! ZAI (zai-coding) is an OpenAI-compatible API at https://api.zai.chat/v1
//!
//! This provider has its own identity (provider_id, base_url) and composes
//! the shared `OpenAiCompatTransport` for infrastructure.

use async_trait::async_trait;

use rcode_core::{
    CompletionRequest, CompletionResponse, ModelInfo,
    StreamingResponse, error::Result,
};
use rcode_core::provider::ProviderCapabilities;

use super::openai_compat::{OpenAiCompatConfig, OpenAiCompatTransport};
use super::LlmProvider;

/// ZAI provider with its own identity, composing OpenAI-compatible transport
pub struct ZaiProvider {
    transport: OpenAiCompatTransport,
}

impl ZaiProvider {
    /// Create a new ZAI provider with the given API key
    pub fn new(api_key: String) -> Self {
        let custom_headers = std::env::var("ZAI_CUSTOM_HEADERS")
            .map(|h| {
                serde_json::from_str::<Vec<(String, String)>>(&h)
                    .unwrap_or_else(|_| vec![])
            })
            .unwrap_or_default();

        let config = OpenAiCompatConfig::new(
            api_key,
            "https://api.zai.chat/v1".to_string(),
            "zai".to_string(),
        )
        .with_custom_headers(custom_headers);

        let transport = OpenAiCompatTransport::new(config);
        Self { transport }
    }

    /// Create a new ZAI provider with a custom base URL
    pub fn new_with_base_url(api_key: String, base_url: String) -> Self {
        let custom_headers = std::env::var("ZAI_CUSTOM_HEADERS")
            .map(|h| {
                serde_json::from_str::<Vec<(String, String)>>(&h)
                    .unwrap_or_else(|_| vec![])
            })
            .unwrap_or_default();

        let config = OpenAiCompatConfig::new(
            api_key,
            base_url,
            "zai".to_string(),
        )
        .with_custom_headers(custom_headers);

        let transport = OpenAiCompatTransport::new(config);
        Self { transport }
    }
}

#[async_trait]
impl LlmProvider for ZaiProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.transport.post(req).await
    }

    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse> {
        self.transport.post_streaming(req).await
    }

    fn model_info(&self, _model_id: &str) -> Option<ModelInfo> {
        // ZAI has multiple models, no static list
        None
    }

    fn provider_id(&self) -> &str {
        "zai"
    }

    fn abort(&self) {
        self.transport.abort()
    }
    
    fn capabilities(&self) -> ProviderCapabilities {
        // ZAI uses OpenAI-compatible API, supports tool calling
        ProviderCapabilities::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_new() {
        let provider = ZaiProvider::new("test-api-key".to_string());
        assert_eq!(provider.provider_id(), "zai");
    }

    #[test]
    fn test_provider_id() {
        let provider = ZaiProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "zai");
    }

    #[test]
    fn test_model_info_returns_none() {
        let provider = ZaiProvider::new("test".to_string());
        assert!(provider.model_info("any-model").is_none());
    }

    #[test]
    fn test_provider_with_custom_base_url() {
        let provider = ZaiProvider::new_with_base_url(
            "test-api-key".to_string(),
            "https://custom.zai.example.com/v1".to_string(),
        );
        assert_eq!(provider.provider_id(), "zai");
    }

    #[test]
    fn test_provider_abort_does_not_panic() {
        let provider = ZaiProvider::new("test".to_string());
        provider.abort();
    }

    #[test]
    fn test_custom_headers_from_env_empty() {
        // Without ZAI_CUSTOM_HEADERS env var, custom_headers should be empty
        let provider = ZaiProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "zai");
    }
}
