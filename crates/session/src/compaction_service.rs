//! Context compaction service for handling long conversations
//!
//! This module provides the main CompactionService that coordinates
//! trigger checking, summarization, and message compaction.

use std::sync::Arc;

use rcode_core::{Message, Result as CoreResult};

use crate::compaction::{CompactionConfig, CompactionResult, CompactionStrategy};
use crate::summarizer::Summarizer;

/// Service responsible for coordinating context compaction
pub struct CompactionService {
    trigger: Arc<CompactionTrigger>,
    summarizer: Arc<Summarizer>,
    strategy: CompactionStrategy,
    max_tokens: usize,
}

/// Trigger for determining when compaction should occur
pub struct CompactionTrigger {
    config: CompactionConfig,
}

impl CompactionTrigger {
    /// Create a new compaction trigger with the given configuration
    pub fn new(config: CompactionConfig) -> Self {
        Self { config }
    }

    /// Check if compaction is needed based on message count and token estimates
    pub fn should_compact(&self, messages: &[Message]) -> bool {
        // Check if we have enough messages to make compaction worthwhile
        if messages.len() < self.config.min_messages_to_compact {
            return false;
        }

        // Check message count limit
        if messages.len() > self.config.max_messages {
            return true;
        }

        // Check token threshold
        let total_tokens = estimate_tokens(messages);
        if total_tokens > self.config.token_threshold() {
            return true;
        }

        false
    }

    /// Get the current configuration
    pub fn config(&self) -> &CompactionConfig {
        &self.config
    }
}

impl Default for CompactionTrigger {
    fn default() -> Self {
        Self::new(CompactionConfig::default())
    }
}

impl CompactionService {
    /// Create a new compaction service
    pub fn new(
        summarizer: Arc<Summarizer>,
        config: CompactionConfig,
        strategy: CompactionStrategy,
    ) -> Self {
        let trigger = Arc::new(CompactionTrigger::new(config.clone()));
        Self {
            trigger,
            summarizer,
            strategy,
            max_tokens: config.max_tokens,
        }
    }

    /// Create with explicit trigger
    pub fn with_trigger(
        trigger: Arc<CompactionTrigger>,
        summarizer: Arc<Summarizer>,
        strategy: CompactionStrategy,
    ) -> Self {
        let config = trigger.config().clone();
        Self {
            trigger,
            summarizer,
            strategy,
            max_tokens: config.max_tokens,
        }
    }

    /// Check if compaction should be performed
    pub fn should_compact(&self, messages: &[Message]) -> bool {
        self.trigger.should_compact(messages)
    }

    /// Perform compaction if needed, returning the result if successful
    pub async fn maybe_compact(
        &self,
        messages: &[Message],
        session_id: &str,
    ) -> CoreResult<Option<CompactionResult>> {
        if !self.should_compact(messages) {
            return Ok(None);
        }

        let original_count = messages.len();
        let original_tokens = estimate_tokens(messages);

        // Apply strategy to determine which messages to preserve
        let (preserved, to_summarize) = self.strategy.apply(messages);

        // If nothing to summarize, we're done
        if to_summarize.is_empty() {
            return Ok(None);
        }

        // Generate summary for the messages being compacted
        let summary = self
            .summarizer
            .summarize_messages(&to_summarize, self.max_tokens / 2, session_id)
            .await?;

        // Build new message list: preserved + summary
        let mut new_messages = preserved;
        new_messages.push(summary.clone());

        let new_tokens = estimate_tokens(&new_messages);

        let result = CompactionResult::new(
            original_count,
            new_messages.len(),
            summary,
            original_tokens.saturating_sub(new_tokens),
        );

        Ok(Some(result))
    }

    /// Get the compaction trigger
    pub fn trigger(&self) -> &Arc<CompactionTrigger> {
        &self.trigger
    }

    /// Get the current strategy
    pub fn strategy(&self) -> CompactionStrategy {
        self.strategy
    }
}

/// Estimate total tokens from messages (rough estimate: 4 chars ≈ 1 token)
pub fn estimate_tokens(messages: &[Message]) -> usize {
    messages.iter().map(|m| estimate_message_tokens(m)).sum()
}

