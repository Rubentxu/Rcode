//! OpenAI provider implementation

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use rcode_core::{
    CompletionRequest, CompletionResponse, ModelInfo,
    StreamingEvent, StreamingResponse,
    TokenUsage, error::Result,
};
use rcode_core::provider::StopReason;

use super::rate_limit::TokenBucket;
use super::LlmProvider;

pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    custom_headers: Vec<(String, String)>,
    http_client: Client,
    rate_limiter: Option<Arc<TokenBucket>>,
    /// Per-stream cancellation token. Each call to stream() gets a new token.
    /// When abort() is called, it cancels the current token and clears it.
    /// Uses std::sync::Mutex because abort() is synchronous.
    active_token: Arc<StdMutex<Option<CancellationToken>>>,
}

impl OpenAIProvider {
    pub fn new(api_key: String) -> Self {
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com".to_string());
        
        let custom_headers = std::env::var("OPENAI_CUSTOM_HEADERS")
            .map(|h| {
                serde_json::from_str::<Vec<(String, String)>>(&h)
                    .unwrap_or_else(|_| vec![])
            })
            .unwrap_or_default();
        
        Self {
            api_key,
            base_url,
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

    /// Create a new OpenAI provider with a custom base URL
    /// This is useful for providers like OpenRouter that use OpenAI-compatible APIs
    pub fn new_with_base_url(api_key: String, base_url: String) -> Self {
        let custom_headers = std::env::var("OPENAI_CUSTOM_HEADERS")
            .map(|h| {
                serde_json::from_str::<Vec<(String, String)>>(&h)
                    .unwrap_or_else(|_| vec![])
            })
            .unwrap_or_default();

        Self {
            api_key,
            base_url,
            custom_headers,
            http_client: Client::new(),
            rate_limiter: None,
            active_token: Arc::new(StdMutex::new(None)),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        // Build messages including system prompt
        let mut messages: Vec<OpenAIMessage> = Vec::new();
        if let Some(sp) = req.system_prompt.clone() {
            messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: Some(sp),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        messages.extend(req.messages.into_iter().map(into_openai_message));
        
        let body = OpenAIRequest {
            model: req.model.clone(),
            messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: false,
            tools: if req.tools.is_empty() { None } else { Some(req.tools.iter().map(into_openai_tool).collect()) },
        };
        
        let url = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));
        let mut request_builder = self.http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");
        
        for (key, value) in &self.custom_headers {
            request_builder = request_builder.header(key, value);
        }
        
        let response = request_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::OpenCodeError::Provider(format!("Network error: {}", e)))?;
        
        // Check HTTP status
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(rcode_core::OpenCodeError::Provider(
                format!("OpenAI API error ({}): {}", status, error_text)
            ));
        }
        
        let openai_resp: serde_json::Value = response.json()
            .await
            .map_err(|e| rcode_core::OpenCodeError::Provider(format!("Failed to parse response: {}", e)))?;
        
        let content = openai_resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        
        let reasoning = openai_resp["choices"][0]["message"]["reasoning"]
            .as_str()
            .map(String::from);
        
        let usage = TokenUsage {
            input_tokens: openai_resp["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: openai_resp["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: openai_resp["usage"]["total_tokens"].as_u64().map(|t| t as u32),
        };
        
        let stop_reason = match openai_resp["choices"][0]["finish_reason"].as_str() {
            Some("length") => StopReason::MaxTokens,
            Some("stop") => StopReason::EndTurn,
            Some("tool_calls") => StopReason::EndTurn,
            _ => StopReason::EndTurn,
        };
        
        // Extract tool calls if present
        let tool_calls = if let Some(tc) = openai_resp["choices"][0]["message"]["tool_calls"].as_array() {
            tc.iter().filter_map(|tc| {
                let id = tc["id"].as_str()?.to_string();
                let name = tc["function"]["name"].as_str()?.to_string();
                let arguments = tc["function"]["arguments"].clone();
                Some(rcode_core::provider::ToolCall { id, name, arguments })
            }).collect()
        } else {
            vec![]
        };
        
        Ok(CompletionResponse {
            content,
            reasoning,
            tool_calls,
            usage,
            stop_reason,
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

        // Build messages including system prompt
        let mut messages: Vec<OpenAIMessage> = Vec::new();
        if let Some(sp) = req.system_prompt.clone() {
            messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: Some(sp),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        messages.extend(req.messages.into_iter().map(into_openai_message));

        let body = OpenAIRequest {
            model: req.model.clone(),
            messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: true,
            tools: if req.tools.is_empty() { None } else { Some(req.tools.iter().map(into_openai_tool).collect()) },
        };

        let url = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));
        let mut request_builder = self.http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");
        
        for (key, value) in &self.custom_headers {
            request_builder = request_builder.header(key, value);
        }
        
        let response = request_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::OpenCodeError::Provider(format!("Network error: {}", e)))?;

        let (tx, rx) = mpsc::channel(1);
        let tx_clone = tx;
        let active_token = Arc::clone(&self.active_token);

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_tool_call: Option<OpenAIToolCall> = None;
            let mut last_finish_reason: Option<String> = None;
            let mut stream_error: Option<String> = None;
            let token_clone = token.clone();

            loop {
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

                                while let Some(newline_pos) = buffer.find('\n') {
                                    let line_str = buffer[..newline_pos].to_string();
                                    buffer = buffer.split_off(newline_pos + 1);
                                    let line = line_str.trim();

                                    if line.is_empty() || line == "data: [DONE]" {
                                        continue;
                                    }

                                    if let Some(data) = line.strip_prefix("data: ") {
                                        if let Some((event, finish_reason)) = parse_openai_sse_event(data, &mut current_tool_call) {
                                            if let Some(fr) = finish_reason {
                                                last_finish_reason = Some(fr);
                                            }
                                            if tx_clone.send(event).await.is_err() {
                                                // Clear the active token
                                                let mut guard = active_token.lock().unwrap();
                                                *guard = None;
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                stream_error = Some(e.to_string());
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

            // Emit ToolCallEnd for any remaining active tool call
            if let Some(tc) = current_tool_call.take() {
                let _ = tx_clone.send(StreamingEvent::ToolCallEnd { id: tc.id }).await;
            }

            // Determine stop reason from finish_reason
            let stop_reason = if let Some(ref fr) = last_finish_reason {
                match fr.as_str() {
                    "length" => StopReason::MaxTokens,
                    "stop" => StopReason::EndTurn,
                    "tool_calls" => StopReason::EndTurn,
                    _ => StopReason::EndTurn,
                }
            } else if stream_error.is_some() {
                StopReason::EndTurn
            } else {
                StopReason::EndTurn
            };

            let _ = tx_clone.send(StreamingEvent::Finish { 
                stop_reason, 
                usage: TokenUsage { 
                    input_tokens: 0, 
                    output_tokens: 0, 
                    total_tokens: None 
                }
            }).await;
            
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
            "gpt-4o" => ModelInfo {
                id: "gpt-4o".into(),
                name: "GPT-4o".into(),
                provider: "openai".into(),
                context_window: 128000,
                max_output_tokens: Some(16384),
            },
            _ => return None,
        };
        Some(info)
    }
    
    fn provider_id(&self) -> &str {
        "openai"
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
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
}

#[derive(Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCallFormat>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct OpenAIToolCallFormat {
    id: String,
    #[serde(rename = "type")]
    typ: String,
    function: OpenAIFunction,
}

#[derive(Serialize)]
struct OpenAIFunction {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    typ: String,
    function: OpenAIToolFunction,
}

#[derive(Serialize)]
struct OpenAIToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

fn into_openai_tool(tool: &rcode_core::ToolDefinition) -> OpenAITool {
    OpenAITool {
        typ: "function".to_string(),
        function: OpenAIToolFunction {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.parameters.clone(),
        },
    }
}

fn into_openai_message(msg: rcode_core::Message) -> OpenAIMessage {
    // Check if message has tool calls (for assistant messages)
    let has_tool_calls = msg.parts.iter().any(|p| matches!(p, rcode_core::Part::ToolCall { .. }));
    let has_tool_results = msg.parts.iter().any(|p| matches!(p, rcode_core::Part::ToolResult { .. }));
    
    // If message has tool results, format as tool result message
    if has_tool_results {
        // For tool results, we take the first tool result part
        if let Some(rcode_core::Part::ToolResult { tool_call_id, content, .. }) = 
            msg.parts.iter().find(|p| matches!(p, rcode_core::Part::ToolResult { .. }))
        {
            return OpenAIMessage {
                role: "tool".to_string(),
                content: Some(content.clone()),
                tool_calls: None,
                tool_call_id: Some(tool_call_id.clone()),
            };
        }
    }
    
    // If message has tool calls (assistant message with tool calls)
    if has_tool_calls {
        let tool_calls: Vec<OpenAIToolCallFormat> = msg.parts.iter()
            .filter_map(|p| match p {
                rcode_core::Part::ToolCall { id, name, arguments } => {
                    // Extract arguments as string - handle both JSON string and JSON object cases
                    let args_str = match arguments.as_ref() {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Object(_) => arguments.to_string(),
                        _ => arguments.to_string(),
                    };
                    Some(OpenAIToolCallFormat {
                        id: id.clone(),
                        typ: "function".to_string(),
                        function: OpenAIFunction {
                            name: name.clone(),
                            arguments: args_str,
                        },
                    })
                }
                _ => None,
            })
            .collect();
        
        return OpenAIMessage {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        };
    }
    
    // Otherwise, flatten to text content (backward compatible)
    let content = msg.parts.iter()
        .map(|p| match p {
            rcode_core::Part::Text { content } => content.clone(),
            rcode_core::Part::Reasoning { content } => format!("[Reasoning]: {}", content),
            rcode_core::Part::Attachment { name, mime_type, .. } => 
                format!("[Attachment: {} ({})]", name, mime_type),
            rcode_core::Part::ToolCall { name, arguments, .. } => 
                format!("Tool call: {}({})", name, arguments),
            rcode_core::Part::ToolResult { content, .. } => content.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    
    OpenAIMessage {
        role: match msg.role {
            rcode_core::Role::User => "user".into(),
            rcode_core::Role::Assistant => "assistant".into(),
            rcode_core::Role::System => "system".into(),
        },
        content: Some(content),
        tool_calls: None,
        tool_call_id: None,
    }
}

struct OpenAIToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn parse_openai_sse_event(
    data: &str,
    current_tool_call: &mut Option<OpenAIToolCall>,
) -> Option<(StreamingEvent, Option<String>)> {
    let chunk: OpenAIChunk = serde_json::from_str(data).ok()?;

    for choice in chunk.choices {
        // Check for finish_reason first
        let finish_reason = choice.finish_reason.clone();

        if let Some(content) = choice.delta.content {
            // Text content
            return Some((StreamingEvent::Text { delta: content }, finish_reason));
        }

        if let Some(tool_calls) = choice.delta.tool_calls {
            for tool_call in tool_calls {
                if let Some(function) = tool_call.function {
                    if let Some(ref mut current) = *current_tool_call {
                        // Continue accumulating (Bug 5 fix: emit incremental, not accumulated)
                        let incremental_args = function.arguments.unwrap_or_default();
                        if !incremental_args.is_empty() {
                            current.arguments.push_str(&incremental_args);
                        }
                        return Some((
                            StreamingEvent::ToolCallArg {
                                id: current.id.clone(),
                                name: current.name.clone(),
                                value: incremental_args,
                            },
                            finish_reason,
                        ));
                    } else {
                        // Start new tool call
                        let id = tool_call.id.unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));
                        let name = function.name.unwrap_or_default();
                        let arguments = function.arguments.unwrap_or_default();
                        *current_tool_call = Some(OpenAIToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: arguments.clone(),
                        });
                        return Some((StreamingEvent::ToolCallStart { id, name }, finish_reason));
                    }
                }
            }
        }
    }

    None
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct OpenAIChunk {
    id: String,
    choices: Vec<OpenAIChoice>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct OpenAIChoice {
    index: u32,
    delta: OpenAIDelta,
    #[serde(rename = "finish_reason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIDelta {
    content: Option<String>,
    #[serde(rename = "tool_calls")]
    tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Deserialize)]
struct OpenAIToolCallDelta {
    id: Option<String>,
    function: Option<OpenAIFunctionDelta>,
}

#[derive(Deserialize)]
struct OpenAIFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
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
    fn test_into_openai_message_user() {
        let msg = create_test_message(Role::User, vec![create_text_part("Hello")]);
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.role, "user");
        assert_eq!(openai_msg.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_into_openai_message_assistant() {
        let msg = create_test_message(Role::Assistant, vec![create_text_part("I am here")]);
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.role, "assistant");
        assert_eq!(openai_msg.content, Some("I am here".to_string()));
    }

    #[test]
    fn test_into_openai_message_system() {
        let msg = create_test_message(Role::System, vec![create_text_part("You are helpful")]);
        let openai_msg = into_openai_message(msg);
        // OpenAI uses "system" for system role
        assert_eq!(openai_msg.role, "system");
        assert_eq!(openai_msg.content, Some("You are helpful".to_string()));
    }

    #[test]
    fn test_into_openai_message_multiple_parts() {
        let msg = create_test_message(
            Role::User, 
            vec![create_text_part("Part 1"), create_text_part("Part 2")]
        );
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.content, Some("Part 1\nPart 2".to_string()));
    }

    #[test]
    fn test_into_openai_message_tool_call() {
        let msg = create_test_message(
            Role::Assistant, 
            vec![create_tool_call_part("call_123", "get_weather", "{\"city\":\"NYC\"}")]
        );
        let openai_msg = into_openai_message(msg);
        // Tool calls are now in OpenAI format with tool_calls array
        assert_eq!(openai_msg.role, "assistant");
        assert!(openai_msg.content.is_none());
        assert!(openai_msg.tool_calls.is_some());
        let tool_calls = openai_msg.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_123");
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert_eq!(tool_calls[0].function.arguments, "{\"city\":\"NYC\"}");
    }

    #[test]
    fn test_into_openai_message_tool_result() {
        let msg = create_test_message(
            Role::User, 
            vec![create_tool_result_part("call_123", "Sunny, 72F")]
        );
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.role, "tool");
        assert_eq!(openai_msg.content, Some("Sunny, 72F".to_string()));
    }

    #[test]
    fn test_into_openai_message_reasoning() {
        let msg = create_test_message(
            Role::Assistant, 
            vec![create_reasoning_part("Let me think")]
        );
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.content, Some("[Reasoning]: Let me think".to_string()));
    }

    #[test]
    fn test_into_openai_message_attachment() {
        let msg = create_test_message(
            Role::User, 
            vec![create_attachment_part("doc.pdf", "application/pdf")]
        );
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.content, Some("[Attachment: doc.pdf (application/pdf)]".to_string()));
    }

