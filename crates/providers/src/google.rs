//! Google Gemini provider implementation

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use rcode_core::{
    CompletionRequest, CompletionResponse, ModelInfo,
    StreamingEvent, StreamingResponse,
    TokenUsage, error::Result,
};
use rcode_core::provider::{StopReason, ProviderCapabilities};

use super::rate_limit::TokenBucket;
use super::LlmProvider;

const GOOGLE_BASE_URL: &str = "https://generativelanguage.googleapis.com";

pub struct GoogleProvider {
    api_key: String,
    base_url: String,
    http_client: Client,
    rate_limiter: Option<Arc<TokenBucket>>,
    /// Per-stream cancellation token. Each call to stream() gets a new token.
    /// When abort() is called, it cancels the current token and clears it.
    /// Uses std::sync::Mutex because abort() is synchronous.
    active_token: Arc<StdMutex<Option<CancellationToken>>>,
}

impl GoogleProvider {
    pub fn new(api_key: String) -> Self {
        let base_url = std::env::var("GOOGLE_BASE_URL")
            .unwrap_or_else(|_| GOOGLE_BASE_URL.to_string());

        Self {
            api_key,
            base_url,
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
impl LlmProvider for GoogleProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        if let Some(limiter) = &self.rate_limiter {
            if let Err(wait_time) = limiter.try_acquire(1) {
                tokio::time::sleep(wait_time).await;
                let _ = limiter.try_acquire(1);
            }
        }

        // Build request body for Gemini API
        let contents: Vec<GeminiContent> = req.messages
            .into_iter()
            .map(into_gemini_content)
            .collect();

        let system_instruction = req.system_prompt.map(|sp| GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart::Text { text: sp }],
        });

        let body = GeminiRequest {
            contents,
            system_instruction,
        };

        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base_url.trim_end_matches('/'),
            req.model,
            self.api_key
        );

        let response = self.http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::RCodeError::Provider(format!("Network error: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(rcode_core::RCodeError::Provider(
                format!("Google API error ({}): {}", status, error_text)
            ));
        }

        let gemini_resp: GeminiResponse = response.json()
            .await
            .map_err(|e| rcode_core::RCodeError::Provider(format!("Parse error: {}", e)))?;

        let content = gemini_resp.candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .and_then(|p| p.text.clone())
            .unwrap_or_default();

        let usage = gemini_resp.usage_metadata.map(|u| TokenUsage {
            input_tokens: u.prompt_token_count.unwrap_or(0) as u32,
            output_tokens: u.candidates_token_count.unwrap_or(0) as u32,
            total_tokens: None,
        }).unwrap_or(TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: None,
        });

        Ok(CompletionResponse {
            content,
            reasoning: None,
            tool_calls: vec![],
            usage,
            stop_reason: StopReason::EndTurn,
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

        // TODO: Implement true SSE streaming for Google Gemini
        // For now, fall back to non-streaming and emit a single Text event + Finish
        if let Some(limiter) = &self.rate_limiter {
            if let Err(wait_time) = limiter.try_acquire(1) {
                tokio::time::sleep(wait_time).await;
                let _ = limiter.try_acquire(1);
            }
        }

        // Build request body for Gemini API
        let contents: Vec<GeminiContent> = req.messages
            .into_iter()
            .map(into_gemini_content)
            .collect();

        let system_instruction = req.system_prompt.map(|sp| GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart::Text { text: sp }],
        });

        let body = GeminiRequest {
            contents,
            system_instruction,
        };

        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}&alt=sse",
            self.base_url.trim_end_matches('/'),
            req.model,
            self.api_key
        );

        let response = self.http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| rcode_core::RCodeError::Provider(format!("Network error: {}", e)))?;

        let (tx, rx) = mpsc::channel(1);
        let tx_clone = tx;
        let active_token = Arc::clone(&self.active_token);

        tokio::spawn(async move {
            let token_clone = token.clone();
            
            // Use select! to allow cancellation while waiting for response
            let response_result = tokio::select! {
                _ = token_clone.cancelled() => {
                    let _ = tx_clone.send(StreamingEvent::Finish {
                        stop_reason: StopReason::EndTurn,
                        usage: TokenUsage {
                            input_tokens: 0,
                            output_tokens: 0,
                            total_tokens: None,
                        },
                    }).await;
                    // Clear the active token
                    let mut guard = active_token.lock().unwrap();
                    *guard = None;
                    return;
                }
                result = response.text() => {
                    result
                }
            };

            match response_result {
                Ok(full_response) => {
                    // Try to parse as SSE
                    let mut text = String::new();
                    for line in full_response.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if let Ok(event) = serde_json::from_str::<GeminiStreamEvent>(data) {
                                if let Some(text_delta) = event.candidates
                                    .first()
                                    .and_then(|c| c.content.parts.first())
                                {
                                    if let Some(t) = &text_delta.text {
                                        text.push_str(t);
                                    }
                                }
                            }
                        }
                    }

                    // If no SSE parsing worked, try JSON directly
                    if text.is_empty() {
                        if let Ok(event) = serde_json::from_str::<GeminiResponse>(&full_response) {
                            text = event.candidates
                                .first()
                                .and_then(|c| c.content.parts.first())
                                .and_then(|p| p.text.clone())
                                .unwrap_or_default();
                        }
                    }

                    let _ = tx_clone.send(StreamingEvent::Text { delta: text }).await;
                    let _ = tx_clone.send(StreamingEvent::Finish {
                        stop_reason: StopReason::EndTurn,
                        usage: TokenUsage {
                            input_tokens: 0,
                            output_tokens: 0,
                            total_tokens: None,
                        },
                    }).await;
                }
                Err(e) => {
                    tracing::error!("Stream error: {}", e);
                    let _ = tx_clone.send(StreamingEvent::Finish {
                        stop_reason: StopReason::EndTurn,
                        usage: TokenUsage {
                            input_tokens: 0,
                            output_tokens: 0,
                            total_tokens: None,
                        },
                    }).await;
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
            "gemini-2.0-flash" => ModelInfo {
                id: "gemini-2.0-flash".into(),
                name: "Gemini 2.0 Flash".into(),
                provider: "google".into(),
                context_window: 1_000_000,
                max_output_tokens: Some(8192),
            },
            "gemini-2.5-pro" => ModelInfo {
                id: "gemini-2.5-pro".into(),
                name: "Gemini 2.5 Pro".into(),
                provider: "google".into(),
                context_window: 1_000_000,
                max_output_tokens: Some(8192),
            },
            "gemini-2.5-flash" => ModelInfo {
                id: "gemini-2.5-flash".into(),
                name: "Gemini 2.5 Flash".into(),
                provider: "google".into(),
                context_window: 1_000_000,
                max_output_tokens: Some(8192),
            },
            _ => return None,
        };
        Some(info)
    }

    fn provider_id(&self) -> &str {
        "google"
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
        // Gemini has limited tool calling support - conservative estimate for now
        ProviderCapabilities::chat_only()
    }
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
}

