//! Context compaction service for handling long conversations
//!
//! This module provides the main CompactionService that coordinates
//! trigger checking, summarization, and message compaction.

use std::sync::Arc;

use opencode_core::{Message, Result as CoreResult};

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
        opencode_core::Role::User => 4,
        opencode_core::Role::Assistant => 4,
        opencode_core::Role::System => 4,
    };

    // Parts content
    for part in &message.parts {
        match part {
            opencode_core::Part::Text { content } => count += content.len() / 4,
            opencode_core::Part::ToolCall {
                name, arguments, ..
            } => {
                count += name.len() / 4;
                count += arguments.to_string().len() / 4;
            }
            opencode_core::Part::ToolResult { content, .. } => count += content.len() / 4,
            opencode_core::Part::Reasoning { content } => count += content.len() / 4,
            opencode_core::Part::Attachment {
                name, mime_type, ..
            } => {
                count += name.len() / 4;
                count += mime_type.len() / 4;
                // Content is binary, estimate fewer tokens
                count += 10;
            }
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencode_core::{Message, Part, Role};

    fn create_test_message(role: Role, content: &str) -> Message {
        Message {
            id: opencode_core::MessageId::new(),
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
}