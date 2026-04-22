//! OpenAI-compatible response codec
//!
//! This module provides response parsing functions and types
//! for the OpenAI-compatible protocol.
//!
//! # Architecture
//!
//! The response parsing is organized around the `StreamingEventParser` trait,
//! which abstracts the differences between provider streaming protocols:
//!
//! - **OpenAI-compatible** (OpenAI, MiniMax, OpenRouter, ZAI): Single `data: {...}` SSE lines
//! - **Anthropic**: `event: type` + `data: {...}` SSE lines with structured content blocks
//!
//! Each provider implements `StreamingEventParser` to convert its specific
//! wire format into the provider-agnostic `StreamingEvent` enum.

use serde::Deserialize;
use rcode_core::{
    CompletionResponse, StreamingEvent, StreamingResponse,
    TokenUsage, error::Result,
};
use rcode_core::provider::StopReason;

// ═══════════════════════════════════════════════════════════════════════════════════
// Streaming Event Parser Trait
// ═══════════════════════════════════════════════════════════════════════════════════

/// Parser state that accumulates across streaming chunks.
///
/// Different providers may need to accumulate state (e.g., tool call arguments
/// that arrive across multiple SSE chunks, or reasoning active state).
///
/// Currently stores state for OpenAI-compatible and Anthropic protocols.
/// Each parser implementation manages its own state field.
#[derive(Debug, Default)]
pub struct ParserState {
    /// Accumulator for OpenAI-style tool call arguments
    pub openai_tool_call: Option<OpenAIToolCall>,
    /// Whether reasoning is currently active (from previous chunks).
    /// Used for proper sequencing: reasoning-end should be emitted before
    /// text or tool_calls when a new content type arrives.
    pub reasoning_active: bool,
    /// Accumulator for Anthropic-style tool call arguments (provider-specific state)
    /// Note: This is a placeholder - actual Anthropic parser would define its own state type
    #[allow(dead_code)]
    anthropic_state: Option<()>,
}

/// A parsed streaming event with optional finish_reason metadata.
#[derive(Debug)]
pub struct ParsedEvent {
    pub event: StreamingEvent,
    pub finish_reason: Option<String>,
}

/// Trait for parsing streaming events from a specific provider protocol.
///
/// Implementors must handle:
/// - Extracting events from provider-specific wire format
/// - Accumulating state across chunks (e.g., tool call args)
/// - Providing finish_reason when the stream ends
///
/// # Example
///
/// ```ignore
/// struct OpenAIStreamParser;
/// impl StreamingEventParser for OpenAIStreamParser {
///     fn parse(&self, data: &str, state: &mut ParserState) -> Vec<ParsedEvent> {
///         // Parse OpenAI SSE data and return events
///     }
/// }
/// ```
pub trait StreamingEventParser: Send + Sync {
    /// Parse a single SSE data payload and return events to emit.
    ///
    /// Some providers send multiple events within a single data payload.
    /// This method should return ALL events that should be emitted in order.
    ///
    /// # Arguments
    /// - `data`: The content after `data: ` prefix (without the prefix itself)
    /// - `state`: Mutable parser state, accumulated across chunks
    ///
    /// # Returns
    /// - Vec of `ParsedEvent`, each containing a `StreamingEvent` and optional `finish_reason`
    fn parse(&self, data: &str, state: &mut ParserState) -> Vec<ParsedEvent>;

    /// Returns the finish_reason string from a data payload, if present.
    ///
    /// This is used by the transport to track the final stop reason even when
    /// no meaningful content event is emitted.
    fn extract_finish_reason(&self, data: &str) -> Option<String>;
}

// ═══════════════════════════════════════════════════════════════════════════════════
// OpenAI-Compatible Parser Implementation
// ═══════════════════════════════════════════════════════════════════════════════════

/// OpenAI-compatible streaming event parser.
///
/// Handles the OpenAI SSE format where each data line contains a JSON chunk
/// with `choices[].delta` containing content, reasoning, or tool_calls.
#[derive(Debug, Clone, Default)]
pub struct OpenAIStreamParser;

