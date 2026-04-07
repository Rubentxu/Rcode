//! OpenRouter provider implementation
//!
//! OpenRouter uses the OpenAI-compatible API at https://openrouter.ai/api/v1

use async_trait::async_trait;

use rcode_core::{
    CompletionRequest, CompletionResponse, ModelInfo,
    StreamingResponse, error::Result,
};
use rcode_core::provider::ProviderCapabilities;

use super::openai::OpenAIProvider;
use super::LlmProvider;

/// OpenRouter provider that wraps OpenAIProvider with OpenRouter-specific configuration
pub struct OpenRouterProvider {
    inner: OpenAIProvider,
}

impl OpenRouterProvider {
    /// Create a new OpenRouter provider with the given API key
    pub fn new(api_key: String) -> Self {
        Self {
            inner: OpenAIProvider::new_with_base_url(
                api_key,
                "https://openrouter.ai/api/v1".to_string(),
            ),
        }
    }

    /// Create a new OpenRouter provider with a custom base URL
    pub fn new_with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            inner: OpenAIProvider::new_with_base_url(api_key, base_url),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenRouterProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.inner.complete(req).await
    }

    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse> {
        self.inner.stream(req).await
    }

    fn model_info(&self, _model_id: &str) -> Option<ModelInfo> {
        // OpenRouter has hundreds of models, no static list
        None
    }

    fn provider_id(&self) -> &str {
        "openrouter"
    }

    fn abort(&self) {
        self.inner.abort()
    }
    
    fn capabilities(&self) -> ProviderCapabilities {
        // OpenRouter uses OpenAI-compatible API, supports tool calling
        ProviderCapabilities::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_new() {
        let provider = OpenRouterProvider::new("test-api-key".to_string());
        assert_eq!(provider.provider_id(), "openrouter");
    }

    #[test]
    fn test_provider_id() {
        let provider = OpenRouterProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "openrouter");
    }

    #[test]
    fn test_model_info_returns_none() {
        let provider = OpenRouterProvider::new("test".to_string());
        // OpenRouter has hundreds of models, so model_info returns None
        assert!(provider.model_info("any-model").is_none());
    }
}