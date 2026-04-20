//! ZAI provider implementation
//!
//! ZAI (zai-coding) is an OpenAI-compatible API at https://api.zai.chat/v1
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

/// ZAI provider with its own identity, composing OpenAI-compatible transport
pub struct ZaiProvider {
    transport: OpenAiCompatTransport,
}

impl ZaiProvider {
    /// Create a new ZAI provider with the given API key
    pub fn new(api_key: String) -> Self {
        let custom_headers = std::env::var("ZAI_CUSTOM_HEADERS")
            .map(|h| {
                serde_json::from_str::<Vec<(String, String)>>(&h)
                    .unwrap_or_else(|_| vec![])
            })
            .unwrap_or_default();

        let config = OpenAiCompatConfig::new(
            api_key,
            "https://api.zai.chat/v1".to_string(),
            "zai".to_string(),
        )
        .with_custom_headers(custom_headers);

        let transport = OpenAiCompatTransport::new(config);
        Self { transport }
    }

    /// Create a new ZAI provider with a custom base URL
    pub fn new_with_base_url(api_key: String, base_url: String) -> Self {
        let custom_headers = std::env::var("ZAI_CUSTOM_HEADERS")
            .map(|h| {
                serde_json::from_str::<Vec<(String, String)>>(&h)
                    .unwrap_or_else(|_| vec![])
            })
            .unwrap_or_default();

        let config = OpenAiCompatConfig::new(
            api_key,
            base_url,
            "zai".to_string(),
        )
        .with_custom_headers(custom_headers);

        let transport = OpenAiCompatTransport::new(config);
        Self { transport }
    }
}

