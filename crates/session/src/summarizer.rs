//! Summary generation for conversation compaction
//!
//! Uses an LLM provider to generate concise summaries of conversation history.

use std::sync::Arc;
use rcode_core::{
    CompletionRequest, Message, Part, Role, error::Result as CoreResult,
};

use crate::compaction::CompactionResult;

/// Summarizer for generating conversation summaries
pub struct Summarizer {
    provider: Arc<dyn rcode_core::LlmProvider>,
    model: String,
}

impl Summarizer {
    /// Create a new summarizer with the given LLM provider
    pub fn new(provider: Arc<dyn rcode_core::LlmProvider>, model: String) -> Self {
        Self { provider, model }
    }

    /// Summarize a set of messages, targeting a specific token count
    pub async fn summarize_messages(
        &self,
        messages: &[Message],
        target_tokens: usize,
        session_id: &str,
    ) -> CoreResult<Message> {
        let summary_prompt = self.create_summary_prompt(messages);
        
        // Create a completion request for summarization
        let request = CompletionRequest {
            model: self.model.clone(),
            messages: vec![Message {
                id: rcode_core::MessageId::new(),
                session_id: session_id.to_string(),
                role: Role::User,
                parts: vec![Part::Text { content: summary_prompt }],
                created_at: chrono::Utc::now(),
            }],
            system_prompt: Some(self.get_system_prompt()),
            tools: vec![],
            temperature: Some(0.3), // Lower temperature for more consistent summaries
            max_tokens: Some((target_tokens / 2) as u32), // Leave room for summary
            reasoning_effort: None,
        };

        let response = self.provider.complete(request).await?;
        
        // Create the summary message
        let summary_message = Message {
            id: rcode_core::MessageId::new(),
            session_id: session_id.to_string(),
            role: Role::Assistant,
            parts: vec![Part::Text { content: response.content }],
            created_at: chrono::Utc::now(),
        };

        Ok(summary_message)
    }

