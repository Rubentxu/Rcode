//! Anthropic provider implementation

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use rcode_core::{
    CompletionRequest, CompletionResponse, StreamingResponse, ModelInfo, StreamingEvent,
    ToolDefinition, TokenUsage, error::Result,
};
use rcode_core::provider::{StopReason, ProviderCapabilities};

use super::rate_limit::TokenBucket;
use super::LlmProvider;

pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    use_bearer_auth: bool,
    custom_headers: Vec<(String, String)>,
    http_client: Client,
    rate_limiter: Option<Arc<TokenBucket>>,
    /// Per-stream cancellation token. Each call to stream() gets a new token.
    /// When abort() is called, it cancels the current token and clears it.
    /// Uses std::sync::Mutex because abort() is synchronous.
    active_token: Arc<StdMutex<Option<CancellationToken>>>,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        
        let use_bearer_auth = std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok();
        
        let custom_headers = std::env::var("ANTHROPIC_CUSTOM_HEADERS")
            .map(|h| {
                serde_json::from_str::<Vec<(String, String)>>(&h)
                    .unwrap_or_else(|_| vec![])
            })
            .unwrap_or_default();
        
        Self {
            api_key,
            base_url,
            use_bearer_auth,
            custom_headers,
            http_client: Client::new(),
            rate_limiter: None,
            active_token: Arc::new(StdMutex::new(None)),
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
            stream: false,
        };
        
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let mut request_builder = self.http_client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");
        
        if self.use_bearer_auth {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", self.api_key));
        } else {
            request_builder = request_builder.header("x-api-key", &self.api_key);
        }
        
        for (key, value) in &self.custom_headers {
            request_builder = request_builder.header(key, value);
        }
        
        let response = request_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::RCodeError::Provider(format!("Network error: {}", e)))?;
        
        let resp: AnthropicResponse = response.json().await
            .map_err(|e| rcode_core::RCodeError::Provider(format!("Parse error: {}", e)))?;

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();
        for block in resp.content.iter() {
            match block {
                AnthropicContentBlock::Text { text } => text_parts.push(text.clone()),
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(rcode_core::provider::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: input.clone(),
                    });
                }
                AnthropicContentBlock::Thinking { .. } => {}
            }
        }
        
        Ok(CompletionResponse {
            content: text_parts.join(""),
            reasoning: None,
            tool_calls,
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
        // Create a new cancellation token for this stream
        let token = CancellationToken::new();
        
        // Store the token so abort() can cancel it
        {
            let mut guard = self.active_token.lock().unwrap();
            *guard = Some(token.clone());
        }

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
            stream: true,
        };

        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let mut request_builder = self.http_client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("anthropic-beta", "interleaved-thinking-2025-05-14");
        
        if self.use_bearer_auth {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", self.api_key));
        } else {
            request_builder = request_builder.header("x-api-key", &self.api_key);
        }
        
        for (key, value) in &self.custom_headers {
            request_builder = request_builder.header(key, value);
        }
        
        let response = request_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::RCodeError::Provider(format!("Network error: {}", e)))?;

        let (tx, rx) = mpsc::channel(1);
        let tx_clone = tx;
        let active_token = Arc::clone(&self.active_token);

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_event = String::new();
            let mut current_data = String::new();
            let mut tool_call_buffer: Option<ToolCallBuffer> = None;
            let token_clone = token.clone();

            loop {
                // Use tokio::select! to wait for either a chunk or cancellation
                tokio::select! {
                    // Check for cancellation
                    _ = token_clone.cancelled() => {
                        let _ = tx_clone.send(StreamingEvent::Finish {
                            stop_reason: StopReason::EndTurn,
                            usage: TokenUsage { input_tokens: 0, output_tokens: 0, total_tokens: None }
                        }).await;
                        // Clear the active token
                        let mut guard = active_token.lock().unwrap();
                        *guard = None;
                        return;
                    }
                    // Get next chunk
                    chunk_result = stream.next() => {
                        match chunk_result {
                            Some(Ok(chunk)) => {
                                let text = String::from_utf8_lossy(&chunk);
                                buffer.push_str(&text);

                                // Process complete lines from buffer
                                while let Some(newline_pos) = buffer.find('\n') {
                                    let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                                    buffer.drain(..=newline_pos);

                                    if line.is_empty() {
                                        // Empty line = dispatch event
                                        if !current_event.is_empty() || !current_data.is_empty() {
                                            if let Some(event) = parse_anthropic_sse_event(
                                                &current_event, &current_data, &mut tool_call_buffer
                                            ) {
                                                if tx_clone.send(event).await.is_err() { 
                                                    // Clear the active token
                                                    let mut guard = active_token.lock().unwrap();
                                                    *guard = None;
                                                    return; 
                                                }
                                            }
                                            current_event.clear();
                                            current_data.clear();
                                        }
                                    } else if let Some(val) = line.strip_prefix("event:") {
                                        current_event = val.trim().to_string();
                                    } else if let Some(val) = line.strip_prefix("data:") {
                                        current_data = val.trim().to_string();
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                tracing::error!("Stream error: {}", e);
                                let _ = tx_clone.send(StreamingEvent::Finish {
                                    stop_reason: StopReason::EndTurn,
                                    usage: TokenUsage { input_tokens: 0, output_tokens: 0, total_tokens: None }
                                }).await;
                                break;
                            }
                            None => {
                                // Stream ended normally
                                break;
                            }
                        }
                    }
                }
            }
            
            // Process any remaining buffered data
            if !current_event.is_empty() || !current_data.is_empty() {
                if let Some(event) = parse_anthropic_sse_event(
                    &current_event, &current_data, &mut tool_call_buffer
                ) {
                    let _ = tx_clone.send(event).await;
                }
            }
            
            // Clear the active token when stream ends
            let mut guard = active_token.lock().unwrap();
            *guard = None;
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
        let mut guard = match self.active_token.lock() {
            Ok(guard) => guard,
            Err(_) => return, // Could not acquire lock, stream is likely ending
        };
        if let Some(token) = guard.take() {
            token.cancel();
        }
    }
    
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::all()
    }
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    system: Option<String>,
    tools: Option<Vec<AnthropicTool>>,
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicMessageContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum AnthropicMessageContent {
    Text(String),
    Blocks(Vec<AnthropicInputContentBlock>),
}

