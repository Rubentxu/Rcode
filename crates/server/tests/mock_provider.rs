//! SlowMockProvider - A mock LLM provider with configurable delays for testing
//!
//! This provider enables deterministic testing of:
//! - Abort cancellation mid-stream
//! - Concurrent prompt handling (409 responses)
//! - Permission approval/denial flows
//! - Title generation timing
#![allow(unused_imports, dead_code)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::atomic::AtomicPtr;
use std::ptr;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use rcode_core::{
    CompletionRequest, CompletionResponse, ModelInfo, StreamingEvent,
    StreamingResponse, TokenUsage, error::{Result, RCodeError},
};
use rcode_core::provider::{StopReason, ProviderCapabilities};
use rcode_providers::LlmProvider;

/// Tracks the last abort call
#[derive(Debug, Clone)]
pub struct AbortRecord {
    pub was_called: bool,
    pub call_count: usize,
}

/// SlowMockProvider configuration
#[derive(Debug, Clone)]
pub struct SlowMockConfig {
    /// Delay per token in streaming responses
    pub delay_per_token_ms: u64,
    /// Whether to use tool calls in response
    pub use_tool_calls: bool,
    /// Custom tool call name to return (if use_tool_calls is true)
    pub tool_call_name: Option<String>,
    /// JSON string to use as tool call arguments (default: `{"test":"value"}`)
    pub tool_call_args_json: String,
    /// Only emit tool calls for the first N invocations (0 = unlimited)
    pub tool_calls_max_invocations: usize,
    /// Number of text events before finish
    pub text_event_count: usize,
    /// Text content for each text event
    pub text_content: String,
}

impl Default for SlowMockConfig {
    fn default() -> Self {
        Self {
            delay_per_token_ms: 100,
            use_tool_calls: false,
            tool_call_name: None,
            tool_call_args_json: r#"{"test":"value"}"#.to_string(),
            tool_calls_max_invocations: 0,
            text_event_count: 5,
            text_content: "test token ".to_string(),
        }
    }
}

/// A mock provider with configurable delays for deterministic testing
pub struct SlowMockProvider {
    config: std::sync::Mutex<SlowMockConfig>,
    abort_called: AtomicBool,
    abort_count: AtomicUsize,
    invocation_count: AtomicUsize,
    /// Pointer to self for accessing abort flag from spawned task
    /// This is set during stream() and cleared when stream completes
    stream_abort_ptr: AtomicPtr<AtomicBool>,
}

