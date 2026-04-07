//! OpenAI-compatible transport layer
//!
//! This module provides the `OpenAiCompatTransport` struct that handles
//! HTTP communication with OpenAI-compatible APIs.

use reqwest::Client;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use rcode_core::{
    CompletionRequest, CompletionResponse,
    StreamingEvent, StreamingResponse,
    TokenUsage, error::Result,
};
use rcode_core::provider::StopReason;

use super::config::OpenAiCompatConfig;
use super::request as request;
use super::response as response;
use crate::rate_limit::TokenBucket;

/// Transport layer for OpenAI-compatible APIs.
/// Handles HTTP communication, auth headers, rate limiting, and streaming.
pub struct OpenAiCompatTransport {
    http_client: Client,
    config: OpenAiCompatConfig,
    rate_limiter: Option<Arc<TokenBucket>>,
    /// Per-stream cancellation token. Each call to stream() gets a new token.
    /// When abort() is called, it cancels the current token and clears it.
    /// Uses std::sync::Mutex because abort() is synchronous (no async context).
    active_token: Arc<StdMutex<Option<CancellationToken>>>,
}

impl OpenAiCompatTransport {
    /// Create a new transport with the given config
    pub fn new(config: OpenAiCompatConfig) -> Self {
        Self {
            http_client: Client::new(),
            config,
            rate_limiter: None,
            active_token: Arc::new(StdMutex::new(None)),
        }
    }

    /// Attach rate limiting to this transport
    pub fn with_rate_limit(mut self, capacity: u64, refill_rate: f64) -> Self {
        self.rate_limiter = Some(Arc::new(TokenBucket::new(capacity, refill_rate)));
        self
    }

    /// Build the chat completions URL with proper base_url normalization
    pub fn chat_completions_url(&self) -> String {
        let trimmed = self.config.base_url.trim_end_matches('/');
        if trimmed.ends_with("/v1") {
            format!("{trimmed}/chat/completions")
        } else {
            format!("{trimmed}/v1/chat/completions")
        }
    }

    /// Send a non-streaming completion request
    pub async fn post(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let body = request::build_openai_request(req.clone(), req.system_prompt.clone());
        
        let url = self.chat_completions_url();
        let mut request_builder = self.http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json");
        
        for (key, value) in &self.config.custom_headers {
            request_builder = request_builder.header(key, value);
        }
        
        let response = request_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::RCodeError::Provider(format!("Network error: {}", e)))?;
        
        // Check HTTP status
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(rcode_core::RCodeError::Provider(
                format!("OpenAI API error ({}): {}", status, error_text)
            ));
        }
        
        let openai_resp: serde_json::Value = response.json()
            .await
            .map_err(|e| rcode_core::RCodeError::Provider(format!("Failed to parse response: {}", e)))?;
        
        response::parse_completion_response(openai_resp)
    }

    /// Send a streaming completion request
    /// Returns the streaming response and a cancellation token for abort
    pub async fn post_streaming(&self, req: CompletionRequest) -> Result<StreamingResponse> {
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

        let body = request::build_openai_request(req.clone(), req.system_prompt.clone());
        
        let url = self.chat_completions_url();
        let mut request_builder = self.http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json");
        
        for (key, value) in &self.config.custom_headers {
            request_builder = request_builder.header(key, value);
        }
        
        let response = request_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::RCodeError::Provider(format!("Network error: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(rcode_core::RCodeError::Provider(
                format!("OpenAI API error ({}): {}", status, error_text)
            ));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();

        if !content_type.contains("text/event-stream") {
            let payload = response.text().await
                .map_err(|e| rcode_core::RCodeError::Provider(format!("Failed to read response body: {}", e)))?;
            return response::streaming_response_from_json_payload(&payload);
        }

        let (tx, rx) = mpsc::channel(1);
        let tx_clone = tx;
        let active_token = Arc::clone(&self.active_token);

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_tool_call: Option<response::OpenAIToolCall> = None;
            let mut last_finish_reason: Option<String> = None;
            let mut stream_error: Option<String> = None;
            let mut stream_done = false;
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

                                    if line.is_empty() {
                                        continue;
                                    }

                                    if line == "data: [DONE]" {
                                        stream_done = true;
                                        break;
                                    }

                                    if let Some(data) = line.strip_prefix("data: ") {
                                        if let Some((event, finish_reason)) = response::parse_openai_sse_event(data, &mut current_tool_call) {
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

                                if stream_done {
                                    break;
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

    /// Abort any in-progress streaming request
    pub fn abort(&self) {
        let mut guard = match self.active_token.lock() {
            Ok(guard) => guard,
            Err(_) => return, // Could not acquire lock, stream is likely ending
        };
        if let Some(token) = guard.take() {
            token.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_completions_url_no_v1() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.openai.com".to_string(),
            "openai".to_string(),
        );
        let transport = OpenAiCompatTransport::new(config);
        assert_eq!(transport.chat_completions_url(), "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_chat_completions_url_with_trailing_slash() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.openai.com/".to_string(),
            "openai".to_string(),
        );
        let transport = OpenAiCompatTransport::new(config);
        assert_eq!(transport.chat_completions_url(), "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_chat_completions_url_with_v1() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.minimax.chat/v1".to_string(),
            "minimax".to_string(),
        );
        let transport = OpenAiCompatTransport::new(config);
        assert_eq!(transport.chat_completions_url(), "https://api.minimax.chat/v1/chat/completions");
    }

    #[test]
    fn test_chat_completions_url_with_v1_trailing_slash() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.minimax.chat/v1/".to_string(),
            "minimax".to_string(),
        );
        let transport = OpenAiCompatTransport::new(config);
        assert_eq!(transport.chat_completions_url(), "https://api.minimax.chat/v1/chat/completions");
    }

    #[test]
    fn test_chat_completions_url_custom_path() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://openrouter.ai".to_string(),
            "openrouter".to_string(),
        );
        let transport = OpenAiCompatTransport::new(config);
        assert_eq!(transport.chat_completions_url(), "https://openrouter.ai/v1/chat/completions");
    }

    #[test]
    fn test_transport_with_rate_limit() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.openai.com".to_string(),
            "openai".to_string(),
        );
        let _transport = OpenAiCompatTransport::new(config)
            .with_rate_limit(100, 10.0);
        // Just verify it compiles and doesn't panic
    }

    #[test]
    fn test_transport_clone_is_independent() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.openai.com".to_string(),
            "openai".to_string(),
        );
        let transport1 = OpenAiCompatTransport::new(config);
        let transport2 = OpenAiCompatTransport::new(OpenAiCompatConfig::new(
            "other-key".to_string(),
            "https://other.api.com".to_string(),
            "other".to_string(),
        ));
        
        // Different configs should be independent
        assert_ne!(transport1.chat_completions_url(), transport2.chat_completions_url());
    }

    #[tokio::test]
    async fn test_abort_when_no_active_stream() {
        let config = OpenAiCompatConfig::new(
            "test-key".to_string(),
            "https://api.openai.com".to_string(),
            "openai".to_string(),
        );
        let transport = OpenAiCompatTransport::new(config);
        
        // abort() should not panic even with no active stream
        transport.abort();
    }
}
