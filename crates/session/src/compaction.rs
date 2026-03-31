//! Context compaction for handling long conversations
//!
//! This module provides automatic summarization to manage conversation history
//! when it exceeds configured thresholds (message count or token limit).

use serde::{Deserialize, Serialize};

/// Configuration for compaction behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Maximum number of messages before triggering compaction
    pub max_messages: usize,
    /// Maximum tokens before triggering compaction
    pub max_tokens: usize,
    /// Trigger compaction when reaching this percentage of max_tokens (0.0-1.0)
    pub summary_threshold: f32,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_messages: 50,
            max_tokens: 100_000,
            summary_threshold: 0.8,
        }
    }
}

impl CompactionConfig {
    /// Create a new config with custom values
    pub fn new(max_messages: usize, max_tokens: usize, summary_threshold: f32) -> Self {
        Self {
            max_messages,
            max_tokens,
            summary_threshold,
        }
    }

    /// Token threshold at which to trigger compaction
    pub fn token_threshold(&self) -> usize {
        (self.max_tokens as f32 * self.summary_threshold) as usize
    }
}

/// Strategy for how to compact messages
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompactionStrategy {
    /// Summarize messages beyond the threshold, keeping recent messages intact
    SummarizeOlder,
    /// Keep first and recent messages, truncate the middle portion
    TruncateMiddle,
    /// Summarize older messages first, truncate if still too long
    Hybrid,
}

impl Default for CompactionStrategy {
    fn default() -> Self {
        Self::Hybrid
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    /// Original message count before compaction
    pub original_count: usize,
    /// New message count after compaction
    pub new_count: usize,
    /// The summary message that replaces the compacted messages
    pub summary_message: opencode_core::Message,
    /// Estimated tokens saved by compaction
    pub tokens_saved: usize,
}

impl CompactionResult {
    pub fn new(
        original_count: usize,
        new_count: usize,
        summary_message: opencode_core::Message,
        tokens_saved: usize,
    ) -> Self {
        Self {
            original_count,
            new_count,
            summary_message,
            tokens_saved,
        }
    }
}

/// Check if compaction is needed based on message count
pub fn needs_compaction_by_count(
    messages: &[opencode_core::Message],
    config: &CompactionConfig,
) -> bool {
    messages.len() > config.max_messages
}

/// Check if compaction is needed based on token estimate
pub fn needs_compaction_by_tokens(
    messages: &[opencode_core::Message],
    config: &CompactionConfig,
) -> bool {
    let total_tokens = estimate_message_tokens(messages);
    total_tokens > config.token_threshold()
}

/// Estimate total tokens from messages (rough estimate: 4 chars ≈ 1 token)
pub fn estimate_message_tokens(messages: &[opencode_core::Message]) -> usize {
    messages
        .iter()
        .map(|m| estimate_message_token_count(m))
        .sum()
}

fn estimate_message_token_count(message: &opencode_core::Message) -> usize {
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
    fn test_default_config() {
        let config = CompactionConfig::default();
        assert_eq!(config.max_messages, 50);
        assert_eq!(config.max_tokens, 100_000);
        assert!((config.summary_threshold - 0.8).abs() < f32::EPSILON);
        assert_eq!(config.token_threshold(), 80_000);
    }

    #[test]
    fn test_needs_compaction_by_count() {
        let config = CompactionConfig::default();
        let messages: Vec<Message> = (0..60)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        assert!(needs_compaction_by_count(&messages, &config));

        let small_messages: Vec<Message> = (0..30)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        assert!(!needs_compaction_by_count(&small_messages, &config));
    }

    #[test]
    fn test_token_estimation() {
        let message = create_test_message(Role::User, "Hello, world!");
        let tokens = estimate_message_token_count(&message);
        // "Hello, world!" = 13 chars / 4 ≈ 3 tokens + role overhead 4 = 7
        assert!(tokens >= 7);
    }

    #[test]
    fn test_compaction_result() {
        let summary = create_test_message(Role::Assistant, "Summary");
        let result = CompactionResult::new(100, 10, summary, 90);
        assert_eq!(result.original_count, 100);
        assert_eq!(result.new_count, 10);
        assert_eq!(result.tokens_saved, 90);
    }
}
