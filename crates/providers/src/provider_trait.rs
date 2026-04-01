//! LLM Provider trait

use async_trait::async_trait;

use rcode_core::{
    CompletionRequest, CompletionResponse, 
    StreamingResponse, ModelInfo, error::Result,
};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse>;
    fn model_info(&self, model_id: &str) -> Option<ModelInfo>;
    fn provider_id(&self) -> &str;
    
    /// Abort any in-progress streaming request
    fn abort(&self);
}