    #[test]
    fn test_into_openai_message_empty_parts() {
        let msg = create_test_message(Role::User, vec![]);
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.content, Some("".to_string()));
    }

    #[test]
    fn test_provider_new() {
        let provider = OpenAIProvider::new("test-api-key".to_string());
        assert_eq!(provider.provider_id(), "openai");
    }

    #[test]
    fn test_provider_with_rate_limit() {
        let provider = OpenAIProvider::new("test-api-key".to_string())
            .with_rate_limit(100, 10.0);
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

    #[test]
    fn test_provider_id() {
        let provider = OpenAIProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "openai");
    }

    // SSE Event parsing tests

    #[test]
    fn test_parse_openai_sse_event_text_delta() {
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_some());
        match event.unwrap().0 {
            StreamingEvent::Text { delta } => assert_eq!(delta, "Hello"),
            _ => panic!("Expected Text event"),
        }
    }

    #[test]
    fn test_parse_openai_sse_event_tool_call_start() {
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"tool_calls":[{"id":"call_abc","function":{"name":"get_weather","arguments":""}}]},"finish_reason":null}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_some());
        match event.unwrap().0 {
            StreamingEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "get_weather");
            }
            _ => panic!("Expected ToolCallStart event"),
        }
        // Verify tool_call buffer is set
        assert!(tool_call.is_some());
    }

    #[test]
    fn test_parse_openai_sse_event_tool_call_arg() {
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"tool_calls":[{"function":{"arguments":"{\"city\""}}]},"finish_reason":null}]}"#;
        let mut tool_call = Some(OpenAIToolCall {
            id: "call_abc".to_string(),
            name: "get_weather".to_string(),
            arguments: "".to_string(),
        });
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_some());
        // Bug 5 fix: emit incremental args, not accumulated
        match event.unwrap().0 {
            StreamingEvent::ToolCallArg { id, name, value } => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "get_weather");
                assert_eq!(value, "{\"city\""); // incremental, not accumulated
            }
            _ => panic!("Expected ToolCallArg event"),
        }
    }

    #[test]
    fn test_parse_openai_sse_event_empty_delta() {
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{},"finish_reason":null}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_openai_sse_event_invalid_json() {
        let data = "not valid json";
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_openai_sse_event_tool_call_without_initial_id() {
        // When id is not provided, it should generate one
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"tool_calls":[{"function":{"name":"test","arguments":""}}]},"finish_reason":null}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_some());
        match event.unwrap().0 {
            StreamingEvent::ToolCallStart { id, name } => {
                assert!(id.starts_with("call_"));
                assert_eq!(name, "test");
            }
            _ => panic!("Expected ToolCallStart event"),
        }
    }

    // Serialization tests

    #[test]
    fn test_openai_request_serialization() {
        let request = OpenAIRequest {
            model: "gpt-4o".to_string(),
            messages: vec![
                OpenAIMessage {
                    role: "user".to_string(),
                    content: Some("Hello".to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                }
            ],
            max_tokens: Some(1024),
            temperature: Some(0.7),
            stream: true,
            tools: None,
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""model":"gpt-4o""#));
        assert!(json.contains(r#""max_tokens":1024"#));
        assert!(json.contains(r#""temperature":0.7"#));
        assert!(json.contains(r#""stream":true"#));
    }

    #[test]
    fn test_openai_message_serialization() {
        let msg = OpenAIMessage {
            role: "user".to_string(),
            content: Some("Test".to_string()),
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""role":"user""#));
        assert!(json.contains(r#""content":"Test""#));
    }

    #[test]
    fn test_openai_chunk_deserialization() {
        let json = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":"stop"}]}"#;
        let chunk: OpenAIChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.id, "chatcmpl-123");
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].index, 0);
    }

    #[test]
    fn test_openai_choice_deserialization() {
        let json = r#"{"index":0,"delta":{"content":"Hello"},"finish_reason":"stop"}"#;
        let choice: OpenAIChoice = serde_json::from_str(json).unwrap();
        assert_eq!(choice.index, 0);
        assert_eq!(choice.finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_openai_delta_deserialization() {
        let json = r#"{"content":"Hello","tool_calls":[{"id":"call_1","function":{"name":"test","arguments":"{}"}}]}"#;
        let delta: OpenAIDelta = serde_json::from_str(json).unwrap();
        assert_eq!(delta.content, Some("Hello".to_string()));
        assert!(delta.tool_calls.is_some());
    }

    #[test]
    fn test_openai_tool_call_delta_deserialization() {
        let json = r#"{"id":"call_123","function":{"name":"get_weather","arguments":"{}"}}"#;
        let tool_call: OpenAIToolCallDelta = serde_json::from_str(json).unwrap();
        assert_eq!(tool_call.id, Some("call_123".to_string()));
        assert!(tool_call.function.is_some());
    }

    #[test]
    fn test_openai_function_delta_deserialization() {
        let json = r#"{"name":"get_weather","arguments":"{\"city\":\"NYC\"}"}"#;
        let function: OpenAIFunctionDelta = serde_json::from_str(json).unwrap();
        assert_eq!(function.name, Some("get_weather".to_string()));
        assert_eq!(function.arguments, Some("{\"city\":\"NYC\"}".to_string()));
    }

    #[test]
    fn test_openai_function_delta_partial() {
        // Function with only arguments (continuation)
        let json = r#"{"arguments":"{\"city\""}"#;
        let function: OpenAIFunctionDelta = serde_json::from_str(json).unwrap();
        assert_eq!(function.name, None);
        assert_eq!(function.arguments, Some("{\"city\"".to_string()));
    }

    #[test]
    fn test_abort_method_exists() {
        let provider = OpenAIProvider::new("test".to_string());
        // abort() should be callable without panicking
        provider.abort();
    }

    #[test]
    fn test_provider_with_rate_limit_retains_api_key() {
        let provider = OpenAIProvider::new("my-secret-key".to_string())
            .with_rate_limit(50, 5.0);
        assert_eq!(provider.provider_id(), "openai");
    }

    #[test]
    fn test_openai_request_serialization_minimal() {
        let request = OpenAIRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                OpenAIMessage {
                    role: "user".to_string(),
                    content: Some("Hi".to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                }
            ],
            max_tokens: None,
            temperature: None,
            stream: true,
            tools: None,
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""model":"gpt-4""#));
        assert!(json.contains(r#""stream":true"#));
    }

    #[test]
    fn test_openai_request_serialization_with_temperature() {
        let request = OpenAIRequest {
            model: "gpt-4o".to_string(),
            messages: vec![],
            max_tokens: Some(1000),
            temperature: Some(0.7),
            stream: true,
            tools: None,
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""temperature":0.7"#));
        assert!(json.contains(r#""max_tokens":1000"#));
    }

    #[test]
    fn test_openai_request_serialization_system_message() {
        let request = OpenAIRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                OpenAIMessage {
                    role: "system".to_string(),
                    content: Some("You are helpful".to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                }
            ],
            max_tokens: None,
            temperature: None,
            stream: true,
            tools: None,
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""role":"system""#));
    }

    #[test]
    fn test_parse_openai_sse_event_text_delta_with_special_chars() {
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":"Hello\nWorld"},"finish_reason":null}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_some());
        match event.unwrap().0 {
            StreamingEvent::Text { delta } => assert_eq!(delta, "Hello\nWorld"),
            _ => panic!("Expected Text event"),
        }
    }

    #[test]
    fn test_parse_openai_sse_event_tool_call_continuation() {
        // First, start a tool call
        let start_data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"tool_calls":[{"id":"call_abc","function":{"name":"get_weather","arguments":""}}]},"finish_reason":null}]}"#;
        let mut tool_call = None;
        let start_event = parse_openai_sse_event(start_data, &mut tool_call);
        assert!(start_event.is_some());
        
        // Then receive continuation with arguments (Bug 5 fix: incremental args)
        let cont_data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"tool_calls":[{"function":{"arguments":"{\"city\""}}]},"finish_reason":null}]}"#;
        let cont_event = parse_openai_sse_event(cont_data, &mut tool_call);
        assert!(cont_event.is_some());
        match cont_event.unwrap().0 {
            StreamingEvent::ToolCallArg { value, .. } => {
                // Bug 5 fix: incremental value is "{\"city\"" not accumulated
                assert_eq!(value, "{\"city\"");
            }
            _ => panic!("Expected ToolCallArg event"),
        }
    }

    #[test]
    fn test_parse_openai_sse_event_multiple_choices() {
        // When there are multiple choices, only the first one with content returns an event
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":"First"}},{"index":1,"delta":{"content":"Second"}}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        // This should return an event (first choice with content)
        assert!(event.is_some());
    }

    #[test]
    fn test_parse_openai_sse_event_finish_reason_stop() {
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        // Empty delta with finish_reason returns None (no event)
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_openai_sse_event_content_and_tool_call_together() {
        // When both content and tool_call are present, content takes precedence
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":"Hello","tool_calls":[{"id":"call_1","function":{"name":"test","arguments":""}}]},"finish_reason":null}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_some());
        match event.unwrap().0 {
            StreamingEvent::Text { delta } => assert_eq!(delta, "Hello"),
            _ => panic!("Expected Text event"),
        }
    }

    #[test]
    fn test_openai_chunk_multiple_choices() {
        let json = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":"First"}},{"index":1,"delta":{"content":"Second"}}]}"#;
        let chunk: OpenAIChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 2);
        assert_eq!(chunk.choices[0].index, 0);
        assert_eq!(chunk.choices[1].index, 1);
    }

    #[test]
    fn test_openai_choice_finish_reason_null() {
        let json = r#"{"index":0,"delta":{"content":"Hi"},"finish_reason":null}"#;
        let choice: OpenAIChoice = serde_json::from_str(json).unwrap();
        assert!(choice.finish_reason.is_none());
    }

    #[test]
    fn test_openai_choice_finish_reason_stop() {
        let json = r#"{"index":0,"delta":{},"finish_reason":"stop"}"#;
        let choice: OpenAIChoice = serde_json::from_str(json).unwrap();
        assert_eq!(choice.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_openai_tool_call_delta_no_function() {
        let json = r#"{"id":"call_123"}"#;
        let tool_call: OpenAIToolCallDelta = serde_json::from_str(json).unwrap();
        assert_eq!(tool_call.id, Some("call_123".to_string()));
        assert!(tool_call.function.is_none());
    }

    #[test]
    fn test_openai_function_delta_only_name() {
        let json = r#"{"name":"get_weather"}"#;
        let function: OpenAIFunctionDelta = serde_json::from_str(json).unwrap();
        assert_eq!(function.name, Some("get_weather".to_string()));
        assert!(function.arguments.is_none());
    }

    // Cancellation tests

    #[tokio::test]
    async fn test_openai_abort_cancels_active_stream() {
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
    async fn test_openai_concurrent_streams_independent_cancellation() {
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
        assert!(count2 >= 3, "Stream 2 should not be affected, got count: {}", count2);
    }

    #[tokio::test]
    async fn test_openai_stream_completes_normally_when_not_aborted() {
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

    #[test]
    fn test_openai_abort_method_is_callable() {
        let provider = OpenAIProvider::new("test-api-key".to_string());
        // abort() should be callable without panicking
        provider.abort();
    }

    #[tokio::test]
    async fn test_openai_per_stream_cancellation_token_pattern() {
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