#[async_trait]
impl LlmProvider for ZaiProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.transport.post(req).await
    }

    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse> {
        self.transport.post_streaming(req).await
    }

    fn model_info(&self, _model_id: &str) -> Option<ModelInfo> {
        // ZAI has multiple models, no static list
        None
    }

    fn provider_id(&self) -> &str {
        "zai"
    }

    fn abort(&self) {
        self.transport.abort()
    }
    
    fn capabilities(&self) -> ProviderCapabilities {
        // ZAI uses OpenAI-compatible API, supports tool calling
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
        let provider = ZaiProvider::new("test-api-key".to_string());
        assert_eq!(provider.provider_id(), "zai");
    }

    #[test]
    fn test_provider_id() {
        let provider = ZaiProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "zai");
    }

    #[test]
    fn test_model_info_returns_none() {
        let provider = ZaiProvider::new("test".to_string());
        assert!(provider.model_info("any-model").is_none());
    }

    #[test]
    fn test_provider_with_custom_base_url() {
        let provider = ZaiProvider::new_with_base_url(
            "test-api-key".to_string(),
            "https://custom.zai.example.com/v1".to_string(),
        );
        assert_eq!(provider.provider_id(), "zai");
    }

    #[test]
    fn test_provider_abort_does_not_panic() {
        let provider = ZaiProvider::new("test".to_string());
        provider.abort();
    }

    #[test]
    fn test_custom_headers_from_env_empty() {
        // Without ZAI_CUSTOM_HEADERS env var, custom_headers should be empty
        let provider = ZaiProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "zai");
    }

    // ============ ZAI Protocol Tests ============

    /// Test that ZAI provider correctly constructs non-streaming requests.
    /// This verifies REQ-ZAI-02: ZAI provider runtime preserves protocol semantics.
    #[test]
    fn test_zai_request_construction() {
        use crate::openai_compat::request::build_openai_request;

        let req = CompletionRequest {
            model: "zai-coding-plan".to_string(),
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
        assert_eq!(body.model, "zai-coding-plan");
        // Verify stream is false for non-streaming
        assert!(!body.stream);
        // Verify message content
        assert_eq!(body.messages.len(), 1);
        assert_eq!(body.messages[0].role, "user");
        let json = serde_json::to_string(&body.messages[0]).unwrap();
        assert!(json.contains(r#""content":"Hello""#));
    }

    /// Test that ZAI provider correctly serializes tool-call requests.
    /// This verifies REQ-ZAI-02: tool-call round-trip semantics.
    #[test]
    fn test_zai_tool_call_request_serialization() {
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
            model: "zai-coding-plan".to_string(),
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

    /// Test that ZAI provider response parsing works correctly.
    /// This verifies REQ-ZAI-02: streaming delta parse.
    #[test]
    fn test_zai_response_parsing() {
        use crate::openai_compat::response::parse_completion_response;

        let json_response = serde_json::json!({
            "id": "zai-123",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello from ZAI"
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
        assert_eq!(response.content, "Hello from ZAI");
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
    }

    /// Test that ZAI provider correctly parses streaming SSE events.
    /// This verifies REQ-ZAI-02: streaming delta parse.
    #[test]
    fn test_zai_streaming_event_parsing() {
        use crate::openai_compat::response::parse_openai_sse_event;

        // Simulate SSE text delta
        let data = r#"{"id":"zai-123","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
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

    /// Test that ZAI provider correctly parses tool-call streaming events.
    /// This verifies REQ-ZAI-02: tool-call round-trip through streaming.
    #[test]
    fn test_zai_tool_call_streaming_event_parsing() {
        use crate::openai_compat::response::parse_openai_sse_event;

        // Simulate SSE tool_call_start delta
        let data = r#"{"id":"zai-123","choices":[{"index":0,"delta":{"tool_calls":[{"id":"call_123","function":{"name":"bash","arguments":""}}]},"finish_reason":null}]}"#;
        let mut tool_call_state = None;
        let event = parse_openai_sse_event(data, &mut tool_call_state);

        assert!(event.is_some());
        let (streaming_event, _finish_reason) = event.unwrap();
        match streaming_event {
            rcode_core::StreamingEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "bash");
            }
            _ => panic!("Expected ToolCallStart event"),
        }
    }

    /// Test that ZAI provider correctly parses non-streaming response with tool_calls.
    /// This verifies REQ-ZAI-02: tool-call round-trip through non-streaming response.
    /// 
    /// The scenario:
    /// - Assistant responds with content AND tool_calls
    /// - Response parser produces CompletionResponse with:
    ///   - content field populated
    ///   - tool_calls field populated with structured ToolCall objects
    #[test]
    fn test_zai_non_streaming_response_with_tool_calls() {
        use crate::openai_compat::response::parse_completion_response;

        // Simulate a non-streaming response where assistant both explains and calls a tool
        let json_response = serde_json::json!({
            "id": "zai-123",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "I'll run that command for you.",
                    "tool_calls": [
                        {
                            "id": "call_abc123",
                            "type": "function",
                            "function": {
                                "name": "bash",
                                "arguments": "{\"cmd\":\"ls -la\"}"
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 15,
                "completion_tokens": 20,
                "total_tokens": 35
            }
        });

        let result = parse_completion_response(json_response);
        assert!(result.is_ok());
        let response = result.unwrap();

        // Verify content is preserved
        assert_eq!(response.content, "I'll run that command for you.");

        // Verify tool_calls are parsed into structured ToolCall objects
        assert!(!response.tool_calls.is_empty());
        assert_eq!(response.tool_calls.len(), 1);
        
        let tool_call = &response.tool_calls[0];
        assert_eq!(tool_call.id, "call_abc123");
        assert_eq!(tool_call.name, "bash");
        
        // Arguments is stored as a JSON Value (could be string or object depending on API response)
        // In this case it's a string representation of JSON: "{\"cmd\":\"ls -la\"}"
        let args_str = tool_call.arguments.as_str().unwrap_or("");
        assert!(args_str.contains("cmd"));
        assert!(args_str.contains("ls -la"));

        // Verify usage
        assert_eq!(response.usage.input_tokens, 15);
        assert_eq!(response.usage.output_tokens, 20);

        // Verify stop_reason indicates tool_calls
        assert_eq!(response.stop_reason, rcode_core::provider::StopReason::EndTurn);
    }

    /// Test that ZAI provider handles streaming tool_call argument accumulation correctly.
    /// This verifies REQ-ZAI-02: incremental arguments are accumulated properly.
    #[test]
    fn test_zai_streaming_tool_call_argument_accumulation() {
        use crate::openai_compat::response::parse_openai_sse_event;

        // Start a tool call
        let start_data = r#"{"id":"zai-123","choices":[{"index":0,"delta":{"tool_calls":[{"id":"call_xyz","function":{"name":"bash","arguments":""}}]},"finish_reason":null}]}"#;
        let mut tool_call_state = None;
        
        let start_event = parse_openai_sse_event(start_data, &mut tool_call_state);
        assert!(start_event.is_some());
        match start_event.unwrap().0 {
            rcode_core::StreamingEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "call_xyz");
                assert_eq!(name, "bash");
            }
            _ => panic!("Expected ToolCallStart"),
        }

        // Receive first chunk of arguments
        let arg_data1 = r#"{"id":"zai-123","choices":[{"index":0,"delta":{"tool_calls":[{"function":{"arguments":"{\"cmd\":"}}]},"finish_reason":null}]}"#;
        let arg_event1 = parse_openai_sse_event(arg_data1, &mut tool_call_state);
        assert!(arg_event1.is_some());
        match arg_event1.unwrap().0 {
            rcode_core::StreamingEvent::ToolCallArg { id, name, value } => {
                assert_eq!(id, "call_xyz");
                assert_eq!(name, "bash");
                // Incremental value
                assert_eq!(value, "{\"cmd\":");
            }
            _ => panic!("Expected ToolCallArg"),
        }

        // Receive second chunk of arguments
        let arg_data2 = r#"{"id":"zai-123","choices":[{"index":0,"delta":{"tool_calls":[{"function":{"arguments":"\"ls -la\"}"}}]},"finish_reason":null}]}"#;
        let arg_event2 = parse_openai_sse_event(arg_data2, &mut tool_call_state);
        assert!(arg_event2.is_some());
        match arg_event2.unwrap().0 {
            rcode_core::StreamingEvent::ToolCallArg { id, name, value } => {
                assert_eq!(id, "call_xyz");
                assert_eq!(name, "bash");
                // Incremental value (not accumulated!)
                assert_eq!(value, "\"ls -la\"}");
            }
            _ => panic!("Expected ToolCallArg"),
        }

        // Verify the accumulated state is correct
        assert!(tool_call_state.is_some());
        let accumulated = tool_call_state.unwrap();
        assert_eq!(accumulated.id, "call_xyz");
        assert_eq!(accumulated.name, "bash");
        assert_eq!(accumulated.arguments, "{\"cmd\":\"ls -la\"}");
    }
}
