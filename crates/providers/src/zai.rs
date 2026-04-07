//! ZAI provider implementation
//!
//! ZAI (zai-coding) is an OpenAI-compatible API at https://api.zai.chat/v1

use async_trait::async_trait;

use rcode_core::{
    CompletionRequest, CompletionResponse, ModelInfo,
    StreamingResponse, error::Result,
};
use rcode_core::provider::ProviderCapabilities;

use super::openai::OpenAIProvider;
use super::LlmProvider;

/// ZAI provider that wraps OpenAIProvider with ZAI-specific configuration
pub struct ZaiProvider {
    inner: OpenAIProvider,
}

impl ZaiProvider {
    /// Create a new ZAI provider with the given API key
    pub fn new(api_key: String) -> Self {
        Self {
            inner: OpenAIProvider::new_with_base_url(
                api_key,
                "https://api.zai.chat/v1".to_string(),
            ),
        }
    }

    /// Create a new ZAI provider with a custom base URL
    pub fn new_with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            inner: OpenAIProvider::new_with_base_url(api_key, base_url),
        }
    }
}

#[async_trait]
impl LlmProvider for ZaiProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.inner.complete(req).await
    }

    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse> {
        self.inner.stream(req).await
    }

    fn model_info(&self, _model_id: &str) -> Option<ModelInfo> {
        // ZAI has multiple models, no static list
        None
    }

    fn provider_id(&self) -> &str {
        "zai"
    }

    fn abort(&self) {
        self.inner.abort()
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
}
