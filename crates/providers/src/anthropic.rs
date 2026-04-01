//! Anthropic provider implementation

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use rcode_core::{
    CompletionRequest, CompletionResponse, StreamingResponse,
    ContentBlock as CoreContentBlock, ModelInfo, StreamingEvent,
    ToolDefinition, TokenUsage, error::Result,
};
use rcode_core::provider::StopReason;

use super::rate_limit::TokenBucket;
use super::LlmProvider;

pub struct AnthropicProvider {
    api_key: String,
    http_client: Client,
    rate_limiter: Option<Arc<TokenBucket>>,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http_client: Client::new(),
            rate_limiter: None,
        }
    }

    pub fn with_rate_limit(mut self, capacity: u64, refill_rate: f64) -> Self {
        self.rate_limiter = Some(Arc::new(TokenBucket::new(capacity, refill_rate)));
        self
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        if let Some(limiter) = &self.rate_limiter {
            if let Err(wait_time) = limiter.try_acquire(1) {
                tokio::time::sleep(wait_time).await;
                let _ = limiter.try_acquire(1);
            }
        }

        let body = AnthropicRequest {
            model: req.model.clone(),
            messages: req.messages.into_iter().map(into_anthropic_message).collect(),
            max_tokens: req.max_tokens.unwrap_or(4096),
            system: req.system_prompt,
            tools: Some(req.tools.into_iter().map(into_anthropic_tool).collect()),
        };
        
        let response = self.http_client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::OpenCodeError::Provider(format!("Network error: {}", e)))?;
        
        let resp: AnthropicResponse = response.json().await
            .map_err(|e| rcode_core::OpenCodeError::Provider(format!("Parse error: {}", e)))?;
        
        Ok(CompletionResponse {
            content: resp.content.first()
                .and_then(|c| if let AnthropicContentBlock::Text { text } = c { Some(text.clone()) } else { None })
                .unwrap_or_default(),
            reasoning: None,
            tool_calls: vec![],
            usage: TokenUsage {
                input_tokens: resp.usage.input_tokens,
                output_tokens: resp.usage.output_tokens,
                total_tokens: None,
            },
            stop_reason: match resp.stop_reason.as_str() {
                "end_turn" => StopReason::EndTurn,
                "max_tokens" => StopReason::MaxTokens,
                _ => StopReason::StopSequence,
            },
        })
    }
    
    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse> {
        if let Some(limiter) = &self.rate_limiter {
            if let Err(wait_time) = limiter.try_acquire(1) {
                tokio::time::sleep(wait_time).await;
                let _ = limiter.try_acquire(1);
            }
        }

        let body = AnthropicRequest {
            model: req.model.clone(),
            messages: req.messages.into_iter().map(into_anthropic_message).collect(),
            max_tokens: req.max_tokens.unwrap_or(4096),
            system: req.system_prompt,
            tools: Some(req.tools.into_iter().map(into_anthropic_tool).collect()),
        };

        let response = self.http_client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("anthropic-beta", "interleaved-thinking-2025-05-14")
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::OpenCodeError::Provider(format!("Network error: {}", e)))?;

        let (tx, rx) = mpsc::channel(1);
        let tx_clone = tx;

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut current_event = String::new();
            let mut current_data = String::new();
            let mut tool_call_buffer: Option<ToolCallBuffer> = None;

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        let text = String::from_utf8_lossy(&chunk);
                        for line in text.lines() {
                            let line = line.trim();
                            if line.is_empty() {
                                if !current_event.is_empty() || !current_data.is_empty() {
                                    if let Some(event) = parse_anthropic_sse_event(&current_event, &current_data, &mut tool_call_buffer) {
                                        if tx_clone.send(event).await.is_err() {
                                            return;
                                        }
                                    }
                                    current_event.clear();
                                    current_data.clear();
                                }
                            } else if line.starts_with("event:") {
                                current_event = line[6..].trim().to_string();
                            } else if line.starts_with("data:") {
                                current_data = line[5..].trim().to_string();
                            }
                        }
                    }
                    Err(_e) => {
                        let _ = tx_clone.send(StreamingEvent::Finish { 
                            stop_reason: StopReason::EndTurn, 
                            usage: TokenUsage { 
                                input_tokens: 0, 
                                output_tokens: 0, 
                                total_tokens: None 
                            }
                        }).await;
                        break;
                    }
                }
            }
        });

        Ok(StreamingResponse {
            events: Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)),
        })
    }
    
    fn model_info(&self, model_id: &str) -> Option<ModelInfo> {
        let info = match model_id {
            "claude-opus-4-5" => ModelInfo {
                id: "claude-opus-4-5".into(),
                name: "Claude Opus 4.5".into(),
                provider: "anthropic".into(),
                context_window: 200000,
                max_output_tokens: Some(8192),
            },
            "claude-sonnet-4-5" => ModelInfo {
                id: "claude-sonnet-4-5".into(),
                name: "Claude Sonnet 4.5".into(),
                provider: "anthropic".into(),
                context_window: 200000,
                max_output_tokens: Some(8192),
            },
            "claude-haiku-3.5" => ModelInfo {
                id: "claude-haiku-3.5".into(),
                name: "Claude Haiku 3.5".into(),
                provider: "anthropic".into(),
                context_window: 200000,
                max_output_tokens: Some(8192),
            },
            _ => return None,
        };
        Some(info)
    }
    
    fn provider_id(&self) -> &str {
        "anthropic"
    }

    fn abort(&self) {
        // TODO: Implement proper abort using CancelToken
    }
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    system: Option<String>,
    tools: Option<Vec<AnthropicTool>>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    usage: Usage,
    stop_reason: String,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    #[allow(dead_code)]
    ToolUse { id: String, name: String, input: serde_json::Value },
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}