impl OpenAIStreamParser {
    pub fn new() -> Self {
        Self
    }
}

impl StreamingEventParser for OpenAIStreamParser {
    fn parse(&self, data: &str, state: &mut ParserState) -> Vec<ParsedEvent> {
        let chunk: OpenAIChunk = match serde_json::from_str(data).ok() {
            Some(c) => c,
            None => return vec![],
        };

        let mut events = Vec::new();

        for mut choice in chunk.choices {
            let finish_reason = choice.finish_reason.clone();

            // Extract what's present in this chunk
            let has_tool_calls = choice.delta.tool_calls.is_some();
            let has_reasoning_this_chunk = choice.delta.reasoning_content.is_some();
            let has_content = choice.delta.content.as_ref().and_then(extract_content_text).is_some();

            // CRITICAL SEQUENCING (like Crush):
            // When reasoning is active from a PREVIOUS chunk and new content arrives (tool_calls or text),
            // we must emit ReasoningEnd BEFORE emitting the new content type.
            if state.reasoning_active && (has_tool_calls || has_content) {
                events.push(ParsedEvent {
                    event: StreamingEvent::ReasoningEnd,
                    finish_reason: finish_reason.clone(),
                });
                state.reasoning_active = false;
            }

            // Priority: tool_calls > reasoning > content (like Crush)
            // Process reasoning FIRST if present, then set reasoning_active for this chunk
            if let Some(reasoning) = choice.delta.reasoning_content {
                events.push(ParsedEvent {
                    event: StreamingEvent::Reasoning { delta: reasoning },
                    finish_reason: finish_reason.clone(),
                });
                // reasoning_active = true means we had reasoning in THIS chunk
                // If there are also tool_calls/text in this same chunk, we'll emit ReasoningEnd after
                state.reasoning_active = true;
            }

            // If this chunk has tool_calls AND we also had reasoning in this chunk,
            // emit ReasoningEnd to properly sequence (reasoning ended -> tool_calls started)
            if has_tool_calls && has_reasoning_this_chunk {
                events.push(ParsedEvent {
                    event: StreamingEvent::ReasoningEnd,
                    finish_reason: finish_reason.clone(),
                });
                state.reasoning_active = false;
            }

            if let Some(tool_calls) = choice.delta.tool_calls.take() {
                for tool_call in tool_calls {
                    if let Some(function) = tool_call.function {
                        if let Some(ref mut current) = state.openai_tool_call {
                            // Continue accumulating
                            let incremental_args = function.arguments.unwrap_or_default();
                            if !incremental_args.is_empty() {
                                current.arguments.push_str(&incremental_args);
                            }
                            events.push(ParsedEvent {
                                event: StreamingEvent::ToolCallArg {
                                    id: current.id.clone(),
                                    name: current.name.clone(),
                                    value: incremental_args,
                                },
                                finish_reason: finish_reason.clone(),
                            });
                        } else {
                            // Start new tool call
                            let id = tool_call.id.unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));
                            let name = function.name.unwrap_or_default();
                            let arguments = function.arguments.unwrap_or_default();
                            state.openai_tool_call = Some(OpenAIToolCall {
                                id: id.clone(),
                                name: name.clone(),
                                arguments: arguments.clone(),
                            });
                            events.push(ParsedEvent {
                                event: StreamingEvent::ToolCallStart { id, name },
                                finish_reason: finish_reason.clone(),
                            });
                        }
                    }
                }
            }

