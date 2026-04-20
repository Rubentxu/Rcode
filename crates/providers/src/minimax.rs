//! MiniMax provider implementation
//!
//! MiniMax is an OpenAI-compatible API at https://api.minimax.chat/v1
//!
//! This provider has its own identity (provider_id, base_url) and composes
//! the shared `OpenAiCompatTransport` for infrastructure.

use async_trait::async_trait;

use rcode_core::{
    CompletionRequest, CompletionResponse, ModelInfo,
    StreamingResponse, error::Result,
};
use rcode_core::provider::ProviderCapabilities;

use super::openai_compat::{OpenAiCompatConfig, OpenAiCompatTransport};
use super::LlmProvider;

/// MiniMax provider with its own identity, composing OpenAI-compatible transport
pub struct MiniMaxProvider {
    transport: OpenAiCompatTransport,
}

impl MiniMaxProvider {
    /// Create a new MiniMax provider with the given API key
    pub fn new(api_key: String) -> Self {
        let config = OpenAiCompatConfig::new(
            api_key,
            "https://api.minimax.chat/v1".to_string(),
            "minimax".to_string(),
        );
        let transport = OpenAiCompatTransport::new(config);
        Self { transport }
    }

    /// Create a new MiniMax provider with a custom base URL
    pub fn new_with_base_url(api_key: String, base_url: String) -> Self {
        let config = OpenAiCompatConfig::new(
            api_key,
            base_url,
            "minimax".to_string(),
        );
        let transport = OpenAiCompatTransport::new(config);
        Self { transport }
    }
}

