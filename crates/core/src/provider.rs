//! LLM Provider trait

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::message::Message;
use crate::error::Result;

/// Provider capabilities - describes what features a provider/model supports
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapabilities {
    /// Whether the provider supports tool calling (function calling)
    pub supports_tool_calling: bool,
    /// Whether the provider supports streaming tool calls (incremental tool call events)
    pub supports_streaming_tool_calls: bool,
    /// Whether the provider supports reasoning/thinking (e.g., Claude's extended thinking)
    pub supports_reasoning: bool,
    /// Whether the provider supports system prompt
    pub supports_system_prompt: bool,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            supports_tool_calling: false,
            supports_streaming_tool_calls: false,
            supports_reasoning: false,
            supports_system_prompt: true,
        }
    }
}

impl ProviderCapabilities {
    /// Returns a ProviderCapabilities with all features enabled
    pub fn all() -> Self {
        Self {
            supports_tool_calling: true,
            supports_streaming_tool_calls: true,
            supports_reasoning: true,
            supports_system_prompt: true,
        }
    }
    
    /// Returns a ProviderCapabilities with only chat (no tool calling)
    pub fn chat_only() -> Self {
        Self::default()
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse>;
    fn model_info(&self, model_id: &str) -> Option<ModelInfo>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub system_prompt: Option<String>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    /// Reasoning effort override (e.g. "low", "medium", "high")
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub context_window: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
}

pub struct StreamingResponse {
    pub events: Pin<Box<dyn Stream<Item = StreamingEvent> + Send>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamingEvent {
    Text {
        delta: String,
    },
    Reasoning {
        delta: String,
    },
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallArg {
        id: String,
        name: String,
        value: String,
    },
    ToolCallEnd {
        id: String,
    },
    ContentBlock {
        content: Box<ContentBlock>,
    },
    Finish {
        stop_reason: StopReason,
        usage: TokenUsage,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_request_deserialization_with_reasoning_effort() {
        let json = r#"{
            "model": "claude-sonnet-4-5",
            "messages": [],
            "tools": [],
            "max_tokens": 4096,
            "reasoning_effort": "high"
        }"#;

        let req: CompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "claude-sonnet-4-5");
        assert_eq!(req.max_tokens, Some(4096));
        assert_eq!(req.reasoning_effort, Some("high".to_string()));
    }

    #[test]
    fn test_completion_request_deserialization_without_reasoning_effort() {
        let json = r#"{
            "model": "claude-sonnet-4-5",
            "messages": [],
            "tools": [],
            "max_tokens": 4096
        }"#;

        let req: CompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "claude-sonnet-4-5");
        assert_eq!(req.max_tokens, Some(4096));
        assert_eq!(req.reasoning_effort, None);
    }

    #[test]
    fn test_completion_request_serialization_skips_none_reasoning_effort() {
        let req = CompletionRequest {
            model: "claude-sonnet-4-5".to_string(),
            messages: vec![],
            system_prompt: None,
            tools: vec![],
            temperature: None,
            max_tokens: Some(4096),
            reasoning_effort: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("reasoning_effort"));
    }

    #[test]
    fn test_completion_request_serialization_includes_reasoning_effort_when_set() {
        let req = CompletionRequest {
            model: "claude-sonnet-4-5".to_string(),
            messages: vec![],
            system_prompt: None,
            tools: vec![],
            temperature: None,
            max_tokens: Some(4096),
            reasoning_effort: Some("high".to_string()),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("reasoning_effort"));
        assert!(json.contains("high"));
    }
}
