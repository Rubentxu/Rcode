//! OpenAI-compatible request codec
//!
//! This module provides request serialization types and conversion functions
//! for the OpenAI-compatible protocol.

use rcode_core::{CompletionRequest, Message, Part, Role};
use serde::Serialize;

/// OpenAI chat completions request body
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAITool>>,
}

/// OpenAI message format
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCallFormat>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// OpenAI tool call format (for assistant messages with tool calls)
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIToolCallFormat {
    pub id: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub function: OpenAIFunction,
}

/// OpenAI function definition within a tool call
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIFunction {
    pub name: String,
    pub arguments: String,
}

/// OpenAI tool definition
#[derive(Debug, Clone, Serialize)]
pub struct OpenAITool {
    #[serde(rename = "type")]
    pub typ: String,
    pub function: OpenAIToolFunction,
}

/// OpenAI function definition within a tool
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Convert a `ToolDefinition` to OpenAI tool format
pub fn into_openai_tool(tool: &rcode_core::ToolDefinition) -> OpenAITool {
    OpenAITool {
        typ: "function".to_string(),
        function: OpenAIToolFunction {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.parameters.clone(),
        },
    }
}

/// Convert a `Message` to OpenAI message format
pub fn into_openai_message(msg: Message) -> OpenAIMessage {
    // Check if message has tool calls (for assistant messages)
    let has_tool_calls = msg.parts.iter().any(|p| matches!(p, Part::ToolCall { .. }));
    let has_tool_results = msg
        .parts
        .iter()
        .any(|p| matches!(p, Part::ToolResult { .. }));

    // If message has tool results, format as tool result message
    if has_tool_results {
        // For tool results, we take the first tool result part
        if let Some(Part::ToolResult {
            tool_call_id,
            content,
            ..
        }) = msg
            .parts
            .iter()
            .find(|p| matches!(p, Part::ToolResult { .. }))
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
        let tool_calls: Vec<OpenAIToolCallFormat> = msg
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
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
    let content = msg
        .parts
        .iter()
        .map(|p| match p {
            Part::Text { content } => content.clone(),
            Part::Reasoning { content } => format!("[Reasoning]: {}", content),
            Part::Attachment {
                name, mime_type, ..
            } => format!("[Attachment: {} ({})]", name, mime_type),
            Part::ToolCall {
                name, arguments, ..
            } => format!("Tool call: {}({})", name, arguments),
            Part::ToolResult { content, .. } => content.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    OpenAIMessage {
        role: match msg.role {
            Role::User => "user".into(),
            Role::Assistant => "assistant".into(),
            Role::System => "system".into(),
        },
        content: Some(content),
        tool_calls: None,
        tool_call_id: None,
    }
}

/// Build an OpenAI request from a CompletionRequest
///
/// The `stream` parameter controls whether this is a streaming request.
/// - Pass `false` for regular non-streaming completions (`post()`)
/// - Pass `true` for streaming completions (`post_streaming()`)
pub fn build_openai_request(
    req: CompletionRequest,
    system_prompt: Option<String>,
    stream: bool,
) -> OpenAIRequest {
    let mut messages: Vec<OpenAIMessage> = Vec::new();

    // Prepend system prompt if provided
    if let Some(sp) = system_prompt {
        messages.push(OpenAIMessage {
            role: "system".to_string(),
            content: Some(sp),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    // Convert messages
    messages.extend(req.messages.into_iter().map(into_openai_message));

    // Build tools list if not empty
    let tools = if req.tools.is_empty() {
        None
    } else {
        Some(req.tools.iter().map(into_openai_tool).collect())
    };

    OpenAIRequest {
        model: req.model,
        messages,
        max_tokens: req.max_tokens,
        temperature: req.temperature,
        stream,
        tools,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{Message, MessageId, Part};

    fn create_test_message(role: Role, parts: Vec<Part>) -> Message {
        Message {
            id: MessageId("msg1".to_string()),
            session_id: "session1".to_string(),
            role,
            parts,
            created_at: chrono::Utc::now(),
        }
    }

    fn create_text_part(content: &str) -> Part {
        Part::Text {
            content: content.to_string(),
        }
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
        Part::Reasoning {
            content: content.to_string(),
        }
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
        assert_eq!(openai_msg.role, "system");
        assert_eq!(openai_msg.content, Some("You are helpful".to_string()));
    }

    #[test]
    fn test_into_openai_message_multiple_parts() {
        let msg = create_test_message(
            Role::User,
            vec![create_text_part("Part 1"), create_text_part("Part 2")],
        );
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.content, Some("Part 1\nPart 2".to_string()));
    }

    #[test]
    fn test_into_openai_message_tool_call() {
        let msg = create_test_message(
            Role::Assistant,
            vec![create_tool_call_part(
                "call_123",
                "get_weather",
                "{\"city\":\"NYC\"}",
            )],
        );
        let openai_msg = into_openai_message(msg);
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
            vec![create_tool_result_part("call_123", "Sunny, 72F")],
        );
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.role, "tool");
        assert_eq!(openai_msg.content, Some("Sunny, 72F".to_string()));
    }

    #[test]
    fn test_into_openai_message_reasoning() {
        let msg = create_test_message(Role::Assistant, vec![create_reasoning_part("Let me think")]);
        let openai_msg = into_openai_message(msg);
        assert_eq!(
            openai_msg.content,
            Some("[Reasoning]: Let me think".to_string())
        );
    }

    #[test]
    fn test_into_openai_message_attachment() {
        let msg = create_test_message(
            Role::User,
            vec![create_attachment_part("doc.pdf", "application/pdf")],
        );
        let openai_msg = into_openai_message(msg);
        assert_eq!(
            openai_msg.content,
            Some("[Attachment: doc.pdf (application/pdf)]".to_string())
        );
    }

    #[test]
    fn test_into_openai_message_empty_parts() {
        let msg = create_test_message(Role::User, vec![]);
        let openai_msg = into_openai_message(msg);
        assert_eq!(openai_msg.content, Some("".to_string()));
    }

    // Serialization tests

    #[test]
    fn test_openai_request_serialization() {
        let request = OpenAIRequest {
            model: "gpt-4o".to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: Some("Hello".to_string()),
                tool_calls: None,
                tool_call_id: None,
            }],
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
    fn test_build_openai_request_with_system_prompt() {
        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![create_test_message(
                Role::User,
                vec![create_text_part("Hello")],
            )],
            system_prompt: None, // system prompt passed as separate arg
            tools: vec![],
            temperature: None,
            max_tokens: Some(1024),
            reasoning_effort: None,
        };

        let openai_req = build_openai_request(req, Some("You are helpful".to_string()), false);

        assert_eq!(openai_req.model, "gpt-4o");
        assert_eq!(openai_req.messages.len(), 2); // system + user
        assert_eq!(openai_req.messages[0].role, "system");
        assert_eq!(
            openai_req.messages[0].content,
            Some("You are helpful".to_string())
        );
    }

    #[test]
    fn test_build_openai_request_with_tools() {
        let tool = rcode_core::ToolDefinition {
            name: "get_weather".to_string(),
            description: "Get weather for a city".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"}
                }
            }),
        };

        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![create_test_message(
                Role::User,
                vec![create_text_part("What's the weather?")],
            )],
            system_prompt: None,
            tools: vec![tool],
            temperature: None,
            max_tokens: Some(1024),
            reasoning_effort: None,
        };

        let openai_req = build_openai_request(req, None, false);

        assert!(openai_req.tools.is_some());
        let tools = openai_req.tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "get_weather");
    }

    #[test]
    fn test_build_openai_request_empty_tools() {
        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![create_test_message(
                Role::User,
                vec![create_text_part("Hello")],
            )],
            system_prompt: None,
            tools: vec![],
            temperature: None,
            max_tokens: Some(1024),
            reasoning_effort: None,
        };

        let openai_req = build_openai_request(req, None, false);

        assert!(openai_req.tools.is_none());
    }

    #[test]
    fn test_build_openai_request_non_streaming() {
        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![create_test_message(
                Role::User,
                vec![create_text_part("Hello")],
            )],
            system_prompt: None,
            tools: vec![],
            temperature: None,
            max_tokens: Some(1024),
            reasoning_effort: None,
        };

        let openai_req = build_openai_request(req, None, false);

        assert!(
            !openai_req.stream,
            "Non-streaming request must have stream: false"
        );
    }

    #[test]
    fn test_build_openai_request_streaming() {
        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![create_test_message(
                Role::User,
                vec![create_text_part("Hello")],
            )],
            system_prompt: None,
            tools: vec![],
            temperature: None,
            max_tokens: Some(1024),
            reasoning_effort: None,
        };

        let openai_req = build_openai_request(req, None, true);

        assert!(
            openai_req.stream,
            "Streaming request must have stream: true"
        );
    }

    #[test]
    fn test_build_openai_request_stream_false_explicit() {
        // Verify that stream: false is explicitly set (not just default)
        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![create_test_message(
                Role::User,
                vec![create_text_part("Hello")],
            )],
            system_prompt: None,
            tools: vec![],
            temperature: None,
            max_tokens: Some(1024),
            reasoning_effort: None,
        };

        let openai_req = build_openai_request(req, None, false);

        // Serialize and check JSON output contains "stream":false
        let json = serde_json::to_string(&openai_req).unwrap();
        assert!(
            json.contains(r#""stream":false"#),
            "JSON must contain \"stream\":false, got: {}",
            json
        );
    }

    #[test]
    fn test_build_openai_request_stream_true_explicit() {
        // Verify that stream: true is explicitly set
        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![create_test_message(
                Role::User,
                vec![create_text_part("Hello")],
            )],
            system_prompt: None,
            tools: vec![],
            temperature: None,
            max_tokens: Some(1024),
            reasoning_effort: None,
        };

        let openai_req = build_openai_request(req, None, true);

        // Serialize and check JSON output contains "stream":true
        let json = serde_json::to_string(&openai_req).unwrap();
        assert!(
            json.contains(r#""stream":true"#),
            "JSON must contain \"stream\":true, got: {}",
            json
        );
    }
}