fn estimate_message_tokens(message: &Message) -> usize {
    let mut count = 0;

    // Role overhead
    count += match message.role {
        rcode_core::Role::User => 4,
        rcode_core::Role::Assistant => 4,
        rcode_core::Role::System => 4,
    };

    // Parts content
    for part in &message.parts {
        match part {
            rcode_core::Part::Text { content } => count += content.len() / 4,
            rcode_core::Part::ToolCall {
                name, arguments, ..
            } => {
                count += name.len() / 4;
                count += arguments.to_string().len() / 4;
            }
            rcode_core::Part::ToolResult { content, .. } => count += content.len() / 4,
            rcode_core::Part::Reasoning { content } => count += content.len() / 4,
            rcode_core::Part::Attachment {
                name, mime_type, ..
            } => {
                count += name.len() / 4;
                count += mime_type.len() / 4;
                // Content is binary, estimate fewer tokens
                count += 10;
            }
            rcode_core::Part::TaskChecklist { items } => {
                count += items
                    .iter()
                    .map(|item| (item.content.len() + item.status.len() + item.priority.len()) / 4)
                    .sum::<usize>();
            }
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{Message, Part, Role};

    fn create_test_message(role: Role, content: &str) -> Message {
        Message {
            id: rcode_core::MessageId::new(),
            session_id: "test".to_string(),
            role,
            parts: vec![Part::Text {
                content: content.to_string(),
            }],
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_trigger_should_compact_by_count() {
        let config = CompactionConfig::default();
        let trigger = CompactionTrigger::new(config);

        // Under threshold
        let messages: Vec<Message> = (0..30)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();
        assert!(!trigger.should_compact(&messages));

        // Over threshold
        let messages: Vec<Message> = (0..60)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();
        assert!(trigger.should_compact(&messages));
    }

    #[test]
    fn test_trigger_respects_min_messages() {
        let mut config = CompactionConfig::default();
        config.min_messages_to_compact = 10;
        let trigger = CompactionTrigger::new(config);

        // Under min_messages_to_compact
        let messages: Vec<Message> = (0..5)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();
        assert!(!trigger.should_compact(&messages));
    }

    #[test]
    fn test_token_estimation() {
        let message = create_test_message(Role::User, "Hello, world!");
        let tokens = estimate_message_tokens(&message);
        // "Hello, world!" = 13 chars / 4 ≈ 3 tokens + role overhead 4 = 7
        assert!(tokens >= 7);
    }

    #[test]
    fn test_compaction_service_creation() {
        // This test would need a mock LLM provider
        // Integration tests would cover this better
    }

    struct TestProvider;
    #[async_trait::async_trait]
    impl rcode_core::LlmProvider for TestProvider {
        async fn complete(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::CompletionResponse> {
            Ok(rcode_core::CompletionResponse {
                content: "summary".into(), reasoning: None, tool_calls: vec![],
                usage: rcode_core::TokenUsage { input_tokens: 0, output_tokens: 0, total_tokens: Some(0) },
                stop_reason: rcode_core::provider::StopReason::EndTurn,
            })
        }
        async fn stream(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::StreamingResponse> { unimplemented!() }
        fn model_info(&self, _model_id: &str) -> Option<rcode_core::ModelInfo> { None }
    }

    #[test]
    fn test_trigger_config_accessor() {
        let config = CompactionConfig::new(50, 100, 0.8, 5);
        let trigger = CompactionTrigger::new(config);
        assert_eq!(trigger.config().max_messages, 50);
    }

    #[test]
    fn test_trigger_default() {
        let trigger = CompactionTrigger::default();
        assert!(!trigger.should_compact(&[]));
    }

    #[test]
    fn test_trigger_should_compact_by_tokens() {
        let config = CompactionConfig::new(5, 100, 0.5, 5);
        let trigger = CompactionTrigger::new(config);
        let msgs: Vec<Message> = (0..10)
            .map(|i| create_test_message(Role::User, &"A".repeat(200)))
            .collect();
        assert!(trigger.should_compact(&msgs));
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(&[]), 0);
    }

    #[test]
    fn test_estimate_message_tokens_with_tool_call() {
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "test".into(),
            role: Role::Assistant,
            parts: vec![Part::ToolCall {
                id: "tc1".into(),
                name: "bash".into(),
                arguments: Box::new(serde_json::json!({"cmd": "ls -la"})),
            }],
            created_at: chrono::Utc::now(),
        };
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens > 4);
    }

    #[test]
    fn test_estimate_message_tokens_with_reasoning() {
        let msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "test".into(),
            role: Role::Assistant,
            parts: vec![Part::Reasoning { content: "thinking step by step".into() }],
            created_at: chrono::Utc::now(),
        };
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens > 4);
    }

    #[tokio::test]
    async fn test_maybe_compact_not_needed() {
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "model".into()
        ));
        let config = CompactionConfig::default();
        let service = CompactionService::new(summarizer, config, CompactionStrategy::Hybrid { preserved_recent: 5, max_total: 50 });

        let msgs = vec![create_test_message(Role::User, "short")];
        let result = service.maybe_compact(&msgs, "s1").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_service_trigger_accessor() {
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "model".into()
        ));
        let service = CompactionService::new(summarizer, CompactionConfig::default(), CompactionStrategy::PreserveFirst { count: 2 });
        assert!(!service.trigger().should_compact(&[]));
    }

    #[tokio::test]
    async fn test_service_strategy_accessor() {
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "model".into()
        ));
        let service = CompactionService::new(summarizer, CompactionConfig::default(), CompactionStrategy::PreserveRecent { count: 5 });
        assert_eq!(service.strategy(), CompactionStrategy::PreserveRecent { count: 5 });
    }

    #[tokio::test]
    async fn test_service_with_trigger() {
        let trigger = Arc::new(CompactionTrigger::new(CompactionConfig::default()));
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "model".into()
        ));
        let service = CompactionService::with_trigger(trigger, summarizer, CompactionStrategy::Hybrid { preserved_recent: 10, max_total: 50 });
        assert!(!service.should_compact(&[]));
    }

    #[tokio::test]
    async fn test_maybe_compact_success() {
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "model".into()
        ));
        let config = CompactionConfig::new(5, 100, 0.5, 5);
        let service = CompactionService::new(
            summarizer,
            config,
            CompactionStrategy::PreserveRecent { count: 2 }
        );

        // Create enough messages to trigger compaction
        let msgs: Vec<Message> = (0..20)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();
        
        let result = service.maybe_compact(&msgs, "s1").await.unwrap();
        assert!(result.is_some());
        let compaction = result.unwrap();
        assert!(compaction.new_count < msgs.len());
        assert!(compaction.tokens_saved > 0);
    }

    struct FailingSummarizer;
    #[async_trait::async_trait]
    impl rcode_core::LlmProvider for FailingSummarizer {
        async fn complete(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::CompletionResponse> {
            Err(rcode_core::RCodeError::Provider("Summarizer failed".into()))
        }
        async fn stream(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::StreamingResponse> { unimplemented!() }
        fn model_info(&self, _model_id: &str) -> Option<rcode_core::ModelInfo> { None }
    }

    #[tokio::test]
    async fn test_maybe_compact_summarizer_error() {
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(FailingSummarizer), "model".into()
        ));
        let config = CompactionConfig::new(5, 100, 0.5, 1);
        let service = CompactionService::new(
            summarizer,
            config,
            CompactionStrategy::PreserveRecent { count: 2 }
        );

        let msgs: Vec<Message> = (0..20)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();
        
        let result = service.maybe_compact(&msgs, "s1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_maybe_compact_empty_to_summarize() {
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "model".into()
        ));
        // PreserveRecent with count equal to message count means nothing to summarize
        let config = CompactionConfig::new(5, 100, 0.5, 1);
        let service = CompactionService::new(
            summarizer,
            config,
            CompactionStrategy::PreserveRecent { count: 10 }
        );

        // Only 3 messages - PreserveRecent{10} will preserve all 3
        let msgs: Vec<Message> = (0..3)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();
        
        let result = service.maybe_compact(&msgs, "s1").await.unwrap();
        // With PreserveRecent{10} and only 3 messages, nothing to summarize
        assert!(result.is_none());
    }
}
