//! MiniMax provider implementation
//!
//! MiniMax is an OpenAI-compatible API at https://api.minimax.chat/v1

use async_trait::async_trait;

use rcode_core::{
    CompletionRequest, CompletionResponse, ModelInfo,
    StreamingResponse, error::Result,
};
use rcode_core::provider::ProviderCapabilities;

use super::openai::OpenAIProvider;
use super::LlmProvider;

/// MiniMax provider that wraps OpenAIProvider with MiniMax-specific configuration
pub struct MiniMaxProvider {
    inner: OpenAIProvider,
}

impl MiniMaxProvider {
    /// Create a new MiniMax provider with the given API key
    pub fn new(api_key: String) -> Self {
        Self {
            inner: OpenAIProvider::new_with_base_url(
                api_key,
                "https://api.minimax.chat/v1".to_string(),
            ),
        }
    }

    /// Create a new MiniMax provider with a custom base URL
    pub fn new_with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            inner: OpenAIProvider::new_with_base_url(api_key, base_url),
        }
    }
}

#[async_trait]
impl LlmProvider for MiniMaxProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.inner.complete(req).await
    }

    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse> {
        self.inner.stream(req).await
    }

    fn model_info(&self, _model_id: &str) -> Option<ModelInfo> {
        // MiniMax has multiple models, no static list
        None
    }

    fn provider_id(&self) -> &str {
        "minimax"
    }

    fn abort(&self) {
        self.inner.abort()
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
        let provider = MiniMaxProvider::new("test-api-key".to_string());
        assert_eq!(provider.provider_id(), "minimax");
    }

    #[test]
    fn test_provider_id() {
        let provider = MiniMaxProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "minimax");
    }

    #[test]
    fn test_model_info_returns_none() {
        let provider = MiniMaxProvider::new("test".to_string());
        assert!(provider.model_info("any-model").is_none());
    }

    #[test]
    fn test_provider_with_custom_base_url() {
        let provider = MiniMaxProvider::new_with_base_url(
            "test-api-key".to_string(),
            "https://custom.minimax.example.com/v1".to_string(),
        );
        assert_eq!(provider.provider_id(), "minimax");
    }

    #[test]
    fn test_provider_abort_does_not_panic() {
        let provider = MiniMaxProvider::new("test".to_string());
        provider.abort();
    }
}