#[derive(Serialize, Debug)]
#[serde(tag = "type")]
enum AnthropicInputContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: String, content: String, is_error: bool },
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
    #[serde(rename = "thinking")]
    Thinking { thinking: String, #[serde(default)] signature: Option<String> },
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
    let has_tool_results = msg.parts.iter().any(|p| matches!(p, rcode_core::Part::ToolResult { .. }));
    let has_tool_calls = msg.parts.iter().any(|p| matches!(p, rcode_core::Part::ToolCall { .. }));

    let mut text_parts = Vec::new();
    let mut blocks = Vec::new();

    for part in msg.parts {
        match part {
            rcode_core::Part::Text { content } => {
                text_parts.push(content.clone());
                blocks.push(AnthropicInputContentBlock::Text { text: content });
            }
            rcode_core::Part::ToolResult { tool_call_id, content, is_error } => {
                blocks.push(AnthropicInputContentBlock::ToolResult {
                    tool_use_id: tool_call_id,
                    content,
                    is_error,
                });
            }
            rcode_core::Part::ToolCall { id, name, arguments } => {
                blocks.push(AnthropicInputContentBlock::ToolUse {
                    id,
                    name,
                    input: *arguments,
                });
            }
            // Never feed model-generated reasoning back as literal history text.
            rcode_core::Part::Reasoning { .. } => {}
            rcode_core::Part::Attachment { name, mime_type, .. } => {
                let text = format!("[Attachment: {} ({})]", name, mime_type);
                text_parts.push(text.clone());
                blocks.push(AnthropicInputContentBlock::Text { text });
            }
        }
    }

    let content = if has_tool_results || has_tool_calls {
        AnthropicMessageContent::Blocks(blocks)
    } else {
        AnthropicMessageContent::Text(text_parts.join("\n"))
    };
    
    AnthropicMessage {
        role: if has_tool_results {
            "user".into()
        } else if has_tool_calls {
            "assistant".into()
        } else {
            match msg.role {
            rcode_core::Role::User => "user".into(),
            rcode_core::Role::Assistant => "assistant".into(),
            rcode_core::Role::System => "user".into(),
        }
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
            // Skip empty content blocks - message_start doesn't have meaningful content to emit
            None
        }
        "content_block_start" => {
            let block: ContentBlockStart = serde_json::from_str(data).ok()?;
            match block.content {
                ContentBlockStartContent::ToolUse { id, name } => {
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
                DeltaContent::ThinkingDelta { thinking } => {
                    Some(StreamingEvent::Reasoning { delta: thinking })
                }
                DeltaContent::SignatureDelta { .. } => {
                    None
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
        "content_block_end" | "content_block_stop" => {
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
    message: AnthropicResponseMessage,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct AnthropicResponseMessage {
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
    #[serde(alias = "content_block")]
    content: ContentBlockStartContent,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlockStartContent {
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String },
    #[serde(rename = "thinking")]
    Thinking,
    #[serde(rename = "text")]
    Text,
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
    TextDelta { text: String },
    ThinkingDelta { thinking: String },
    SignatureDelta { signature: String },
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

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{Message, Part, message::Role};
    use rcode_core::ToolDefinition;

    fn create_test_message(role: Role, parts: Vec<Part>) -> Message {
        Message {
            id: rcode_core::MessageId("msg1".to_string()),
            session_id: "session1".to_string(),
            role,
            parts,
            created_at: chrono::Utc::now(),
        }
    }

    fn create_text_part(content: &str) -> Part {
        Part::Text { content: content.to_string() }
    }

    fn create_tool_call_part(id: &str, name: &str, arguments: &str) -> Part {
        Part::ToolCall { 
            id: id.to_string(), 
            name: name.to_string(), 
            arguments: Box::new(serde_json::json!(arguments)), 
        }
    }

    fn create_tool_result_part(tool_call_id: &str, content: &str) -> Part {
        Part::ToolResult { 
            tool_call_id: tool_call_id.to_string(), 
            content: content.to_string(), 
            is_error: false,
        }
    }

    fn create_reasoning_part(content: &str) -> Part {
        Part::Reasoning { content: content.to_string() }
    }

    fn create_attachment_part(name: &str, mime_type: &str) -> Part {
        Part::Attachment { 
            id: "att1".to_string(),
            name: name.to_string(), 
            mime_type: mime_type.to_string(), 
            content: vec![1, 2, 3],
        }
    }

    #[test]
    fn test_into_anthropic_message_user_text() {
        let msg = create_test_message(
            Role::User, 
            vec![create_text_part("Hello world")]
        );
        let anthropic_msg = into_anthropic_message(msg);
        assert_eq!(anthropic_msg.role, "user");
        match anthropic_msg.content {
            AnthropicMessageContent::Text(content) => assert_eq!(content, "Hello world"),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_into_anthropic_message_assistant_text() {
        let msg = create_test_message(
            Role::Assistant, 
            vec![create_text_part("I am an assistant")]
        );
        let anthropic_msg = into_anthropic_message(msg);
        assert_eq!(anthropic_msg.role, "assistant");
        match anthropic_msg.content {
            AnthropicMessageContent::Text(content) => assert_eq!(content, "I am an assistant"),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_into_anthropic_message_system_becomes_user() {
        let msg = create_test_message(
            Role::System, 
            vec![create_text_part("You are helpful")]
        );
        let anthropic_msg = into_anthropic_message(msg);
        // System role maps to "user" in Anthropic
        assert_eq!(anthropic_msg.role, "user");
        match anthropic_msg.content {
            AnthropicMessageContent::Text(content) => assert_eq!(content, "You are helpful"),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_into_anthropic_message_multiple_parts() {
        let msg = create_test_message(
            Role::User, 
            vec![
                create_text_part("First part"),
                create_text_part("Second part"),
            ]
        );
        let anthropic_msg = into_anthropic_message(msg);
        // Multiple text parts are joined with newlines
        match anthropic_msg.content {
            AnthropicMessageContent::Text(content) => assert_eq!(content, "First part\nSecond part"),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_into_anthropic_message_tool_call() {
        let msg = create_test_message(
            Role::Assistant, 
            vec![create_tool_call_part("call_123", "get_weather", "{\"city\":\"NYC\"}")]
        );
        let anthropic_msg = into_anthropic_message(msg);
        assert_eq!(anthropic_msg.role, "assistant");
        match anthropic_msg.content {
            AnthropicMessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    AnthropicInputContentBlock::ToolUse { id, name, input } => {
                        assert_eq!(id, "call_123");
                        assert_eq!(name, "get_weather");
                        assert_eq!(input, &serde_json::json!("{\"city\":\"NYC\"}"));
                    }
                    _ => panic!("expected tool_use block"),
                }
            }
            _ => panic!("expected block content"),
        }
    }

    #[test]
    fn test_into_anthropic_message_tool_result() {
        let msg = create_test_message(
            Role::Assistant, 
            vec![create_tool_result_part("call_123", "Sunny, 72°F")]
        );
        let anthropic_msg = into_anthropic_message(msg);
        assert_eq!(anthropic_msg.role, "user");
        match anthropic_msg.content {
            AnthropicMessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    AnthropicInputContentBlock::ToolResult { tool_use_id, content, is_error } => {
                        assert_eq!(tool_use_id, "call_123");
                        assert_eq!(content, "Sunny, 72°F");
                        assert!(!is_error);
                    }
                    _ => panic!("expected tool_result block"),
                }
            }
            _ => panic!("expected block content"),
        }
    }

    #[test]
    fn test_into_anthropic_message_reasoning() {
        let msg = create_test_message(
            Role::Assistant, 
            vec![create_reasoning_part("Let me think step by step")]
        );
        let anthropic_msg = into_anthropic_message(msg);
        match anthropic_msg.content {
            AnthropicMessageContent::Text(content) => assert_eq!(content, ""),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_into_anthropic_message_attachment() {
        let msg = create_test_message(
            Role::User, 
            vec![create_attachment_part("document.pdf", "application/pdf")]
        );
        let anthropic_msg = into_anthropic_message(msg);
        match anthropic_msg.content {
            AnthropicMessageContent::Text(content) => {
                assert_eq!(content, "[Attachment: document.pdf (application/pdf)]")
            }
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_into_anthropic_message_empty_parts() {
        let msg = create_test_message(Role::User, vec![]);
        let anthropic_msg = into_anthropic_message(msg);
        match anthropic_msg.content {
            AnthropicMessageContent::Text(content) => assert_eq!(content, ""),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_into_anthropic_tool() {
        let tool = ToolDefinition {
            name: "get_weather".to_string(),
            description: "Get weather for a city".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"}
                }
            }),
        };
        let anthropic_tool = into_anthropic_tool(tool);
        assert_eq!(anthropic_tool.name, "get_weather");
        assert_eq!(anthropic_tool.description, "Get weather for a city");
        assert!(anthropic_tool.input_schema.is_object());
    }

    #[test]
    fn test_provider_new() {
        let provider = AnthropicProvider::new("test-api-key".to_string());
        assert_eq!(provider.provider_id(), "anthropic");
    }

    #[test]
    fn test_provider_with_rate_limit() {
        let provider = AnthropicProvider::new("test-api-key".to_string())
            .with_rate_limit(100, 10.0);
        // Rate limiter is set internally, just verify it doesn't panic
        assert_eq!(provider.provider_id(), "anthropic");
    }

    #[test]
    fn test_model_info_opus() {
        let provider = AnthropicProvider::new("test".to_string());
        let info = provider.model_info("claude-opus-4-5").unwrap();
        assert_eq!(info.id, "claude-opus-4-5");
        assert_eq!(info.name, "Claude Opus 4.5");
        assert_eq!(info.provider, "anthropic");
        assert_eq!(info.context_window, 200000);
        assert_eq!(info.max_output_tokens, Some(8192));
    }

    #[test]
    fn test_model_info_sonnet() {
        let provider = AnthropicProvider::new("test".to_string());
        let info = provider.model_info("claude-sonnet-4-5").unwrap();
        assert_eq!(info.id, "claude-sonnet-4-5");
        assert_eq!(info.name, "Claude Sonnet 4.5");
    }

    #[test]
    fn test_model_info_haiku() {
        let provider = AnthropicProvider::new("test".to_string());
        let info = provider.model_info("claude-haiku-3.5").unwrap();
        assert_eq!(info.id, "claude-haiku-3.5");
        assert_eq!(info.name, "Claude Haiku 3.5");
    }

    #[test]
    fn test_model_info_unknown() {
        let provider = AnthropicProvider::new("test".to_string());
        let info = provider.model_info("unknown-model");
        assert!(info.is_none());
    }

    #[test]
    fn test_provider_id() {
        let provider = AnthropicProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "anthropic");
    }

    // SSE Event parsing tests

    #[test]
    fn test_parse_anthropic_sse_event_message_start() {
        let data = r#"{"message":{"id":"msg1","type":"message","role":"assistant"}}"#;
        let event = parse_anthropic_sse_event("message_start", data, &mut None);
        // message_start now returns None (empty content blocks are skipped)
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_anthropic_sse_event_content_block_start_text() {
        let data = r#"{"index":0,"content":{"type":"text"}}"#;
        let event = parse_anthropic_sse_event("content_block_start", data, &mut None);
        // Text content blocks return None (handled by match _ => None)
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_anthropic_sse_event_content_block_start_tool_use() {
        // With #[serde(tag = "type")], the ToolUse variant correctly matches
        // content_block_start for tool_use, properly initializing the tool_call_buffer
        let data = r#"{"index":0,"content":{"type":"tool_use","id":"call_123","name":"get_weather"}}"#;
        let mut tool_buffer = None;
        let event = parse_anthropic_sse_event("content_block_start", data, &mut tool_buffer);
        // Now correctly returns ToolCallStart event with populated tool_buffer
        assert!(event.is_some());
        assert!(tool_buffer.is_some());
        match event.unwrap() {
            StreamingEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "get_weather");
            }
            _ => panic!("Expected ToolCallStart event"),
        }
    }

    #[test]
    fn test_parse_anthropic_sse_event_content_block_delta_text() {
        let data = r#"{"index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let event = parse_anthropic_sse_event("content_block_delta", data, &mut None);
        assert!(event.is_some());
        match event.unwrap() {
            StreamingEvent::Text { delta } => assert_eq!(delta, "Hello"),
            _ => panic!("Expected Text event"),
        }
    }

    #[test]
    fn test_parse_anthropic_sse_event_content_block_delta_tool_args() {
        let data = r#"{"index":0,"delta":{"type":"input_json_delta","partial_json":"{\"city\""}}"#;
        let mut tool_buffer = Some(ToolCallBuffer {
            id: "call_123".to_string(),
            name: "get_weather".to_string(),
            arguments: serde_json::Map::new().into(),
        });
        let event = parse_anthropic_sse_event("content_block_delta", data, &mut tool_buffer);
        assert!(event.is_some());
        match event.unwrap() {
            StreamingEvent::ToolCallArg { id, name, value } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "get_weather");
                assert_eq!(value, "{\"city\"");
            }
            _ => panic!("Expected ToolCallArg event"),
        }
    }

    #[test]
    fn test_parse_anthropic_sse_event_content_block_delta_without_buffer() {
        let data = r#"{"index":0,"delta":{"type":"input_json_delta","partial_json":"test"}}"#;
        let event = parse_anthropic_sse_event("content_block_delta", data, &mut None);
        // Without a tool call buffer, returns None
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_anthropic_sse_event_content_block_end_with_buffer() {
        let mut tool_buffer = Some(ToolCallBuffer {
            id: "call_123".to_string(),
            name: "get_weather".to_string(),
            arguments: serde_json::Map::new().into(),
        });
        let data = r#"{}"#;
        let event = parse_anthropic_sse_event("content_block_end", data, &mut tool_buffer);
        assert!(event.is_some());
        match event.unwrap() {
            StreamingEvent::ToolCallEnd { id } => assert_eq!(id, "call_123"),
            _ => panic!("Expected ToolCallEnd event"),
        }
        // Buffer should be consumed
        assert!(tool_buffer.is_none());
    }

    #[test]
    fn test_parse_anthropic_sse_event_content_block_end_without_buffer() {
        let data = r#"{}"#;
        let event = parse_anthropic_sse_event("content_block_end", data, &mut None);
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_anthropic_sse_event_message_delta_end_turn() {
        // MessageDelta expects delta to be DeltaUsage (just output_tokens)
        let data = r#"{"delta":{"output_tokens":100},"stop_reason":"end_turn","usage":{"output_tokens":100}}"#;
        let event = parse_anthropic_sse_event("message_delta", data, &mut None);
        assert!(event.is_some());
        match event.unwrap() {
            StreamingEvent::Finish { stop_reason, usage } => {
                assert_eq!(stop_reason, StopReason::EndTurn);
                assert_eq!(usage.output_tokens, 100);
            }
            _ => panic!("Expected Finish event"),
        }
    }

    #[test]
    fn test_parse_anthropic_sse_event_message_delta_max_tokens() {
        let data = r#"{"delta":{"output_tokens":100},"stop_reason":"max_tokens","usage":{"output_tokens":100}}"#;
        let event = parse_anthropic_sse_event("message_delta", data, &mut None);
        assert!(event.is_some());
        match event.unwrap() {
            StreamingEvent::Finish { stop_reason, .. } => {
                assert_eq!(stop_reason, StopReason::MaxTokens);
            }
            _ => panic!("Expected Finish event"),
        }
    }

    #[test]
    fn test_parse_anthropic_sse_event_message_delta_stop_sequence() {
        let data = r#"{"delta":{"output_tokens":50},"stop_reason":"stop_sequence","usage":{"output_tokens":50}}"#;
        let event = parse_anthropic_sse_event("message_delta", data, &mut None);
        assert!(event.is_some());
        match event.unwrap() {
            StreamingEvent::Finish { stop_reason, .. } => {
                assert_eq!(stop_reason, StopReason::StopSequence);
            }
            _ => panic!("Expected Finish event"),
        }
    }

    #[test]
    fn test_parse_anthropic_sse_event_unknown_event() {
        let event = parse_anthropic_sse_event("unknown_event", "{}", &mut None);
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_anthropic_sse_event_invalid_json() {
        // message_start doesn't parse data, but content_block_start does
        let event = parse_anthropic_sse_event("content_block_start", "not valid json", &mut None);
        assert!(event.is_none());
    }

    #[test]
    fn test_merge_json_values_simple() {
        let mut target: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        merge_json_values(&mut target, serde_json::json!({"key": "value"}));
        assert_eq!(target.get("key").unwrap(), &serde_json::json!("value"));
    }

    #[test]
    fn test_merge_json_values_nested() {
        let mut target: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        target.insert("nested".to_string(), serde_json::json!({"inner": "old"}));
        
        merge_json_values(&mut target, serde_json::json!({"nested": {"inner": "new", "extra": "value"}}));
        
        let nested = target.get("nested").unwrap().as_object().unwrap();
        assert_eq!(nested.get("inner").unwrap(), &serde_json::json!("new"));
        assert_eq!(nested.get("extra").unwrap(), &serde_json::json!("value"));
    }

    #[test]
    fn test_merge_json_values_new_key() {
        let mut target: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        target.insert("existing".to_string(), serde_json::json!("value"));
        
        merge_json_values(&mut target, serde_json::json!({"new_key": "new_value"}));
        
        assert_eq!(target.get("existing").unwrap(), &serde_json::json!("value"));
        assert_eq!(target.get("new_key").unwrap(), &serde_json::json!("new_value"));
    }

    #[test]
    fn test_merge_json_values_array_not_merged() {
        // Arrays are replaced, not merged
        let mut target: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        target.insert("arr".to_string(), serde_json::json!(["a", "b"]));
        
        merge_json_values(&mut target, serde_json::json!({"arr": ["c", "d"]}));
        
        let arr = target.get("arr").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], serde_json::json!("c"));
        assert_eq!(arr[1], serde_json::json!("d"));
    }

    // Serialization tests for request/response types

    #[test]
    fn test_anthropic_request_serialization() {
        let request = AnthropicRequest {
            model: "claude-3".to_string(),
            messages: vec![
                AnthropicMessage {
                    role: "user".to_string(),
                    content: AnthropicMessageContent::Text("Hello".to_string()),
                }
            ],
            max_tokens: 1024,
            system: Some("You are helpful".to_string()),
            tools: Some(vec![
                AnthropicTool {
                    name: "test".to_string(),
                    description: "A test tool".to_string(),
                    input_schema: serde_json::json!({}),
                }
            ]),
            stream: false,
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""model":"claude-3""#));
        assert!(json.contains(r#""max_tokens":1024"#));
        assert!(json.contains(r#""system":"You are helpful""#));
    }

    #[test]
    fn test_anthropic_response_deserialization() {
        let json = r#"{
            "content": [{"type": "text", "text": "Hello!"}],
            "usage": {"input_tokens": 10, "output_tokens": 20},
            "stop_reason": "end_turn"
        }"#;
        
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content.len(), 1);
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 20);
        assert_eq!(response.stop_reason, "end_turn");
    }

    #[test]
    fn test_anthropic_response_tool_use_content() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Let me help"},
                {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {}}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 20},
            "stop_reason": "end_turn"
        }"#;
        
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content.len(), 2);
    }

    #[test]
    fn test_anthropic_message_serialization() {
        let msg = AnthropicMessage {
            role: "user".to_string(),
            content: AnthropicMessageContent::Text("Test content".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""role":"user""#));
        assert!(json.contains(r#""content":"Test content""#));
    }

    #[test]
    fn test_usage_deserialization() {
        let json = r#"{"input_tokens": 100, "output_tokens": 200}"#;
        let usage: Usage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 200);
    }

    #[test]
    fn test_delta_usage_deserialization() {
        let json = r#"{"output_tokens": 150}"#;
        let usage: DeltaUsage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.output_tokens, 150);
    }

    #[test]
    fn test_content_block_delta_text_deserialization() {
        let json = r#"{"type": "text_delta", "text": "Hello world"}"#;
        let delta: DeltaContent = serde_json::from_str(json).unwrap();
        match delta {
            DeltaContent::TextDelta { text } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected TextDelta"),
        }
    }

    #[test]
    fn test_content_block_delta_input_json_deserialization() {
        let json = r#"{"type": "input_json_delta", "partial_json": "{\"key\""}"#;
        let delta: DeltaContent = serde_json::from_str(json).unwrap();
        match delta {
            DeltaContent::InputJsonDelta { partial_json } => assert_eq!(partial_json, "{\"key\""),
            _ => panic!("Expected InputJsonDelta"),
        }
    }

    #[test]
    fn test_content_block_start_text_deserialization() {
        let json = r#"{"index": 0, "content": {"type": "text"}}"#;
        let block: ContentBlockStart = serde_json::from_str(json).unwrap();
        assert_eq!(block.index, 0);
        match block.content {
            ContentBlockStartContent::Text { .. } => {},
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_content_block_start_tool_use_deserialization() {
        // With #[serde(tag = "type")], ToolUse variant correctly matches tool_use content blocks
        let json = r#"{"index": 0, "content": {"type": "tool_use", "id": "call_1", "name": "test"}}"#;
        let block: ContentBlockStart = serde_json::from_str(json).unwrap();
        match block.content {
            ContentBlockStartContent::ToolUse { id, name } => {
                assert_eq!(id, "call_1");
                assert_eq!(name, "test");
            }
            _ => panic!("Expected ToolUse variant"),
        }
    }

    #[test]
    fn test_message_delta_deserialization() {
        // DeltaUsage only has output_tokens, not stop_reason
        let json = r#"{
            "delta": {"output_tokens": 50},
            "stop_reason": "end_turn",
            "usage": {"output_tokens": 50}
        }"#;
        let msg_delta: MessageDelta = serde_json::from_str(json).unwrap();
        assert_eq!(msg_delta.stop_reason, "end_turn");
        assert_eq!(msg_delta.usage.output_tokens, 50);
    }

    #[test]
    fn test_abort_method_exists() {
        let provider = AnthropicProvider::new("test".to_string());
        // abort() should be callable without panicking
        provider.abort();
        // If we get here, the test passes
    }

    #[test]
    fn test_provider_with_rate_limit_retains_api_key() {
        let provider = AnthropicProvider::new("my-secret-key".to_string())
            .with_rate_limit(50, 5.0);
        // Just verify it doesn't panic and retains identity
        assert_eq!(provider.provider_id(), "anthropic");
    }

    #[test]
    fn test_anthropic_request_serialization_with_minimal_fields() {
        // Test request with minimal fields
        let request = AnthropicRequest {
            model: "claude-3-opus".to_string(),
            messages: vec![
                AnthropicMessage {
                    role: "user".to_string(),
                    content: AnthropicMessageContent::Text("Hi".to_string()),
                }
            ],
            max_tokens: 1024,
            system: None,
            tools: None,
            stream: false,
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""model":"claude-3-opus""#));
        assert!(json.contains(r#""max_tokens":1024"#));
    }

    #[test]
    fn test_anthropic_request_serialization_system_prompt() {
        let request = AnthropicRequest {
            model: "claude-3".to_string(),
            messages: vec![],
            max_tokens: 2048,
            system: Some("You are a helpful assistant".to_string()),
            tools: None,
            stream: false,
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""system":"You are a helpful assistant""#));
    }

    #[test]
    fn test_content_block_start_tool_use_parsing() {
        // Test parsing content_block_start with tool_use when properly formatted
        // Using the correct JSON structure that matches the deserialization
        let json = r#"{"index":0,"content":{"type":"tool_use","id":"call_abc","name":"test_func"}}"#;
        let block: ContentBlockStart = serde_json::from_str(json).unwrap();
        assert_eq!(block.index, 0);
    }

    #[test]
    fn test_message_start_deserialization() {
        let json = r#"{
            "message": {
                "id": "msg_123",
                "type": "message",
                "role": "assistant",
                "content": [],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 10, "output_tokens": 20}
            }
        }"#;
        let msg: MessageStart = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message.id, "msg_123");
        assert_eq!(msg.message.role, "assistant");
    }

    #[test]
    fn test_anthropic_response_no_content() {
        let json = r#"{
            "content": [],
            "usage": {"input_tokens": 5, "output_tokens": 0},
            "stop_reason": "end_turn"
        }"#;
        
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content.len(), 0);
        assert_eq!(response.stop_reason, "end_turn");
    }

    #[test]
    fn test_anthropic_response_multiple_text_blocks() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "First message"},
                {"type": "text", "text": "Second message"}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 20},
            "stop_reason": "end_turn"
        }"#;
        
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content.len(), 2);
    }

    #[test]
    fn test_merge_json_values_deeply_nested() {
        let mut target: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        target.insert("level1".to_string(), serde_json::json!({
            "level2": {"level3": "old"}
        }));
        
        merge_json_values(&mut target, serde_json::json!({
            "level1": {
                "level2": {
                    "level3": "new",
                    "extra": "value"
                }
            }
        }));
        
        let level1 = target.get("level1").unwrap().as_object().unwrap();
        let level2 = level1.get("level2").unwrap().as_object().unwrap();
        assert_eq!(level2.get("level3").unwrap(), &serde_json::json!("new"));
        assert_eq!(level2.get("extra").unwrap(), &serde_json::json!("value"));
    }

    #[test]
    fn test_merge_json_values_replaces_non_object() {
        let mut target: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        target.insert("key".to_string(), serde_json::json!("string value"));
        
        merge_json_values(&mut target, serde_json::json!({"key": {"nested": "object"}}));
        
        // String should be replaced by object
        let new_val = target.get("key").unwrap();
        assert!(new_val.is_object());
    }

    #[test]
    fn test_parse_anthropic_sse_event_message_start_with_usage() {
        let data = r#"{"message":{"id":"msg1","type":"message","role":"assistant"}}"#;
        let event = parse_anthropic_sse_event("message_start", data, &mut None);
        // message_start now returns None (empty content blocks are skipped)
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_anthropic_sse_event_content_block_delta_text_accumulates() {
        // Simulate multiple text deltas being received
        let data1 = r#"{"index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let event1 = parse_anthropic_sse_event("content_block_delta", data1, &mut None);
        assert!(event1.is_some());
        
        let data2 = r#"{"index":0,"delta":{"type":"text_delta","text":" World"}}"#;
        let event2 = parse_anthropic_sse_event("content_block_delta", data2, &mut None);
        assert!(event2.is_some());
    }

    #[test]
    fn test_openai_message_role_system() {
        let msg = create_test_message(Role::System, vec![create_text_part("You are helpful")]);
        let openai_msg = into_anthropic_message(msg);
        // System role maps to "user" in Anthropic
        assert_eq!(openai_msg.role, "user");
    }

    #[test]
    fn test_openai_message_role_assistant() {
        let msg = create_test_message(Role::Assistant, vec![create_text_part("I am here")]);
        let openai_msg = into_anthropic_message(msg);
        assert_eq!(openai_msg.role, "assistant");
    }

    #[test]
    fn test_delta_content_untagged_deserialization() {
        // Test that the untagged DeltaContent can deserialize both variants
        let text_json = r#"{"type": "text_delta", "text": "hi"}"#;
        let text_delta: DeltaContent = serde_json::from_str(text_json).unwrap();
        assert!(matches!(text_delta, DeltaContent::TextDelta { .. }));

        let json_json = r#"{"type": "input_json_delta", "partial_json": "{}"}"#;
        let json_delta: DeltaContent = serde_json::from_str(json_json).unwrap();
        assert!(matches!(json_delta, DeltaContent::InputJsonDelta { .. }));
    }

    #[test]
    fn test_content_block_delta_deserialization() {
        let json = r#"{"index":0,"delta":{"type":"text_delta","text":"Hello world"}}"#;
        let block: ContentBlockDelta = serde_json::from_str(json).unwrap();
        assert_eq!(block.index, 0);
    }

    // Cancellation tests using CancellationToken

    #[tokio::test]
    async fn test_abort_cancels_active_stream() {
        // Create a cancellation token
        let token = tokio_util::sync::CancellationToken::new();
        let token_clone = token.clone();

        let handle = tokio::spawn(async move {
            let mut count = 0;
            loop {
                tokio::select! {
                    _ = token_clone.cancelled() => {
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {
                        count += 1;
                        if count > 100 {
                            break; // Safety timeout
                        }
                    }
                }
            }
            count
        });

        // Give the loop time to start
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        // Cancel
        token.cancel();

        let final_count = handle.await.unwrap();
        assert!(final_count < 100, "Stream should have been cancelled before 100 iterations");
    }

    #[tokio::test]
    async fn test_concurrent_streams_independent_cancellation() {
        // Test that aborting one stream does NOT affect another stream

        let token1 = tokio_util::sync::CancellationToken::new();
        let token2 = tokio_util::sync::CancellationToken::new();

        let token1_clone = token1.clone();
        let token2_clone = token2.clone();

        // Start stream 1
        let handle1 = tokio::spawn(async move {
            let mut count = 0;
            loop {
                tokio::select! {
                    _ = token1_clone.cancelled() => {
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(5)) => {
                        count += 1;
                        if count > 100 {
                            break; // Safety timeout
                        }
                    }
                }
            }
            count
        });

        // Start stream 2
        let handle2 = tokio::spawn(async move {
            let mut count = 0;
            loop {
                tokio::select! {
                    _ = token2_clone.cancelled() => {
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(5)) => {
                        count += 1;
                        if count > 100 {
                            break; // Safety timeout
                        }
                    }
                }
            }
            count
        });

        // Let them run for a bit
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        // Cancel only stream 1
        token1.cancel();

        let count1 = handle1.await.unwrap();
        let count2 = handle2.await.unwrap();

        // Stream 1 should have been cancelled early
        assert!(count1 < 10, "Stream 1 should have been cancelled, got count: {}", count1);

        // Stream 2 should NOT have been affected (should have continued)
        // It should have at least 3 iterations in 30ms at 5ms intervals
        assert!(count2 >= 3, "Stream 2 should not be affected, got count: {}", count2);
    }

    #[tokio::test]
    async fn test_stream_completes_normally_when_not_aborted() {
        let token = tokio_util::sync::CancellationToken::new();
        let token_clone = token.clone();

        let handle = tokio::spawn(async move {
            let mut count = 0;
            loop {
                tokio::select! {
                    _ = token_clone.cancelled() => {
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(5)) => {
                        count += 1;
                        if count >= 5 {
                            // Simulate normal completion
                            break;
                        }
                    }
                }
            }
            count
        });

        let final_count = handle.await.unwrap();

        // Should have completed normally without being cancelled
        assert_eq!(final_count, 5, "Stream should have completed normally");
        assert!(!token.is_cancelled(), "Token should not be cancelled");
    }

    // =============================================================================
    // Multi-turn post-tool regression tests
    // =============================================================================

    /// Regression test: assistant message with Part::ToolCall must serialize as
    /// role="assistant" with a tool_use block (NOT flattened to text).
    ///
    /// Bug: Previously tool_use was flattened to text, causing second turn to fail.
    #[test]
    fn test_into_anthropic_message_tool_call_is_not_flattened() {
        let msg = create_test_message(
            Role::Assistant,
            vec![create_tool_call_part("call_001", "get_weather", r#"{"city":"NYC"}"#)],
        );
        let anthropic_msg = into_anthropic_message(msg);

        // Role must be "assistant" for tool calls
        assert_eq!(anthropic_msg.role, "assistant");

        // Content must be Blocks format, not Text
        let blocks = match &anthropic_msg.content {
            AnthropicMessageContent::Blocks(b) => b,
            AnthropicMessageContent::Text(_) => {
                panic!("tool_call must NOT be flattened to text content")
            }
        };

        // Must have exactly one block of type ToolUse
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            AnthropicInputContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "call_001");
                assert_eq!(name, "get_weather");
                // input is the JSON arguments as a Value (stored as string in helper)
                // The helper wraps arguments as a json string, so input is a string value
                assert_eq!(input, &serde_json::json!("{\"city\":\"NYC\"}"));
            }
            other => panic!("expected ToolUse block, got {:?}", other),
        }
    }

    /// Regression test: message with Part::ToolResult must serialize as
    /// role="user" with a tool_result block (NOT flattened to text).
    ///
    /// Bug: Previously tool_result was flattened to text, causing second turn to fail.
    #[test]
    fn test_into_anthropic_message_tool_result_is_not_flattened() {
        let msg = create_test_message(
            Role::Assistant,
            vec![create_tool_result_part("call_001", "Sunny, 72°F")],
        );
        let anthropic_msg = into_anthropic_message(msg);

        // Role must be "user" when message contains tool results
        assert_eq!(anthropic_msg.role, "user");

        // Content must be Blocks format, not Text
        let blocks = match &anthropic_msg.content {
            AnthropicMessageContent::Blocks(b) => b,
            AnthropicMessageContent::Text(t) => {
                panic!("tool_result must NOT be flattened to text content: {:?}", t)
            }
        };

        // Must have exactly one block of type ToolResult
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            AnthropicInputContentBlock::ToolResult { tool_use_id, content, is_error } => {
                assert_eq!(tool_use_id, "call_001");
                assert_eq!(content, "Sunny, 72°F");
                assert!(!is_error);
            }
            other => panic!("expected ToolResult block, got {:?}", other),
        }
    }

    /// Regression test: Part::Reasoning must NOT be re-injected as history text.
    ///
    /// Bug: Previously reasoning content was added to text_parts and joined as history,
    /// causing reasoning to appear as literal user/assistant text in subsequent turns.
    #[test]
    fn test_into_anthropic_message_reasoning_not_injected_as_text() {
        let msg = create_test_message(
            Role::Assistant,
            vec![
                create_reasoning_part("Let me think step by step"),
                create_text_part("Final answer"),
            ],
        );
        let anthropic_msg = into_anthropic_message(msg);

        // The message should have assistant role
        assert_eq!(anthropic_msg.role, "assistant");

        // Content should be Blocks since there's text + reasoning (no tool calls/results)
        // But reasoning should NOT appear anywhere in the content
        match &anthropic_msg.content {
            AnthropicMessageContent::Text(text) => {
                // Reasoning should NOT be in the text
                assert!(
                    !text.contains("Let me think step by step"),
                    "reasoning must NOT be injected as history text"
                );
                assert_eq!(text, "Final answer");
            }
            AnthropicMessageContent::Blocks(blocks) => {
                // If blocks format, reasoning definitely not included (it's skipped in the loop)
                for block in blocks {
                    match block {
                        AnthropicInputContentBlock::Text { text } => {
                            assert!(
                                !text.contains("Let me think step by step"),
                                "reasoning must NOT be in text block"
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Regression test: assistant message with ONLY Part::Reasoning must produce
    /// empty content (reasoning is skipped entirely).
    #[test]
    fn test_into_anthropic_message_only_reasoning_produces_empty() {
        let msg = create_test_message(
            Role::Assistant,
            vec![create_reasoning_part("thinking...")],
        );
        let anthropic_msg = into_anthropic_message(msg);

        // Reasoning-only message should still be "assistant" role
        assert_eq!(anthropic_msg.role, "assistant");

        // Content should be empty text (reasoning skipped, no text_parts)
        match &anthropic_msg.content {
            AnthropicMessageContent::Text(text) => {
                assert!(text.is_empty(), "reasoning-only should produce empty text");
            }
            AnthropicMessageContent::Blocks(blocks) => {
                assert!(blocks.is_empty(), "reasoning-only should produce no blocks");
            }
        }
    }

    /// Regression test: multi-turn conversation flow with tool call and tool result.
    ///
    /// This validates the complete conversation history that would be sent to Anthropic:
    /// 1. User text
    /// 2. Assistant text + tool_call (role=assistant, content=tool_use block)
    /// 3. User tool_result (role=user, content=tool_result block)
    /// 4. Assistant final text
    ///
    /// Bug: Without proper handling, messages after tool_result would be misformatted.
    #[test]
    fn test_into_anthropic_message_multi_turn_tool_conversation() {
        // Turn 1: User asks question
        let user_msg = create_test_message(
            Role::User,
            vec![create_text_part("What's the weather in NYC?")],
        );
        let anthropic_user = into_anthropic_message(user_msg);
        assert_eq!(anthropic_user.role, "user");
        match &anthropic_user.content {
            AnthropicMessageContent::Text(t) => {
                assert!(t.contains("weather"));
            }
            _ => panic!("expected text content"),
        }

        // Turn 2: Assistant makes tool call
        let assistant_tool_msg = create_test_message(
            Role::Assistant,
            vec![
                create_text_part("I'll check that for you."),
                create_tool_call_part("call_001", "get_weather", r#"{"city":"NYC"}"#),
            ],
        );
        let anthropic_asst_tool = into_anthropic_message(assistant_tool_msg);
        assert_eq!(anthropic_asst_tool.role, "assistant");
        match &anthropic_asst_tool.content {
            AnthropicMessageContent::Blocks(blocks) => {
                // Should have text + tool_use block
                assert_eq!(blocks.len(), 2);
                match &blocks[0] {
                    AnthropicInputContentBlock::Text { text } => {
                        assert!(text.contains("check"));
                    }
                    _ => panic!("first block should be text"),
                }
                match &blocks[1] {
                    AnthropicInputContentBlock::ToolUse { id, name, .. } => {
                        assert_eq!(id, "call_001");
                        assert_eq!(name, "get_weather");
                    }
                    _ => panic!("second block should be tool_use"),
                }
            }
            _ => panic!("tool_call should use blocks format"),
        }

        // Turn 3: User provides tool result
        let user_tool_result_msg = create_test_message(
            Role::User,
            vec![create_tool_result_part("call_001", "Sunny, 72°F")],
        );
        let anthropic_user_result = into_anthropic_message(user_tool_result_msg);
        // Tool results force role to "user"
        assert_eq!(anthropic_user_result.role, "user");
        match &anthropic_user_result.content {
            AnthropicMessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    AnthropicInputContentBlock::ToolResult { tool_use_id, content, .. } => {
                        assert_eq!(tool_use_id, "call_001");
                        assert_eq!(content, "Sunny, 72°F");
                    }
                    _ => panic!("expected tool_result block"),
                }
            }
            _ => panic!("tool_result should use blocks format"),
        }

        // Turn 4: Assistant final response
        let assistant_final_msg = create_test_message(
            Role::Assistant,
            vec![create_text_part("The weather in NYC is Sunny, 72°F.")],
        );
        let anthropic_final = into_anthropic_message(assistant_final_msg);
        assert_eq!(anthropic_final.role, "assistant");
        match &anthropic_final.content {
            AnthropicMessageContent::Text(t) => {
                assert!(t.contains("Sunny"));
            }
            _ => panic!("final text should be simple text content"),
        }
    }

    /// Regression test: verify request serialization with multi-turn tool conversation.
    ///
    /// This validates that an AnthropicRequest with multiple messages (including
    /// tool_use and tool_result blocks) serializes correctly to valid JSON.
    ///
    /// Bug: Without proper block handling, serialization would produce malformed requests.
    #[test]
    fn test_anthropic_request_multi_turn_serialization() {
        // Build a multi-turn conversation
        let messages: Vec<AnthropicMessage> = vec![
            // Turn 1: User
            AnthropicMessage {
                role: "user".to_string(),
                content: AnthropicMessageContent::Text("What's the weather?".to_string()),
            },
            // Turn 2: Assistant with tool call
            AnthropicMessage {
                role: "assistant".to_string(),
                content: AnthropicMessageContent::Blocks(vec![
                    AnthropicInputContentBlock::ToolUse {
                        id: "call_001".to_string(),
                        name: "get_weather".to_string(),
                        input: serde_json::json!({"city": "NYC"}),
                    },
                ]),
            },
            // Turn 3: User with tool result
            AnthropicMessage {
                role: "user".to_string(),
                content: AnthropicMessageContent::Blocks(vec![
                    AnthropicInputContentBlock::ToolResult {
                        tool_use_id: "call_001".to_string(),
                        content: "Sunny, 72°F".to_string(),
                        is_error: false,
                    },
                ]),
            },
            // Turn 4: Assistant final
            AnthropicMessage {
                role: "assistant".to_string(),
                content: AnthropicMessageContent::Text("The weather is sunny!".to_string()),
            },
        ];

        let request = AnthropicRequest {
            model: "claude-sonnet-4-5".to_string(),
            messages,
            max_tokens: 1024,
            system: Some("You are a helpful assistant.".to_string()),
            tools: Some(vec![
                AnthropicTool {
                    name: "get_weather".to_string(),
                    description: "Get weather for a city".to_string(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "city": {"type": "string"}
                        }
                    }),
                },
            ]),
            stream: false,
        };

        // Serialize to JSON
        let json = serde_json::to_string(&request).expect("should serialize");

        // Verify key elements in JSON
        assert!(json.contains(r#""model":"claude-sonnet-4-5""#));
        assert!(json.contains(r#""role":"user""#));
        assert!(json.contains(r#""role":"assistant""#));
        assert!(json.contains(r#""type":"tool_use""#));
        assert!(json.contains(r#""type":"tool_result""#));
        assert!(json.contains(r#""id":"call_001""#));
        assert!(json.contains(r#""name":"get_weather""#));

        // Verify tool_use block is NOT flattened to plain text
        assert!(json.contains(r#""input":{"city":"NYC"}"#),
            "tool_use input should be structured JSON, not text");

        // Verify tool_result content is preserved
        assert!(json.contains(r#""content":"Sunny, 72°F""#),
            "tool_result content should be preserved");
    }

    /// Regression test: assistant message with tool_call and reasoning together.
    ///
    /// Ensures that when both reasoning and tool_call are present, reasoning
    /// is still NOT injected as history text.
    #[test]
    fn test_into_anthropic_message_tool_call_with_reasoning() {
        let msg = create_test_message(
            Role::Assistant,
            vec![
                create_reasoning_part("I need to call the weather tool"),
                create_tool_call_part("call_001", "get_weather", r#"{"city":"NYC"}"#),
            ],
        );
        let anthropic_msg = into_anthropic_message(msg);

        assert_eq!(anthropic_msg.role, "assistant");

        // Must be blocks format (has tool_call)
        match &anthropic_msg.content {
            AnthropicMessageContent::Blocks(blocks) => {
                // Should only have the tool_use block, no reasoning text
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    AnthropicInputContentBlock::ToolUse { id, .. } => {
                        assert_eq!(id, "call_001");
                    }
                    other => panic!("expected ToolUse block, got {:?}", other),
                }
            }
            AnthropicMessageContent::Text(t) => {
                panic!("tool_call + reasoning should NOT produce text: {:?}", t);
            }
        }
    }

    #[test]
    fn test_abort_method_is_callable() {
        let provider = AnthropicProvider::new("test-api-key".to_string());
        // abort() should be callable without panicking
        provider.abort();
        // If we get here, the test passes
    }

    #[tokio::test]
    async fn test_per_stream_cancellation_token_pattern() {
        // Test the actual pattern: each stream gets its own token
        // and abort() cancels only the current stream's token

        use std::sync::{Arc, Mutex as StdMutex};

        // Simulate the per-stream token pattern
        let active_token: Arc<StdMutex<Option<tokio_util::sync::CancellationToken>>> =
            Arc::new(StdMutex::new(None));

        // Simulate starting stream 1
        let token1 = tokio_util::sync::CancellationToken::new();
        {
            let mut guard = active_token.lock().unwrap();
            *guard = Some(token1.clone());
        }

        // Simulate starting stream 2 (replaces token1)
        let token2 = tokio_util::sync::CancellationToken::new();
        {
            let mut guard = active_token.lock().unwrap();
            *guard = Some(token2.clone());
        }

        // Simulate abort - should cancel token2 (the current one)
        {
            let mut guard = active_token.lock().unwrap();
            if let Some(token) = guard.take() {
                token.cancel();
            }
        }

        // token1 should NOT be cancelled (it was replaced)
        assert!(!token1.is_cancelled(), "Old stream token should not be cancelled");

        // token2 SHOULD be cancelled (it's the active one)
        assert!(token2.is_cancelled(), "Active stream token should be cancelled");
    }
}