            // Only emit content if we didn't emit tool_calls (tool_calls take precedence)
            if !has_tool_calls {
                if let Some(content) = choice.delta.content.as_ref().and_then(extract_content_text) {
                    events.push(ParsedEvent {
                        event: StreamingEvent::Text { delta: content },
                        finish_reason: finish_reason.clone(),
                    });
                }
            }
        }

        events
    }

    fn extract_finish_reason(&self, data: &str) -> Option<String> {
        let chunk: OpenAIChunk = serde_json::from_str(data).ok()?;
        chunk.choices.into_iter().next()?.finish_reason
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════
// Legacy Parse Functions (for backward compatibility)
// ═══════════════════════════════════════════════════════════════════════════════════

/// Parse a non-streaming completion response from OpenAI API JSON
pub fn parse_completion_response(json: serde_json::Value) -> Result<CompletionResponse> {
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let reasoning = json["choices"][0]["message"]["reasoning"]
        .as_str()
        .map(String::from);

    let usage = TokenUsage {
        input_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        output_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
        total_tokens: json["usage"]["total_tokens"].as_u64().map(|t| t as u32),
    };

    let stop_reason = match json["choices"][0]["finish_reason"].as_str() {
        Some("length") => StopReason::MaxTokens,
        Some("stop") => StopReason::EndTurn,
        Some("tool_calls") => StopReason::EndTurn,
        _ => StopReason::EndTurn,
    };

    // Extract tool calls if present
    let tool_calls = if let Some(tc) = json["choices"][0]["message"]["tool_calls"].as_array() {
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

/// OpenAI streaming chunk
#[allow(dead_code)]
#[derive(Deserialize)]
pub struct OpenAIChunk {
    pub id: String,
    pub choices: Vec<OpenAIChoice>,
}

/// OpenAI choice in a chunk
#[allow(dead_code)]
#[derive(Deserialize)]
pub struct OpenAIChoice {
    pub index: u32,
    pub delta: OpenAIDelta,
    #[serde(rename = "finish_reason")]
    pub finish_reason: Option<String>,
}

/// OpenAI delta within a choice
#[derive(Deserialize)]
pub struct OpenAIDelta {
    pub content: Option<serde_json::Value>,
    pub reasoning_content: Option<String>,
    #[serde(rename = "tool_calls")]
    pub tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

/// OpenAI tool call delta
#[derive(Deserialize)]
pub struct OpenAIToolCallDelta {
    pub id: Option<String>,
    pub function: Option<OpenAIFunctionDelta>,
}

/// OpenAI function delta within a tool call
#[derive(Deserialize)]
pub struct OpenAIFunctionDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

/// Internal tool call state for SSE accumulation
#[derive(Debug)]
pub struct OpenAIToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Parse streaming response from JSON payload (non-SSE fallback)
pub fn streaming_response_from_json_payload(payload: &str) -> Result<StreamingResponse> {
    let openai_resp: serde_json::Value = serde_json::from_str(payload)
        .map_err(|e| rcode_core::RCodeError::Provider(format!("Failed to parse JSON streaming fallback: {}", e)))?;

    let content = openai_resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let reasoning = openai_resp["choices"][0]["message"]["reasoning"]
        .as_str()
        .or_else(|| openai_resp["choices"][0]["message"]["reasoning_content"].as_str())
        .map(str::to_string);

    let stop_reason = match openai_resp["choices"][0]["finish_reason"].as_str() {
        Some("length") => StopReason::MaxTokens,
        Some("stop") => StopReason::EndTurn,
        Some("tool_calls") => StopReason::EndTurn,
        _ => StopReason::EndTurn,
    };

    let usage = TokenUsage {
        input_tokens: openai_resp["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        output_tokens: openai_resp["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
        total_tokens: openai_resp["usage"]["total_tokens"].as_u64().map(|t| t as u32),
    };

    let mut events = Vec::new();
    if !content.is_empty() {
        events.push(StreamingEvent::Text { delta: content });
    }
    if let Some(reasoning) = reasoning {
        if !reasoning.is_empty() {
            events.push(StreamingEvent::Reasoning { delta: reasoning });
        }
    }
    events.push(StreamingEvent::Finish { stop_reason, usage });

    Ok(StreamingResponse {
        events: Box::pin(tokio_stream::iter(events)),
    })
}

/// Parse a single SSE event from OpenAI streaming response
///
/// `current_tool_call` is caller-supplied state for accumulating
/// tool call arguments across multiple SSE chunks.
pub fn parse_openai_sse_event(
    data: &str,
    current_tool_call: &mut Option<OpenAIToolCall>,
) -> Option<(StreamingEvent, Option<String>)> {
    let chunk: OpenAIChunk = serde_json::from_str(data).ok()?;

    for choice in chunk.choices {
        // Check for finish_reason first
        let finish_reason = choice.finish_reason.clone();

        // Priority: tool_calls > reasoning > content
        // This ensures tool calls are never lost when they appear alongside text

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

        if let Some(reasoning) = choice.delta.reasoning_content {
            return Some((StreamingEvent::Reasoning { delta: reasoning }, finish_reason));
        }

        if let Some(content) = choice.delta.content.as_ref().and_then(extract_content_text) {
            // Text content
            return Some((StreamingEvent::Text { delta: content }, finish_reason));
        }
    }

    None
}

/// Extract text content from polymorphic delta value
pub fn extract_content_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(parts) => {
            let text = parts.iter()
                .filter_map(|part| match part {
                    serde_json::Value::String(text) => Some(text.clone()),
                    serde_json::Value::Object(obj) => obj
                        .get("text")
                        .and_then(|value| value.as_str().map(str::to_string))
                        .or_else(|| obj.get("content").and_then(|value| value.as_str().map(str::to_string))),
                    _ => None,
                })
                .collect::<String>();

            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        serde_json::Value::Object(obj) => obj
            .get("text")
            .and_then(|value| value.as_str().map(str::to_string))
            .or_else(|| obj.get("content").and_then(|value| value.as_str().map(str::to_string))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_openai_sse_event_text_delta_from_object_content() {
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":{"text":"Hello"}},"finish_reason":null}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_some());
        match event.unwrap().0 {
            StreamingEvent::Text { delta } => assert_eq!(delta, "Hello"),
            _ => panic!("Expected Text event"),
        }
    }

    #[test]
    fn test_parse_openai_sse_event_text_delta_from_array_content() {
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":[{"text":"Hel"},{"text":"lo"}]},"finish_reason":null}]}"#;
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

    // Deserialization tests

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
    fn test_openai_delta_deserialization() {
        let json = r#"{"content":"Hello","tool_calls":[{"id":"call_1","function":{"name":"test","arguments":"{}"}}]}"#;
        let delta: OpenAIDelta = serde_json::from_str(json).unwrap();
        assert_eq!(delta.content, Some(serde_json::json!("Hello")));
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
    fn test_openai_tool_call_delta_no_function() {
        let json = r#"{"id":"call_123"}"#;
        let tool_call: OpenAIToolCallDelta = serde_json::from_str(json).unwrap();
        assert_eq!(tool_call.id, Some("call_123".to_string()));
        assert!(tool_call.function.is_none());
    }

    #[test]
    fn test_openai_function_delta_deserialization() {
        let json = r#"{"name":"get_weather","arguments":"{\"city\":\"NYC\"}"}"#;
        let function: OpenAIFunctionDelta = serde_json::from_str(json).unwrap();
        assert_eq!(function.name, Some("get_weather".to_string()));
        assert_eq!(function.arguments, Some("{\"city\":\"NYC\"}".to_string()));
    }

    #[test]
    fn test_openai_function_delta_only_name() {
        let json = r#"{"name":"get_weather"}"#;
        let function: OpenAIFunctionDelta = serde_json::from_str(json).unwrap();
        assert_eq!(function.name, Some("get_weather".to_string()));
        assert!(function.arguments.is_none());
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
    fn test_extract_content_text_string() {
        let json = serde_json::json!("Hello");
        assert_eq!(extract_content_text(&json), Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_content_text_object_with_text() {
        let json = serde_json::json!({"text": "Hello"});
        assert_eq!(extract_content_text(&json), Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_content_text_object_with_content() {
        let json = serde_json::json!({"content": "Hello"});
        assert_eq!(extract_content_text(&json), Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_content_text_array() {
        let json = serde_json::json!([{"text": "Hel"}, {"text": "lo"}]);
        assert_eq!(extract_content_text(&json), Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_content_text_empty_array() {
        let json = serde_json::json!([]);
        assert_eq!(extract_content_text(&json), None);
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
        // When both content and tool_call are present, tool_call takes precedence
        // This ensures tool calls are never lost when they appear alongside text
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":"Hello","tool_calls":[{"id":"call_1","function":{"name":"test","arguments":""}}]},"finish_reason":null}]}"#;
        let mut tool_call = None;
        let event = parse_openai_sse_event(data, &mut tool_call);
        assert!(event.is_some());
        match event.unwrap().0 {
            StreamingEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "call_1");
                assert_eq!(name, "test");
            }
            _ => panic!("Expected ToolCallStart event, prioritizing tool calls over text"),
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

    #[tokio::test]
    async fn test_streaming_response_from_json_payload() {
        let json = r#"{
            "choices": [{
                "message": {
                    "content": "Hello",
                    "reasoning": "Let me think"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;
        
        let response = streaming_response_from_json_payload(json).unwrap();
        let events: Vec<_> = tokio_stream::StreamExt::collect(response.events).await;
        
        assert_eq!(events.len(), 3); // Text + Reasoning + Finish
        match &events[0] {
            StreamingEvent::Text { delta } => assert_eq!(delta, "Hello"),
            _ => panic!("Expected Text event"),
        }
        match &events[1] {
            StreamingEvent::Reasoning { delta } => assert_eq!(delta, "Let me think"),
            _ => panic!("Expected Reasoning event"),
        }
        match &events[2] {
            StreamingEvent::Finish { stop_reason, usage } => {
                assert_eq!(*stop_reason, StopReason::EndTurn);
                assert_eq!(usage.input_tokens, 10);
                assert_eq!(usage.output_tokens, 5);
            }
            _ => panic!("Expected Finish event"),
        }
    }

    #[tokio::test]
    async fn test_streaming_response_from_json_payload_empty_content() {
        let json = r#"{
            "choices": [{
                "message": {
                    "content": ""
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 0,
                "total_tokens": 10
            }
        }"#;
        
        let response = streaming_response_from_json_payload(json).unwrap();
        let events: Vec<_> = tokio_stream::StreamExt::collect(response.events).await;
        
        // Only Finish event since content is empty
        assert_eq!(events.len(), 1);
        match &events[0] {
            StreamingEvent::Finish { stop_reason, .. } => {
                assert_eq!(*stop_reason, StopReason::EndTurn);
            }
            _ => panic!("Expected Finish event"),
        }
    }

    #[tokio::test]
    async fn test_streaming_response_from_json_payload_with_reasoning_content_fallback() {
        // Test that reasoning_content is used as fallback when reasoning is absent
        let json = r#"{
            "choices": [{
                "message": {
                    "content": "Hello",
                    "reasoning_content": "Let me think harder"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;
        
        let response = streaming_response_from_json_payload(json).unwrap();
        let events: Vec<_> = tokio_stream::StreamExt::collect(response.events).await;
        
        assert_eq!(events.len(), 3); // Text + Reasoning + Finish
        match &events[1] {
            StreamingEvent::Reasoning { delta } => assert_eq!(delta, "Let me think harder"),
            _ => panic!("Expected Reasoning event"),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════════════
    // Crush Edge Case Tests
    // These tests verify the 3 critical edge cases identified in Crush's streaming architecture:
    // 1. reasoning_opaque and content come in the SAME chunk
    // 2. reasoning goes directly to tool_calls with NO content
    // 3. reasoning_opaque and tool_calls come in the same chunk
    // ═══════════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_openai_stream_parser_reasoning_and_content_same_chunk() {
        // Edge Case 1: reasoning_opaque and content come in the SAME chunk
        // When a chunk contains both reasoning and content, we should:
        // - Set reasoning_active = true
        // - But since content comes AFTER reasoning in priority order, emit ReasoningEnd first
        // - Then emit Text
        let parser = OpenAIStreamParser::new();
        let mut state = ParserState::default();
        
        // Chunk with both reasoning and content
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"reasoning_content":"thinking...","content":"Hello"},"finish_reason":null}]}"#;
        
        let events = parser.parse(data, &mut state);
        
        // Should emit: ReasoningEnd (from prior state), Reasoning, Text
        // But since this is first chunk with no prior reasoning_active, just Reasoning + Text
        assert_eq!(events.len(), 2);
        match &events[0].event {
            StreamingEvent::Reasoning { delta } => assert_eq!(delta, "thinking..."),
            _ => panic!("Expected Reasoning first"),
        }
        match &events[1].event {
            StreamingEvent::Text { delta } => assert_eq!(delta, "Hello"),
            _ => panic!("Expected Text second"),
        }
    }

    #[test]
    fn test_openai_stream_parser_reasoning_to_tool_calls_no_content() {
        // Edge Case 2: reasoning goes directly to tool_calls with NO content
        // When reasoning was active and next chunk has tool_calls (no content/reasoning):
        // - Emit ReasoningEnd first
        // - Then emit ToolCallStart
        let parser = OpenAIStreamParser::new();
        let mut state = ParserState::default();
        
        // First chunk: reasoning active
        let data1 = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"reasoning_content":"thinking..."},"finish_reason":null}]}"#;
        let events1 = parser.parse(data1, &mut state);
        assert_eq!(events1.len(), 1);
        assert!(matches!(&events1[0].event, StreamingEvent::Reasoning { .. }));
        assert!(state.reasoning_active);
        
        // Second chunk: tool_calls but no content/reasoning
        let data2 = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"tool_calls":[{"id":"call_abc","function":{"name":"get_weather","arguments":""}}]},"finish_reason":null}]}"#;
        let events2 = parser.parse(data2, &mut state);
        
        // Should emit ReasoningEnd first, then ToolCallStart
        assert_eq!(events2.len(), 2);
        match &events2[0].event {
            StreamingEvent::ReasoningEnd => {},
            _ => panic!("Expected ReasoningEnd first, got {:?}", events2[0].event),
        }
        match &events2[1].event {
            StreamingEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "get_weather");
            },
            _ => panic!("Expected ToolCallStart second"),
        }
        assert!(!state.reasoning_active);
    }

    #[test]
    fn test_openai_stream_parser_reasoning_and_tool_calls_same_chunk() {
        // Edge Case 3: reasoning and tool_calls come in the same chunk
        // When a chunk contains both reasoning and tool_calls:
        // 1. Emit Reasoning (the delta from this chunk)
        // 2. Emit ReasoningEnd (because tool_calls arrived in same chunk = reasoning ended)
        // 3. Emit ToolCallStart
        let parser = OpenAIStreamParser::new();
        let mut state = ParserState::default();

        // Chunk with both reasoning and tool_calls
        let data = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"reasoning_content":"thinking...","tool_calls":[{"id":"call_abc","function":{"name":"get_weather","arguments":""}}]},"finish_reason":null}]}"#;

        let events = parser.parse(data, &mut state);

        // Should emit: Reasoning -> ReasoningEnd -> ToolCallStart
        assert_eq!(events.len(), 3);
        match &events[0].event {
            StreamingEvent::Reasoning { delta } => assert_eq!(delta, "thinking..."),
            _ => panic!("Expected Reasoning first, got {:?}", events[0].event),
        }
        match &events[1].event {
            StreamingEvent::ReasoningEnd => {},
            _ => panic!("Expected ReasoningEnd second, got {:?}", events[1].event),
        }
        match &events[2].event {
            StreamingEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "get_weather");
            },
            _ => panic!("Expected ToolCallStart third, got {:?}", events[2].event),
        }
    }

    #[test]
    fn test_openai_stream_parser_reasoning_active_then_text_emit_end_first() {
        // Verify that when reasoning_active is true and text arrives,
        // ReasoningEnd is emitted BEFORE Text
        let parser = OpenAIStreamParser::new();
        let mut state = ParserState::default();
        
        // First chunk: reasoning starts
        let data1 = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"reasoning_content":"thinking..."},"finish_reason":null}]}"#;
        let events1 = parser.parse(data1, &mut state);
        assert!(matches!(&events1[0].event, StreamingEvent::Reasoning { .. }));
        assert!(state.reasoning_active);
        
        // Second chunk: text arrives while reasoning_active
        let data2 = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let events2 = parser.parse(data2, &mut state);
        
        // Must emit ReasoningEnd BEFORE Text
        assert_eq!(events2.len(), 2);
        match &events2[0].event {
            StreamingEvent::ReasoningEnd => {},
            _ => panic!("Expected ReasoningEnd first, got {:?}", events2[0].event),
        }
        match &events2[1].event {
            StreamingEvent::Text { delta } => assert_eq!(delta, "Hello"),
            _ => panic!("Expected Text second"),
        }
        assert!(!state.reasoning_active);
    }

    #[test]
    fn test_openai_stream_parser_multiple_reasoning_chunks_then_end() {
        // Verify reasoning_active persists across multiple reasoning chunks
        let parser = OpenAIStreamParser::new();
        let mut state = ParserState::default();
        
        // First reasoning chunk
        let data1 = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"reasoning_content":"step 1..."},"finish_reason":null}]}"#;
        let events1 = parser.parse(data1, &mut state);
        assert!(matches!(&events1[0].event, StreamingEvent::Reasoning { .. }));
        assert!(state.reasoning_active);
        
        // Second reasoning chunk (continuation)
        let data2 = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"reasoning_content":"step 2..."},"finish_reason":null}]}"#;
        let events2 = parser.parse(data2, &mut state);
        assert!(matches!(&events2[0].event, StreamingEvent::Reasoning { .. }));
        assert!(state.reasoning_active);
        
        // Third chunk: content ends reasoning
        let data3 = r#"{"id":"chatcmpl-123","choices":[{"index":0,"delta":{"content":"done"},"finish_reason":null}]}"#;
        let events3 = parser.parse(data3, &mut state);
        
        assert_eq!(events3.len(), 2);
        assert!(matches!(&events3[0].event, StreamingEvent::ReasoningEnd));
        assert!(matches!(&events3[1].event, StreamingEvent::Text { .. }));
        assert!(!state.reasoning_active);
    }

    #[test]
    fn test_openai_stream_parser_tool_call_accumulation() {
        // Verify tool call arguments are accumulated correctly across chunks
        use serde_json::json;

        let parser = OpenAIStreamParser::new();
        let mut state = ParserState::default();

        // Start tool call
        let data1 = json!({
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "id": "call_abc",
                        "function": {"name": "get_weather", "arguments": ""}
                    }]
                },
                "finish_reason": null
            }]
        }).to_string();
        let events1 = parser.parse(&data1, &mut state);
        assert!(matches!(&events1[0].event, StreamingEvent::ToolCallStart { id, name } if id == "call_abc" && name == "get_weather"));

        // Accumulate arguments: first chunk of args
        let data2 = json!({
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "function": {"arguments": "{\"city\""}
                    }]
                },
                "finish_reason": null
            }]
        }).to_string();
        let events2 = parser.parse(&data2, &mut state);
        assert!(matches!(&events2[0].event, StreamingEvent::ToolCallArg { value, .. } if value == "{\"city\""));

        // More accumulation: middle chunk with quotes
        let data3 = json!({
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "function": {"arguments": ": \"NYC\""}
                    }]
                },
                "finish_reason": null
            }]
        }).to_string();
        let events3 = parser.parse(&data3, &mut state);
        // The arguments value is ": "NYC"" (with literal quotes around NYC)
        assert!(matches!(&events3[0].event, StreamingEvent::ToolCallArg { value, .. } if value == ": \"NYC\""));

        // Final arguments: closing brace
        let data4 = json!({
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "function": {"arguments": "}"}
                    }]
                },
                "finish_reason": null
            }]
        }).to_string();
        let events4 = parser.parse(&data4, &mut state);
        assert!(matches!(&events4[0].event, StreamingEvent::ToolCallArg { value, .. } if value == "}"));
    }
}