    /// Create a prompt for summarizing messages
    fn create_summary_prompt(&self, messages: &[Message]) -> String {
        let formatted_messages = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                    Role::System => "System",
                };
                let content = self.extract_text_content(m);
                format!("[{}]: {}\n", role, content)
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"The following is a conversation history. Create a concise summary that captures:
1. The main topics discussed
2. Key decisions made
3. Important context for continuing the conversation

CONVERSATION:
{}

SUMMARY:"#,
            formatted_messages
        )
    }

    /// Extract text content from a message's parts
    fn extract_text_content(&self, message: &Message) -> String {
        message
            .parts
            .iter()
            .filter_map(|part| match part {
                Part::Text { content } => Some(content.clone()),
                Part::ToolCall { name, arguments, .. } => {
                    Some(format!("[Tool: {} with args: {}]", name, arguments))
                }
                Part::ToolResult { content, .. } => Some(format!("[Result: {}]", content)),
                Part::Reasoning { content } => Some(format!("[Reasoning: {}]", content)),
                Part::Attachment { name, mime_type, .. } => {
                    Some(format!("[Attachment: {} ({})]", name, mime_type))
                }
                Part::TaskChecklist { items } => Some(format!(
                    "[Checklist: {}]",
                    items
                        .iter()
                        .map(|item| format!("{} ({}, {})", item.content, item.status, item.priority))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// Get the system prompt for summarization
    fn get_system_prompt(&self) -> String {
        r#"You are a conversation summarizer. Your task is to create concise, accurate summaries of conversation histories.

Guidelines:
- Capture the essence of what was discussed, not every detail
- Identify key decisions and outcomes
- Preserve critical context needed to continue the conversation
- Use a neutral, informative tone
- Keep the summary focused and avoid redundancy
- If the conversation is about code, preserve important technical details
"#.to_string()
    }

    /// Compact messages using the Hybrid strategy: summarize older, truncate if still too long
    pub async fn compact_messages(
        &self,
        messages: &[Message],
        max_messages: usize,
        max_tokens: usize,
        session_id: &str,
    ) -> CoreResult<CompactionResult> {
        let original_count = messages.len();
        
        // If we're under the limits, no compaction needed
        if original_count <= max_messages && self.estimate_tokens(messages) <= max_tokens {
            return Err(rcode_core::RCodeError::Session(
                "No compaction needed".to_string()
            ));
        }

        // Keep first 2 messages (system prompt and initial context) and last N messages
        let keep_recent = max_messages.saturating_sub(2);
        let preserve_count = 2 + keep_recent;

        if original_count <= preserve_count {
            // Not enough messages to compact meaningfully
            return Err(rcode_core::RCodeError::Session(
                "Not enough messages to compact".to_string()
            ));
        }

        // Messages to summarize (middle portion)
        let to_summarize = &messages[2..original_count - keep_recent];
        
        if to_summarize.is_empty() {
            return Err(rcode_core::RCodeError::Session(
                "No messages to summarize".to_string()
            ));
        }

        // Generate summary targeting half of max_tokens
        let summary = self.summarize_messages(to_summarize, max_tokens / 2, session_id).await?;
        
        // Build new message list: first 2 + summary + recent
        let mut new_messages = Vec::with_capacity(preserve_count + 1);
        new_messages.push(messages[0].clone());
        new_messages.push(messages[1].clone());
        new_messages.push(summary.clone());
        new_messages.extend_from_slice(&messages[original_count - keep_recent..]);

        // Estimate tokens saved
        let original_tokens = self.estimate_tokens(messages);
        let new_tokens = self.estimate_tokens(&new_messages);
        let tokens_saved = original_tokens.saturating_sub(new_tokens);

        Ok(CompactionResult::new(
            original_count,
            new_messages.len(),
            summary,
            tokens_saved,
        ))
    }

    /// Estimate total tokens in messages
    fn estimate_tokens(&self, messages: &[Message]) -> usize {
        messages.iter().map(|m| self.estimate_message_tokens(m)).sum()
    }

    fn estimate_message_tokens(&self, message: &Message) -> usize {
        let mut count = 0;
        
        count += match message.role {
            Role::User => 4,
            Role::Assistant => 4,
            Role::System => 4,
        };
        
        for part in &message.parts {
            match part {
                Part::Text { content } => count += content.len() / 4,
                Part::ToolCall { name, arguments, .. } => {
                    count += name.len() / 4;
                    count += arguments.to_string().len() / 4;
                }
                Part::ToolResult { content, .. } => count += content.len() / 4,
                Part::Reasoning { content } => count += content.len() / 4,
                Part::Attachment { name, mime_type, .. } => {
                    count += name.len() / 4;
                    count += mime_type.len() / 4;
                    count += 10;
                }
                Part::TaskChecklist { items } => {
                    count += items
                        .iter()
                        .map(|item| (item.content.len() + item.status.len() + item.priority.len()) / 4)
                        .sum::<usize>();
                }
            }
        }
        
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal mock LlmProvider that implements rcode_core::LlmProvider
    struct TestProvider;

    #[async_trait::async_trait]
    impl rcode_core::LlmProvider for TestProvider {
        async fn complete(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::CompletionResponse> {
            Ok(rcode_core::CompletionResponse {
                content: "Summary of conversation: discussed Rust testing.".to_string(),
                reasoning: None,
                tool_calls: vec![],
                usage: rcode_core::TokenUsage { input_tokens: 10, output_tokens: 20, total_tokens: Some(30) },
                stop_reason: rcode_core::provider::StopReason::EndTurn,
            })
        }

        async fn stream(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::StreamingResponse> {
            unimplemented!()
        }

        fn model_info(&self, _model_id: &str) -> Option<rcode_core::ModelInfo> {
            Some(rcode_core::ModelInfo {
                id: "test".into(), name: "Test".into(), provider: "test".into(),
                context_window: 1000, max_output_tokens: Some(100),
            })
        }
    }

    struct ErrorProvider;

    #[async_trait::async_trait]
    impl rcode_core::LlmProvider for ErrorProvider {
        async fn complete(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::CompletionResponse> {
            Err(rcode_core::RCodeError::Provider("LLM down".into()))
        }
        async fn stream(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::StreamingResponse> {
            unimplemented!()
        }
        fn model_info(&self, _model_id: &str) -> Option<rcode_core::ModelInfo> { None }
    }

    fn test_provider() -> Arc<dyn rcode_core::LlmProvider> {
        Arc::new(TestProvider)
    }

    fn error_provider() -> Arc<dyn rcode_core::LlmProvider> {
        Arc::new(ErrorProvider)
    }

    fn make_msg(role: Role, content: &str) -> Message {
        Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".to_string(),
            role,
            parts: vec![Part::Text { content: content.to_string() }],
            created_at: chrono::Utc::now(),
        }
    }

    fn make_tool_call_msg() -> Message {
        Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".to_string(),
            role: Role::Assistant,
            parts: vec![Part::ToolCall {
                id: "tc1".into(),
                name: "bash".into(),
                arguments: Box::new(serde_json::json!({"cmd": "ls"})),
            }],
            created_at: chrono::Utc::now(),
        }
    }

    fn make_tool_result_msg() -> Message {
        Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".to_string(),
            role: Role::User,
            parts: vec![Part::ToolResult {
                tool_call_id: "tc1".into(),
                content: "file1.rs\nfile2.rs".into(),
                is_error: false,
            }],
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_extract_text_content_simple() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = make_msg(Role::User, "Hello world");
        let text = s.extract_text_content(&msg);
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_extract_text_content_tool_call() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = make_tool_call_msg();
        let text = s.extract_text_content(&msg);
        assert!(text.contains("Tool: bash"));
    }

    #[test]
    fn test_extract_text_content_tool_result() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = make_tool_result_msg();
        let text = s.extract_text_content(&msg);
        assert!(text.contains("Result:"));
    }

    #[test]
    fn test_extract_text_content_reasoning() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::Assistant,
            parts: vec![Part::Reasoning { content: "thinking...".into() }],
            created_at: chrono::Utc::now(),
        };
        let text = s.extract_text_content(&msg);
        assert!(text.contains("Reasoning:"));
    }

    #[test]
    fn test_extract_text_content_mixed_parts() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::Assistant,
            parts: vec![
                Part::Text { content: "Check this".into() },
                Part::ToolCall { id: "tc1".into(), name: "grep".into(), arguments: Box::new(serde_json::json!({"q": "test"})) },
            ],
            created_at: chrono::Utc::now(),
        };
        let text = s.extract_text_content(&msg);
        assert!(text.contains("Check this"));
        assert!(text.contains("Tool: grep"));
    }

    #[test]
    fn test_extract_text_content_empty_parts() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::User,
            parts: vec![],
            created_at: chrono::Utc::now(),
        };
        let text = s.extract_text_content(&msg);
        assert_eq!(text, "");
    }

    #[test]
    fn test_estimate_message_tokens() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = make_msg(Role::User, "Hello world");
        let tokens = s.estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_multiple_messages() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msgs = vec![
            make_msg(Role::User, "A"),
            make_msg(Role::Assistant, "B"),
        ];
        let tokens = s.estimate_tokens(&msgs);
        assert!(tokens >= 8); // 2 messages * (4 role overhead + content)
    }

    #[test]
    fn test_estimate_tokens_empty() {
        let s = Summarizer::new(test_provider(), "model".into());
        assert_eq!(s.estimate_tokens(&[]), 0);
    }

    #[tokio::test]
    async fn test_summarize_messages() {
        let s = Summarizer::new(test_provider(), "test-model".into());
        let msgs = vec![
            make_msg(Role::User, "Let's discuss testing"),
            make_msg(Role::Assistant, "Sure, what about it?"),
        ];
        let result = s.summarize_messages(&msgs, 100, "s1").await.unwrap();
        assert_eq!(result.role, Role::Assistant);
        assert!(result.parts.iter().any(|p| matches!(p, Part::Text { content } if content.contains("Summary"))));
    }

    #[tokio::test]
    async fn test_summarize_messages_with_error() {
        let s = Summarizer::new(error_provider(), "model".into());
        let msgs = vec![make_msg(Role::User, "test")];
        let result = s.summarize_messages(&msgs, 100, "s1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_compact_messages_under_limit() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msgs = vec![make_msg(Role::User, "short")];
        let result = s.compact_messages(&msgs, 100, 1000, "s1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_compact_messages_needs_compaction() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msgs: Vec<Message> = (0..20)
            .map(|_i| make_msg(Role::User, &"A".repeat(100)))
            .collect();
        let result = s.compact_messages(&msgs, 5, 50, "s1").await;
        assert!(result.is_ok());
        let compacted = result.unwrap();
        assert!(compacted.new_count < msgs.len());
        assert!(compacted.tokens_saved > 0);
    }

    #[tokio::test]
    async fn test_compact_messages_not_enough_to_compact() {
        let s = Summarizer::new(test_provider(), "model".into());
        // With max_messages=10, preserve_count=10, so 5 messages still <= preserve_count
        // but we need the first check to fail (to reach the second check)
        // First check: original_count <= max_messages AND tokens <= max_tokens
        // We make tokens > max_tokens to fail the first check
        let msgs: Vec<Message> = (0..5)
            .map(|_i| make_msg(Role::User, &"A".repeat(500)))  // Long messages
            .collect();
        // max_tokens=50 is small, so estimate_tokens will exceed it and first check fails
        // But with 5 msgs and preserve_count=10, 5 <= 10 triggers "Not enough messages"
        let result = s.compact_messages(&msgs, 10, 50, "s1").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Not enough messages to compact"));
    }

    #[tokio::test]
    async fn test_compact_messages_empty_to_summarize() {
        let s = Summarizer::new(test_provider(), "model".into());
        // 5 messages: 2 preserved + 3 recent, nothing in middle
        let msgs = vec![
            make_msg(Role::System, "System"),
            make_msg(Role::User, "Hello"),
            make_msg(Role::Assistant, "Hi"),
            make_msg(Role::User, "How are you?"),
            make_msg(Role::Assistant, "Fine thanks"),
        ];
        // max_messages=5 means keep_recent=3, preserve_count=5, so to_summarize is empty
        let result = s.compact_messages(&msgs, 5, 50, "s1").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_create_summary_prompt() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msgs = vec![
            make_msg(Role::User, "Tell me about Rust"),
            make_msg(Role::Assistant, "Rust is a systems language"),
        ];
        let prompt = s.create_summary_prompt(&msgs);
        assert!(prompt.contains("CONVERSATION:"));
        assert!(prompt.contains("SUMMARY:"));
        assert!(prompt.contains("[User]:"));
        assert!(prompt.contains("[Assistant]:"));
    }

    #[test]
    fn test_get_system_prompt() {
        let s = Summarizer::new(test_provider(), "model".into());
        let system = s.get_system_prompt();
        assert!(system.contains("summarizer"));
        assert!(system.contains("concise"));
    }

    #[test]
    fn test_extract_text_content_with_attachment() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::User,
            parts: vec![Part::Attachment {
                id: "att1".into(),
                name: "report.pdf".into(),
                mime_type: "application/pdf".into(),
                content: vec![],
            }],
            created_at: chrono::Utc::now(),
        };
        let text = s.extract_text_content(&msg);
        assert!(text.contains("Attachment:"));
        assert!(text.contains("report.pdf"));
        assert!(text.contains("application/pdf"));
    }

    #[test]
    fn test_create_summary_prompt_with_system_message() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msgs = vec![
            make_msg(Role::System, "You are a helpful assistant"),
            make_msg(Role::User, "Hello"),
            make_msg(Role::Assistant, "Hi there!"),
        ];
        let prompt = s.create_summary_prompt(&msgs);
        // Should contain [System] tag
        assert!(prompt.contains("[System]:"));
        assert!(prompt.contains("[User]:"));
        assert!(prompt.contains("[Assistant]:"));
    }

    #[tokio::test]
    async fn test_compact_messages_exactly_at_limit() {
        let s = Summarizer::new(test_provider(), "model".into());
        // Create messages where token estimate exactly hits boundary
        let msgs: Vec<Message> = (0..5)
            .map(|_i| make_msg(Role::User, &"A".repeat(10)))
            .collect();
        // max_messages=5, max_tokens=10000 should not trigger compaction
        let result = s.compact_messages(&msgs, 5, 10000, "s1").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No compaction needed") || err.to_string().contains("Not enough"));
    }

    #[test]
    fn test_estimate_message_tokens_with_tool_call() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::Assistant,
            parts: vec![Part::ToolCall {
                id: "tc1".into(),
                name: "bash".into(),
                arguments: Box::new(serde_json::json!({"command": "ls -la"})),
            }],
            created_at: chrono::Utc::now(),
        };
        let tokens = s.estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_message_tokens_with_tool_result() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::User,
            parts: vec![Part::ToolResult {
                tool_call_id: "tc1".into(),
                content: "file1.txt\nfile2.txt\nfile3.txt".into(),
                is_error: false,
            }],
            created_at: chrono::Utc::now(),
        };
        let tokens = s.estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_message_tokens_with_reasoning() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::Assistant,
            parts: vec![Part::Reasoning {
                content: "This is a very long reasoning chain that needs to be summarized...".into(),
            }],
            created_at: chrono::Utc::now(),
        };
        let tokens = s.estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_message_tokens_with_attachment() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::User,
            parts: vec![Part::Attachment {
                id: "att1".into(),
                name: "data.csv".into(),
                mime_type: "text/csv".into(),
                content: vec![],
            }],
            created_at: chrono::Utc::now(),
        };
        let tokens = s.estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_extract_text_content_multiple_parts() {
        let s = Summarizer::new(test_provider(), "model".into());
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::Assistant,
            parts: vec![
                Part::Text { content: "First part".into() },
                Part::Text { content: "Second part".into() },
            ],
            created_at: chrono::Utc::now(),
        };
        let text = s.extract_text_content(&msg);
        assert!(text.contains("First part"));
        assert!(text.contains("Second part"));
    }
}