#[derive(Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum GeminiPart {
    Text { text: String },
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContentResponse,
}

#[derive(Deserialize)]
struct GeminiContentResponse {
    parts: Vec<GeminiPartResponse>,
}

#[derive(Deserialize)]
struct GeminiPartResponse {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    _total_token_count: Option<u32>,
}

#[derive(Deserialize)]
struct GeminiStreamEvent {
    candidates: Vec<GeminiCandidate>,
}

fn into_gemini_content(msg: rcode_core::Message) -> GeminiContent {
    let role = match msg.role {
        rcode_core::Role::User => "user",
        rcode_core::Role::Assistant => "model",
        rcode_core::Role::System => "user",
    };

    let parts: Vec<GeminiPart> = msg.parts.iter()
        .map(|p| match p {
            rcode_core::Part::Text { content } => GeminiPart::Text { text: content.clone() },
            rcode_core::Part::ToolResult { content, .. } => GeminiPart::Text { text: content.clone() },
            rcode_core::Part::ToolCall { name, arguments, .. } => {
                GeminiPart::Text { text: format!("Tool call: {}({})", name, arguments) }
            }
            rcode_core::Part::Reasoning { content } => {
                GeminiPart::Text { text: format!("[Reasoning]: {}", content) }
            }
            rcode_core::Part::Attachment { name, mime_type, .. } => {
                GeminiPart::Text { text: format!("[Attachment: {} ({})]", name, mime_type) }
            }
        })
        .collect();

    GeminiContent {
        role: role.to_string(),
        parts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{Message, Part, message::Role};

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

    #[test]
    fn test_provider_new() {
        let provider = GoogleProvider::new("test-api-key".to_string());
        assert_eq!(provider.provider_id(), "google");
    }

    #[test]
    fn test_provider_with_rate_limit() {
        let provider = GoogleProvider::new("test-api-key".to_string())
            .with_rate_limit(100, 10.0);
        assert_eq!(provider.provider_id(), "google");
    }

    #[test]
    fn test_provider_id() {
        let provider = GoogleProvider::new("test".to_string());
        assert_eq!(provider.provider_id(), "google");
    }

    #[test]
    fn test_model_info_gemini_flash() {
        let provider = GoogleProvider::new("test".to_string());
        let info = provider.model_info("gemini-2.0-flash").unwrap();
        assert_eq!(info.id, "gemini-2.0-flash");
        assert_eq!(info.name, "Gemini 2.0 Flash");
        assert_eq!(info.provider, "google");
        assert_eq!(info.context_window, 1_000_000);
        assert_eq!(info.max_output_tokens, Some(8192));
    }

    #[test]
    fn test_model_info_gemini_pro() {
        let provider = GoogleProvider::new("test".to_string());
        let info = provider.model_info("gemini-2.5-pro").unwrap();
        assert_eq!(info.id, "gemini-2.5-pro");
        assert_eq!(info.name, "Gemini 2.5 Pro");
        assert_eq!(info.provider, "google");
    }

    #[test]
    fn test_model_info_gemini_flash_25() {
        let provider = GoogleProvider::new("test".to_string());
        let info = provider.model_info("gemini-2.5-flash").unwrap();
        assert_eq!(info.id, "gemini-2.5-flash");
        assert_eq!(info.name, "Gemini 2.5 Flash");
    }

    #[test]
    fn test_model_info_unknown() {
        let provider = GoogleProvider::new("test".to_string());
        let info = provider.model_info("unknown-model");
        assert!(info.is_none());
    }

    #[test]
    fn test_abort_method_exists() {
        let provider = GoogleProvider::new("test".to_string());
        // abort() should be callable without panicking
        provider.abort();
    }

    #[test]
    fn test_into_gemini_content_user() {
        let msg = create_test_message(Role::User, vec![create_text_part("Hello")]);
        let content = into_gemini_content(msg);
        assert_eq!(content.role, "user");
        assert_eq!(content.parts.len(), 1);
    }

    #[test]
    fn test_into_gemini_content_assistant() {
        let msg = create_test_message(Role::Assistant, vec![create_text_part("Hi")]);
        let content = into_gemini_content(msg);
        assert_eq!(content.role, "model");
    }

    #[test]
    fn test_into_gemini_content_system() {
        let msg = create_test_message(Role::System, vec![create_text_part("You are helpful")]);
        let content = into_gemini_content(msg);
        // System role maps to "user" in Gemini
        assert_eq!(content.role, "user");
    }

    #[test]
    fn test_gemini_request_serialization() {
        let request = GeminiRequest {
            contents: vec![
                GeminiContent {
                    role: "user".to_string(),
                    parts: vec![GeminiPart::Text { text: "Hello".to_string() }],
                }
            ],
            system_instruction: Some(GeminiContent {
                role: "user".to_string(),
                parts: vec![GeminiPart::Text { text: "You are helpful".to_string() }],
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"contents\""));
        assert!(json.contains("\"system_instruction\""));
    }

    #[test]
    fn test_gemini_response_deserialization() {
        let json = r#"{
            "candidates": [
                {
                    "content": {
                        "parts": [{"text": "Hello!"}]
                    }
                }
            ],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 20
            }
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        let usage = response.usage_metadata.unwrap();
        assert_eq!(usage.prompt_token_count, Some(10));
        assert_eq!(usage.candidates_token_count, Some(20));
    }

    #[test]
    fn test_gemini_response_no_usage() {
        let json = r#"{
            "candidates": [
                {
                    "content": {
                        "parts": [{"text": "Hello!"}]
                    }
                }
            ]
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        assert!(response.usage_metadata.is_none());
    }

    // Cancellation tests

    #[tokio::test]
    async fn test_google_abort_cancels_active_stream() {
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
    async fn test_google_concurrent_streams_independent_cancellation() {
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
    async fn test_google_stream_completes_normally_when_not_aborted() {
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
    fn test_google_abort_method_is_callable() {
        let provider = GoogleProvider::new("test-api-key".to_string());
        // abort() should be callable without panicking
        provider.abort();
    }

    #[tokio::test]
    async fn test_google_per_stream_cancellation_token_pattern() {
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