impl SlowMockProvider {
    pub fn new() -> Self {
        Self {
            config: std::sync::Mutex::new(SlowMockConfig::default()),
            abort_called: AtomicBool::new(false),
            abort_count: AtomicUsize::new(0),
            invocation_count: AtomicUsize::new(0),
            stream_abort_ptr: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Configure the provider with custom settings
    pub fn with_config(config: SlowMockConfig) -> Self {
        Self {
            config: std::sync::Mutex::new(config),
            abort_called: AtomicBool::new(false),
            abort_count: AtomicUsize::new(0),
            invocation_count: AtomicUsize::new(0),
            stream_abort_ptr: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Set the delay per token in milliseconds
    pub fn set_delay_per_token(&self, delay_ms: u64) {
        let mut config = self.config.lock().unwrap();
        config.delay_per_token_ms = delay_ms;
    }

    /// Configure the provider to return tool calls
    pub fn set_tool_calls(&self, enabled: bool, tool_name: Option<String>) {
        let mut config = self.config.lock().unwrap();
        config.use_tool_calls = enabled;
        config.tool_call_name = tool_name;
    }

    /// Configure the JSON arguments sent with the tool call
    pub fn set_tool_call_args(&self, args_json: String) {
        let mut config = self.config.lock().unwrap();
        config.tool_call_args_json = args_json;
    }

    /// Only emit tool calls for the first N invocations (0 = unlimited)
    pub fn set_tool_calls_max_invocations(&self, max: usize) {
        let mut config = self.config.lock().unwrap();
        config.tool_calls_max_invocations = max;
    }

    /// Configure text events
    pub fn set_text_events(&self, count: usize, content: String) {
        let mut config = self.config.lock().unwrap();
        config.text_event_count = count;
        config.text_content = content;
    }

    /// Check if abort() was called
    pub fn was_aborted(&self) -> bool {
        self.abort_called.load(Ordering::SeqCst)
    }

    /// Get abort call count
    pub fn abort_count(&self) -> usize {
        self.abort_count.load(Ordering::SeqCst)
    }

    /// Get invocation count
    pub fn invocation_count(&self) -> usize {
        self.invocation_count.load(Ordering::SeqCst)
    }

    /// Reset all state
    pub fn reset(&self) {
        self.abort_called.store(false, Ordering::SeqCst);
        self.abort_count.store(0, Ordering::SeqCst);
        self.invocation_count.store(0, Ordering::SeqCst);
        self.stream_abort_ptr.store(ptr::null_mut(), Ordering::SeqCst);
    }

    /// Get current configuration
    pub fn get_config(&self) -> SlowMockConfig {
        self.config.lock().unwrap().clone()
    }

    /// Internal: Check if current stream should abort
    fn is_current_stream_aborted(&self) -> bool {
        let ptr = self.stream_abort_ptr.load(Ordering::SeqCst);
        if ptr.is_null() {
            // No active stream, check general abort flag
            self.abort_called.load(Ordering::SeqCst)
        } else {
            // Safety: dereference the atomic bool
            // This is safe because we only set it to point to abort_called
            unsafe { (*ptr).load(Ordering::SeqCst) }
        }
    }
}

impl Default for SlowMockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for SlowMockProvider {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse> {
        self.invocation_count.fetch_add(1, Ordering::SeqCst);
        
        let config = self.config.lock().unwrap().clone();
        
        Ok(CompletionResponse {
            content: "Mock complete response".to_string(),
            reasoning: None,
            tool_calls: if config.use_tool_calls {
                vec![rcode_core::ToolCall {
                    id: "tool_1".to_string(),
                    name: config.tool_call_name.unwrap_or_else(|| "test_tool".to_string()),
                    arguments: serde_json::json!({}),
                }]
            } else {
                vec![]
            },
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                total_tokens: Some(30),
            },
            stop_reason: StopReason::EndTurn,
        })
    }

    async fn stream(&self, _req: CompletionRequest) -> Result<StreamingResponse> {
        let invocation = self.invocation_count.fetch_add(1, Ordering::SeqCst) + 1;
        let config = self.config.lock().unwrap().clone();
        let delay = Duration::from_millis(config.delay_per_token_ms);
        
        let (tx, rx) = mpsc::channel(1);
        let tx_clone = tx;
        
        // Spawn streaming task
        tokio::spawn(async move {
            // Send text events with delay
            for i in 0..config.text_event_count {
                if tx_clone.send(StreamingEvent::Text { 
                    delta: format!("{}{}", config.text_content, i)
                }).await.is_err() {
                    return;
                }
                sleep(delay).await;
            }
            
            // Send tool call if configured
            let emit_tool_call = config.use_tool_calls && (
                config.tool_calls_max_invocations == 0 || invocation <= config.tool_calls_max_invocations
            );
            if emit_tool_call {
                let tool_name = config.tool_call_name.unwrap_or_else(|| "test_tool".to_string());
                let tool_id = format!("tool_{}", invocation);
                
                if tx_clone.send(StreamingEvent::ToolCallStart {
                    id: tool_id.clone(),
                    name: tool_name.clone(),
                }).await.is_err() {
                    return;
                }
                sleep(delay).await;
                
                if tx_clone.send(StreamingEvent::ToolCallArg {
                    id: tool_id.clone(),
                    name: "input".to_string(),
                    value: config.tool_call_args_json.clone(),
                }).await.is_err() {
                    return;
                }
                sleep(delay).await;
                
                if tx_clone.send(StreamingEvent::ToolCallEnd {
                    id: tool_id,
                }).await.is_err() {
                    return;
                }
                sleep(delay).await;
            }
            
            // Send finish
            let _ = tx_clone.send(StreamingEvent::Finish {
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: config.text_event_count as u32,
                    total_tokens: Some(10 + config.text_event_count as u32),
                },
            }).await;
        });

        Ok(StreamingResponse {
            events: Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
        })
    }

    fn model_info(&self, _model_id: &str) -> Option<ModelInfo> {
        Some(ModelInfo {
            id: "slow-mock-model".to_string(),
            name: "Slow Mock Model".to_string(),
            provider: "slowmock".to_string(),
            context_window: 200000,
            max_output_tokens: Some(4096),
        })
    }

    fn provider_id(&self) -> &str {
        "slowmock"
    }

    fn abort(&self) {
        self.abort_called.store(true, Ordering::SeqCst);
        self.abort_count.fetch_add(1, Ordering::SeqCst);
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::Message;
    use rcode_core::message::Role;

    fn create_test_request() -> CompletionRequest {
        CompletionRequest {
            model: "slowmock/test".to_string(),
            messages: vec![Message {
                id: rcode_core::MessageId("msg1".to_string()),
                session_id: "session1".to_string(),
                role: Role::User,
                parts: vec![],
                created_at: chrono::Utc::now(),
            }],
            system_prompt: None,
            tools: vec![],
            temperature: None,
            max_tokens: Some(100),
            reasoning_effort: None,
        }
    }

    #[tokio::test]
    async fn test_slow_mock_provider_stream_with_delay() {
        let provider = SlowMockProvider::new();
        provider.set_delay_per_token(50);
        provider.set_text_events(3, "token".to_string());
        
        let response = provider.stream(create_test_request()).await.unwrap();
        use tokio_stream::StreamExt;
        let events: Vec<_> = response.events.collect().await;
        
        // 3 text events + 1 finish event = 4 total
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], StreamingEvent::Text { .. }));
        assert!(matches!(events[3], StreamingEvent::Finish { .. }));
    }

