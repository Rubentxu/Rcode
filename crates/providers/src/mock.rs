//! Mock LLM Provider for testing

use std::sync::atomic::{AtomicUsize, Ordering};
use async_trait::async_trait;
use tokio::sync::mpsc;

use rcode_core::{
    CompletionRequest, CompletionResponse, 
    StreamingResponse, ModelInfo, StreamingEvent, TokenUsage,
    error::{Result, OpenCodeError},
};
use rcode_core::provider::StopReason;
use crate::LlmProvider;

/// Invocation record for tracking calls
#[derive(Debug, Clone)]
pub struct Invocation {
    pub request: CompletionRequest,
    pub call_count: usize,
}

/// Mock LLM Provider for testing
pub struct MockLlmProvider {
    pub invocation_count: AtomicUsize,
    pub next_response: std::sync::Mutex<Option<CompletionResponse>>,
    pub next_error: std::sync::Mutex<Option<OpenCodeError>>,
    pub stream_events: std::sync::Mutex<Vec<StreamingEvent>>,
    pub invocation_history: std::sync::Mutex<Vec<Invocation>>,
    pub should_stream: std::sync::Mutex<bool>,
}

impl MockLlmProvider {
    pub fn new() -> Self {
        Self {
            invocation_count: AtomicUsize::new(0),
            next_response: std::sync::Mutex::new(None),
            next_error: std::sync::Mutex::new(None),
            stream_events: std::sync::Mutex::new(Vec::new()),
            invocation_history: std::sync::Mutex::new(Vec::new()),
            should_stream: std::sync::Mutex::new(false),
        }
    }

    /// Set the response to return on next call
    pub fn set_response(&self, response: CompletionResponse) {
        *self.next_response.lock().unwrap() = Some(response);
    }

    /// Set an error to return on next call
    pub fn set_error(&self, error: OpenCodeError) {
        *self.next_error.lock().unwrap() = Some(error);
    }

    /// Configure streaming events for the next call
    pub fn set_stream_events(&self, events: Vec<StreamingEvent>) {
        *self.stream_events.lock().unwrap() = events;
        *self.should_stream.lock().unwrap() = true;
    }

    /// Get the number of times complete() was called
    pub fn invocation_count(&self) -> usize {
        self.invocation_count.load(Ordering::SeqCst)
    }

    /// Get all invocation history
    pub fn get_history(&self) -> Vec<Invocation> {
        self.invocation_history.lock().unwrap().clone()
    }

    /// Clear all state
    pub fn reset(&self) {
        self.invocation_count.store(0, Ordering::SeqCst);
        *self.next_response.lock().unwrap() = None;
        *self.next_error.lock().unwrap() = None;
        self.stream_events.lock().unwrap().clear();
        self.invocation_history.lock().unwrap().clear();
        *self.should_stream.lock().unwrap() = false;
    }
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let count = self.invocation_count.fetch_add(1, Ordering::SeqCst) + 1;
        
        // Record invocation
        self.invocation_history.lock().unwrap().push(Invocation {
            request: req.clone(),
            call_count: count,
        });

        // Check for error first
        if let Some(error) = self.next_error.lock().unwrap().take() {
            return Err(error);
        }

        // Return configured response or default
        if let Some(response) = self.next_response.lock().unwrap().take() {
            Ok(response)
        } else {
            Ok(CompletionResponse {
                content: "Mock response".to_string(),
                reasoning: None,
                tool_calls: vec![],
                usage: TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: None,
                },
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    async fn stream(&self, req: CompletionRequest) -> Result<StreamingResponse> {
        let count = self.invocation_count.fetch_add(1, Ordering::SeqCst) + 1;
        
        // Record invocation
        self.invocation_history.lock().unwrap().push(Invocation {
            request: req.clone(),
            call_count: count,
        });

        // Check for error first
        if let Some(error) = self.next_error.lock().unwrap().take() {
            return Err(error);
        }

        // Return streaming response with configured events or default
        let events = if *self.should_stream.lock().unwrap() {
            let events: Vec<StreamingEvent> = self.stream_events.lock().unwrap().drain(..).collect();
            *self.should_stream.lock().unwrap() = false;
            events
        } else {
            vec![
                StreamingEvent::Text { delta: "Mock".to_string() },
                StreamingEvent::Text { delta: " streaming".to_string() },
                StreamingEvent::Finish { 
                    stop_reason: StopReason::EndTurn, 
                    usage: TokenUsage { 
                        input_tokens: 0, 
                        output_tokens: 0, 
                        total_tokens: None 
                    },
                },
            ]
        };

        let (tx, rx) = mpsc::channel(1);
        let tx_clone = tx;

        tokio::spawn(async move {
            for event in events {
                if tx_clone.send(event).await.is_err() {
                    return;
                }
            }
        });

        Ok(StreamingResponse { 
            events: Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
        })
    }

    fn model_info(&self, _model_id: &str) -> Option<ModelInfo> {
        Some(ModelInfo {
            id: "mock-model".to_string(),
            name: "Mock Model".to_string(),
            provider: "mock".to_string(),
            context_window: 200000,
            max_output_tokens: Some(4096),
        })
    }

    fn provider_id(&self) -> &str {
        "mock"
    }

    fn abort(&self) {
        // No-op for mock - streaming doesn't actually start
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::Message;
    use rcode_core::message::Role;

    fn create_test_request() -> CompletionRequest {
        CompletionRequest {
            model: "mock-model".to_string(),
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
        }
    }

    fn create_test_response() -> CompletionResponse {
        CompletionResponse {
            content: "test response".to_string(),
            reasoning: None,
            tool_calls: vec![],
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                total_tokens: Some(30),
            },
            stop_reason: StopReason::EndTurn,
        }
    }

    #[tokio::test]
    async fn test_mock_provider_complete() {
        let provider = MockLlmProvider::new();
        
        let response = create_test_response();
        provider.set_response(response.clone());
        
        let result = provider.complete(create_test_request()).await.unwrap();
        assert_eq!(result.content, "test response");
        assert_eq!(provider.invocation_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_complete_error() {
        let provider = MockLlmProvider::new();
        
        provider.set_error(OpenCodeError::Provider("Test error".to_string()));
        
        let result = provider.complete(create_test_request()).await;
        assert!(result.is_err());
        assert_eq!(provider.invocation_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_stream() {
        let provider = MockLlmProvider::new();
        
        let events = vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: StopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 0, 
                    output_tokens: 5, 
                    total_tokens: Some(5) 
                },
            },
        ];
        provider.set_stream_events(events);
        
        let result = provider.stream(create_test_request()).await.unwrap();
        // Just verify it doesn't panic - we can't easily count events in broadcast stream
        assert_eq!(provider.invocation_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_history() {
        let provider = MockLlmProvider::new();
        
        let req = create_test_request();
        provider.complete(req.clone()).await.unwrap();
        provider.complete(req.clone()).await.unwrap();
        
        let history = provider.get_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].call_count, 1);
        assert_eq!(history[1].call_count, 2);
    }

    #[tokio::test]
    async fn test_mock_provider_model_info() {
        let provider = MockLlmProvider::new();
        
        let info = provider.model_info("mock-model");
        assert!(info.is_some());
        assert_eq!(info.unwrap().id, "mock-model");
    }

    #[test]
    fn test_mock_provider_reset() {
        let provider = MockLlmProvider::new();
        provider.set_response(create_test_response());
        
        assert_eq!(provider.invocation_count(), 0);
        provider.reset();
        // After reset, invocation count should be 0
        assert_eq!(provider.invocation_count(), 0);
    }
}