fn into_anthropic_message(msg: rcode_core::Message) -> AnthropicMessage {
    let content = msg.parts.iter()
        .map(|p| match p {
            rcode_core::Part::Text { content } => content.clone(),
            rcode_core::Part::ToolResult { content, .. } => content.clone(),
            rcode_core::Part::ToolCall { name, arguments, .. } => 
                format!("Tool call: {}({})", name, arguments),
            rcode_core::Part::Reasoning { content } => format!("[Reasoning]: {}", content),
            rcode_core::Part::Attachment { name, mime_type, .. } => 
                format!("[Attachment: {} ({})]", name, mime_type),
        })
        .collect::<Vec<_>>()
        .join("\n");
    
    AnthropicMessage {
        role: match msg.role {
            rcode_core::Role::User => "user".into(),
            rcode_core::Role::Assistant => "assistant".into(),
            rcode_core::Role::System => "user".into(),
        },
        content,
    }
}

fn into_anthropic_tool(tool: ToolDefinition) -> AnthropicTool {
    AnthropicTool {
        name: tool.name,
        description: tool.description,
        input_schema: tool.parameters,
    }
}

struct ToolCallBuffer {
    id: String,
    name: String,
    arguments: serde_json::Value,
}

fn parse_anthropic_sse_event(
    event_type: &str,
    data: &str,
    tool_call_buffer: &mut Option<ToolCallBuffer>,
) -> Option<StreamingEvent> {
    match event_type {
        "message_start" => {
            Some(StreamingEvent::ContentBlock {
                content: Box::new(CoreContentBlock::Text { text: String::new() }),
            })
        }
        "content_block_start" => {
            let block: ContentBlockStart = serde_json::from_str(data).ok()?;
            match block.content {
                ContentBlockStartContent::ToolUse { id, name, .. } => {
                    *tool_call_buffer = Some(ToolCallBuffer {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: serde_json::Value::Object(serde_json::Map::new()),
                    });
                    Some(StreamingEvent::ToolCallStart { id, name })
                }
                _ => None,
            }
        }
        "content_block_delta" => {
            let delta: ContentBlockDelta = serde_json::from_str(data).ok()?;
            match delta.delta {
                DeltaContent::TextDelta { text } => {
                    Some(StreamingEvent::Text { delta: text })
                }
                DeltaContent::InputJsonDelta { partial_json } => {
                    if let Some(ref mut buffer) = *tool_call_buffer {
                        // Accumulate the arguments
                        if let serde_json::Value::Object(ref mut args) = buffer.arguments {
                            // Simple approach: try to merge the partial JSON
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&partial_json) {
                                merge_json_values(args, parsed);
                            }
                        }
                        Some(StreamingEvent::ToolCallArg {
                            id: buffer.id.clone(),
                            name: buffer.name.clone(),
                            value: partial_json,
                        })
                    } else {
                        None
                    }
                }
            }
        }
        "content_block_end" => {
            if let Some(buffer) = tool_call_buffer.take() {
                Some(StreamingEvent::ToolCallEnd { id: buffer.id })
            } else {
                None
            }
        }
        "message_delta" => {
            let delta: MessageDelta = serde_json::from_str(data).ok()?;
            Some(StreamingEvent::Finish { 
                stop_reason: match delta.stop_reason.as_str() {
                    "end_turn" => StopReason::EndTurn,
                    "max_tokens" => StopReason::MaxTokens,
                    _ => StopReason::StopSequence,
                },
                usage: TokenUsage { 
                    input_tokens: 0, 
                    output_tokens: delta.usage.output_tokens, 
                    total_tokens: None 
                }
            })
        }
        _ => None,
    }
}

fn merge_json_values(target: &mut serde_json::Map<String, serde_json::Value>, source: serde_json::Value) {
    if let serde_json::Value::Object(src) = source {
        for (k, v) in src {
            if let Some(existing) = target.get_mut(&k) {
                if let (serde_json::Value::Object(existing_obj), serde_json::Value::Object(new_obj)) = (existing, &v) {
                    merge_json_values(existing_obj, serde_json::Value::Object(new_obj.clone()));
                    continue;
                }
            }
            target.insert(k, v);
        }
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct MessageStart {
    message: AnthropicMessageContent,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct AnthropicMessageContent {
    id: String,
    #[serde(rename = "type")]
    msg_type: String,
    role: String,
    content: Vec<AnthropicContentBlock>,
    #[serde(rename = "stop_reason")]
    stop_reason: Option<String>,
    #[serde(rename = "usage")]
    usage: Usage,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct ContentBlockStart {
    index: u32,
    content: ContentBlockStartContent,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(untagged)]
enum ContentBlockStartContent {
    Text { #[serde(rename = "type")] t: String },
    ToolUse { id: String, name: String, #[serde(rename = "type")] t: String },
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct ContentBlockDelta {
    index: u32,
    delta: DeltaContent,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DeltaContent {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct MessageDelta {
    delta: DeltaUsage,
    #[serde(rename = "stop_reason")]
    stop_reason: String,
    usage: DeltaUsage,
}

#[derive(Deserialize)]
struct DeltaUsage {
    #[serde(rename = "output_tokens")]
    output_tokens: u32,
}