    #[tokio::test]
    async fn test_slow_mock_provider_abort_tracking() {
        let provider = SlowMockProvider::new();
        
        assert!(!provider.was_aborted());
        assert_eq!(provider.abort_count(), 0);
        
        provider.abort();
        
        assert!(provider.was_aborted());
        assert_eq!(provider.abort_count(), 1);
    }

    #[tokio::test]
    async fn test_slow_mock_provider_invocation_count() {
        let provider = SlowMockProvider::new();
        
        assert_eq!(provider.invocation_count(), 0);
        
        provider.complete(create_test_request()).await.unwrap();
        assert_eq!(provider.invocation_count(), 1);
        
        provider.stream(create_test_request()).await.unwrap();
        assert_eq!(provider.invocation_count(), 2);
    }

    #[tokio::test]
    async fn test_slow_mock_provider_with_tool_calls() {
        let provider = SlowMockProvider::new();
        provider.set_tool_calls(true, Some("execute_command".to_string()));
        
        let response = provider.complete(create_test_request()).await.unwrap();
        assert!(!response.tool_calls.is_empty());
        assert_eq!(response.tool_calls[0].name, "execute_command");
    }

    #[tokio::test]
    async fn test_slow_mock_provider_reset() {
        let provider = SlowMockProvider::new();
        
        provider.abort();
        provider.complete(create_test_request()).await.unwrap();
        
        assert!(provider.was_aborted());
        assert_eq!(provider.invocation_count(), 1);
        
        provider.reset();
        
        assert!(!provider.was_aborted());
        assert_eq!(provider.invocation_count(), 0);
        assert_eq!(provider.abort_count(), 0);
    }

    #[tokio::test]
    async fn test_slow_mock_provider_default_response() {
        let provider = SlowMockProvider::new();
        
        let response = provider.complete(create_test_request()).await.unwrap();
        assert_eq!(response.content, "Mock complete response");
        assert!(response.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn test_slow_mock_provider_model_info() {
        let provider = SlowMockProvider::new();
        
        let info = provider.model_info("any-model");
        assert!(info.is_some());
        assert_eq!(info.unwrap().provider, "slowmock");
    }
}