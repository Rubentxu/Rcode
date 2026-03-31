//! OpenAI provider implementation

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use opencode_core::{
    CompletionRequest, CompletionResponse, ModelInfo,
    StreamingEvent, StreamingResponse,
    TokenUsage, error::Result,
};
use opencode_core::provider::StopReason;

use super::LlmProvider;

pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    http_client: Client,
}

impl OpenAIProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.openai.com".to_string(),
            http_client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse> {
        todo!()
    }
    
    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse> {
        let body = OpenAIRequest {
            model: req.model.clone(),
            messages: req.messages.into_iter().map(into_openai_message).collect(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: true,
        };

        let response = self.http_client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| opencode_core::OpenCodeError::Provider(format!("Network error: {}", e)))?;

        let (tx, rx) = tokio::sync::broadcast::channel(100);
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_tool_call: Option<OpenAIToolCall> = None;

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        let text = String::from_utf8_lossy(&chunk);
                        buffer.push_str(&text);

                        // Process complete lines
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line_str = (&buffer[..newline_pos]).to_string();
                            let remainder_str = buffer[newline_pos + 1..].to_string();
                            buffer = remainder_str;
                            let line = line_str.trim();

                            if line.is_empty() || line == "data: [DONE]" {
                                continue;
                            }

                            if let Some(data) = line.strip_prefix("data: ") {
                                if let Some(event) = parse_openai_sse_event(data, &mut current_tool_call) {
                                    let _ = tx_clone.send(event);
                                }
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
                        });
                        break;
                    }
                }
            }

            // Send finish event if not already sent
            let _ = tx_clone.send(StreamingEvent::Finish { 
                stop_reason: StopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 0, 
                    output_tokens: 0, 
                    total_tokens: None 
                }
            });
        });

        Ok(StreamingResponse {
            events: tokio_stream::wrappers::BroadcastStream::new(rx),
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
        // TODO: Implement proper abort using CancelToken
    }
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

fn into_openai_message(msg: opencode_core::Message) -> OpenAIMessage {
    let content = msg.parts.iter()
        .map(|p| match p {
            opencode_core::Part::Text { content } => content.clone(),
            opencode_core::Part::ToolResult { content, .. } => content.clone(),
            opencode_core::Part::ToolCall { name, arguments, .. } => 
                format!("Tool call: {}({})", name, arguments),
            opencode_core::Part::Reasoning { content } => format!("[Reasoning]: {}", content),
            opencode_core::Part::Attachment { name, mime_type, .. } => 
                format!("[Attachment: {} ({})]", name, mime_type),
        })
        .collect::<Vec<_>>()
        .join("\n");
    
    OpenAIMessage {
        role: match msg.role {
            opencode_core::Role::User => "user".into(),
            opencode_core::Role::Assistant => "assistant".into(),
            opencode_core::Role::System => "system".into(),
        },
        content,
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
) -> Option<StreamingEvent> {
    let chunk: OpenAIChunk = serde_json::from_str(data).ok()?;

    for choice in chunk.choices {
        if let Some(content) = choice.delta.content {
            // Text content
            return Some(StreamingEvent::Text { delta: content });
        }

        if let Some(tool_calls) = choice.delta.tool_calls {
            for tool_call in tool_calls {
                if let Some(function) = tool_call.function {
                    if let Some(ref mut current) = *current_tool_call {
                        // Continue accumulating
                        if let Some(args) = &function.arguments {
                            current.arguments.push_str(args);
                        }
                        return Some(StreamingEvent::ToolCallArg {
                            id: current.id.clone(),
                            name: current.name.clone(),
                            value: current.arguments.clone(),
                        });
                    } else {
                        // Start new tool call
                        let id = tool_call.id.unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));
                        let name = function.name.unwrap_or_default();
                        *current_tool_call = Some(OpenAIToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: function.arguments.unwrap_or_default(),
                        });
                        return Some(StreamingEvent::ToolCallStart { id, name });
                    }
                }
            }
        }
    }

    None
}

#[derive(Deserialize)]
struct OpenAIChunk {
    id: String,
    choices: Vec<OpenAIChoice>,
}

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