#[async_trait]
impl LlmProvider for MiniMaxProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.transport.post(req).await
    }

    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse> {
        self.transport.post_streaming(req).await
    }

    fn model_info(&self, _model_id: &str) -> Option<ModelInfo> {
        // MiniMax has multiple models, no static list
        None
    }

    fn provider_id(&self) -> &str {
        "minimax"
    }

    fn abort(&self) {
        self.transport.abort()
    }
    
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{CompletionRequest, Message, Part, Role, MessageId};
    use chrono::Utc;

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

    // ============ MiniMax Protocol Tests ============

    /// Test that MiniMax provider correctly constructs non-streaming requests.
    /// This verifies REQ-MINI-02: MiniMax runtime preserves protocol semantics.
    #[test]
    fn test_minimax_request_construction() {
        use crate::openai_compat::request::build_openai_request;

        let req = CompletionRequest {
            model: "MiniMax-Text-01".to_string(),
            messages: vec![Message {
                id: MessageId::new(),
                session_id: "test-session".to_string(),
                role: Role::User,
                parts: vec![Part::Text { content: "Hello".to_string() }],
                created_at: Utc::now(),
            }],
            system_prompt: None,
            tools: vec![],
            max_tokens: None,
            temperature: None,
            reasoning_effort: None,
        };

        let body = build_openai_request(req.clone(), None, false);

        // Verify model is set correctly
        assert_eq!(body.model, "MiniMax-Text-01");
        // Verify stream is false for non-streaming
        assert!(!body.stream);
        // Verify message content
        assert_eq!(body.messages.len(), 1);
        assert_eq!(body.messages[0].role, "user");
        let json = serde_json::to_string(&body.messages[0]).unwrap();
        assert!(json.contains(r#""content":"Hello""#));
    }

    /// Test that MiniMax provider correctly serializes tool-call requests.
    /// This verifies REQ-MINI-02: tool-call semantics preserved.
    #[test]
    fn test_minimax_tool_call_request_serialization() {
        use crate::openai_compat::request::build_openai_request;
        use rcode_core::ToolDefinition;

        let tool = ToolDefinition {
            name: "bash".to_string(),
            description: "Run bash command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Command to run"
                    }
                },
                "required": ["command"]
            }),
        };

        let req = CompletionRequest {
            model: "MiniMax-Text-01".to_string(),
            messages: vec![Message {
                id: MessageId::new(),
                session_id: "test-session".to_string(),
                role: Role::User,
                parts: vec![Part::Text { content: "Run pwd".to_string() }],
                created_at: Utc::now(),
            }],
            system_prompt: None,
            tools: vec![tool],
            max_tokens: None,
            temperature: None,
            reasoning_effort: None,
        };

        let body = build_openai_request(req.clone(), None, false);

        // Verify tools are serialized correctly
        assert!(body.tools.is_some());
        let tools = body.tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "bash");
        assert_eq!(tools[0].function.description, "Run bash command");
    }

    /// Test that MiniMax provider response parsing works correctly.
    /// This verifies REQ-MINI-02: streaming delta parse.
    #[test]
    fn test_minimax_response_parsing() {
        use crate::openai_compat::response::parse_completion_response;

        let json_response = serde_json::json!({
            "id": "minimax-123",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello from MiniMax"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });

        let result = parse_completion_response(json_response);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.content, "Hello from MiniMax");
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
    }

    /// Test that MiniMax provider correctly parses streaming SSE events.
    /// This verifies REQ-MINI-02: streaming delta parse.
    #[test]
    fn test_minimax_streaming_event_parsing() {
        use crate::openai_compat::response::parse_openai_sse_event;

        // Simulate SSE text delta
        let data = r#"{"id":"minimax-123","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let mut tool_call_state = None;
        let event = parse_openai_sse_event(data, &mut tool_call_state);

        assert!(event.is_some());
        let (streaming_event, _finish_reason) = event.unwrap();
        match streaming_event {
            rcode_core::StreamingEvent::Text { delta } => {
                assert_eq!(delta, "Hello");
            }
            _ => panic!("Expected Text event"),
        }
    }

    /// Test that MiniMax provider correctly handles multi-turn tool-call request assembly.
    /// This verifies REQ-MINI-02: full tool-call round-trip with history.
    /// 
    /// The scenario:
    /// 1. User asks to run a command
    /// 2. Assistant responds with a tool_call
    /// 3. Tool returns result
    /// 4. Assistant responds again
    /// 
    /// This test proves the request builder correctly serializes:
    /// - Assistant message with tool_call (not tool_calls array in content)
    /// - Tool result message with role="tool" and tool_call_id
    #[test]
    fn test_minimax_multi_turn_tool_call_request() {
        use crate::openai_compat::request::build_openai_request;

        // Create a multi-turn conversation with tool-call history
        let req = CompletionRequest {
            model: "MiniMax-Text-01".to_string(),
            messages: vec![
                // Turn 1: User asks to run command
                Message {
                    id: MessageId::new(),
                    session_id: "test-session".to_string(),
                    role: Role::User,
                    parts: vec![Part::Text { content: "Run pwd".to_string() }],
                    created_at: Utc::now(),
                },
                // Turn 2: Assistant responds with tool_call
                Message {
                    id: MessageId::new(),
                    session_id: "test-session".to_string(),
                    role: Role::Assistant,
                    parts: vec![Part::ToolCall {
                        id: "call_1".to_string(),
                        name: "bash".to_string(),
                        arguments: Box::new(serde_json::json!({"cmd": "pwd"})),
                    }],
                    created_at: Utc::now(),
                },
                // Turn 3: Tool result
                Message {
                    id: MessageId::new(),
                    session_id: "test-session".to_string(),
                    role: Role::User,
                    parts: vec![Part::ToolResult {
                        tool_call_id: "call_1".to_string(),
                        content: "/home/rubentxu".to_string(),
                        is_error: false,
                    }],
                    created_at: Utc::now(),
                },
                // Turn 4: Assistant responds again
                Message {
                    id: MessageId::new(),
                    session_id: "test-session".to_string(),
                    role: Role::Assistant,
                    parts: vec![Part::Text { content: "You're in /home/rubentxu".to_string() }],
                    created_at: Utc::now(),
                },
            ],
            system_prompt: None,
            tools: vec![rcode_core::ToolDefinition {
                name: "bash".to_string(),
                description: "Run bash command".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "cmd": {
                            "type": "string",
                            "description": "Command to run"
                        }
                    },
                    "required": ["cmd"]
                }),
            }],
            max_tokens: None,
            temperature: None,
            reasoning_effort: None,
        };

        let body = build_openai_request(req, None, false);

        // Verify we have 4 messages
        assert_eq!(body.messages.len(), 4);

        // Message 1: User text
        assert_eq!(body.messages[0].role, "user");
        let json0 = serde_json::to_string(&body.messages[0]).unwrap();
        assert!(json0.contains(r#""content":"Run pwd""#));
        assert!(body.messages[0].tool_calls.is_none());

        // Message 2: Assistant with tool_call (should use tool_calls array, not content)
        assert_eq!(body.messages[1].role, "assistant");
        assert!(body.messages[1].content.is_none());
        assert!(body.messages[1].tool_calls.is_some());
        let tool_calls = body.messages[1].tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_1");
        assert_eq!(tool_calls[0].function.name, "bash");
        assert_eq!(tool_calls[0].function.arguments, r#"{"cmd":"pwd"}"#);

        // Message 3: Tool result (should have role="tool", tool_call_id)
        assert_eq!(body.messages[2].role, "tool");
        let json2 = serde_json::to_string(&body.messages[2]).unwrap();
        assert!(json2.contains(r#""content":"/home/rubentxu""#));
        assert_eq!(body.messages[2].tool_call_id, Some("call_1".to_string()));

        // Message 4: Assistant text
        assert_eq!(body.messages[3].role, "assistant");
        let json3 = serde_json::to_string(&body.messages[3]).unwrap();
        assert!(json3.contains(r#""content":"You're in /home/rubentxu""#));

        // Verify tools are included
        assert!(body.tools.is_some());
        let tools = body.tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "bash");
    }
}
