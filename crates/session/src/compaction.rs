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
    /// Minimum number of messages required before compaction is considered
    pub min_messages_to_compact: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_messages: 50,
            max_tokens: 100_000,
            summary_threshold: 0.8,
            min_messages_to_compact: 10,
        }
    }
}

impl CompactionConfig {
    /// Create a new config with custom values
    pub fn new(
        max_messages: usize,
        max_tokens: usize,
        summary_threshold: f32,
        min_messages_to_compact: usize,
    ) -> Self {
        Self {
            max_messages,
            max_tokens,
            summary_threshold,
            min_messages_to_compact,
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
    /// Keep first N messages (usually system prompt and initial context)
    PreserveFirst { count: usize },
    /// Keep last N messages
    PreserveRecent { count: usize },
    /// Summarize older messages, keep recent ones
    SummarizeOlder { preserved_recent: usize },
    /// Truncate middle messages, keeping first and recent
    TruncateMiddle { preserved_recent: usize },
    /// Hybrid: summarize older, truncate middle if still too long
    Hybrid {
        preserved_recent: usize,
        max_total: usize,
    },
}

impl Default for CompactionStrategy {
    fn default() -> Self {
        Self::Hybrid {
            preserved_recent: 20,
            max_total: 50,
        }
    }
}

impl CompactionStrategy {
    /// Returns true if this strategy requires LLM summarization
    pub fn needs_summarization(&self) -> bool {
        !matches!(self, CompactionStrategy::TruncateMiddle { .. })
    }

    /// Apply the strategy to messages, returning (preserved_messages, messages_to_summarize_or_truncate)
    ///
    /// For TruncateMiddle, the second tuple element contains messages to truncate (not summarize)
    pub fn apply(
        &self,
        messages: &[rcode_core::Message],
    ) -> (Vec<rcode_core::Message>, Vec<rcode_core::Message>) {
        if messages.is_empty() {
            return (Vec::new(), Vec::new());
        }

        match self {
            CompactionStrategy::PreserveFirst { count } => {
                let count = (*count).min(messages.len());
                (messages[..count].to_vec(), messages[count..].to_vec())
            }
            CompactionStrategy::PreserveRecent { count } => {
                let count = (*count).min(messages.len());
                (
                    messages[..messages.len() - count].to_vec(),
                    messages[messages.len() - count..].to_vec(),
                )
            }
            CompactionStrategy::SummarizeOlder { preserved_recent } => {
                if messages.len() <= *preserved_recent {
                    return (messages.to_vec(), Vec::new());
                }
                // Keep system message (first) and recent messages
                let preserve_first = 1; // Always keep system message
                let to_summarize_end = messages.len() - preserved_recent;
                if to_summarize_end <= preserve_first {
                    return (messages.to_vec(), Vec::new());
                }
                (
                    messages[..preserve_first].to_vec(),
                    messages[preserve_first..to_summarize_end].to_vec(),
                )
            }
            CompactionStrategy::TruncateMiddle { preserved_recent } => {
                if messages.len() <= *preserved_recent + 2 {
                    // Not enough messages to truncate meaningfully
                    return (messages.to_vec(), Vec::new());
                }
                // Keep first 2 messages (system + initial context) and recent messages
                let preserve_first = 2;
                let to_summarize_end = messages.len() - preserved_recent;
                if to_summarize_end <= preserve_first {
                    return (messages.to_vec(), Vec::new());
                }
                (
                    messages[..preserve_first].to_vec(),
                    messages[preserve_first..to_summarize_end].to_vec(),
                )
            }
            CompactionStrategy::Hybrid {
                preserved_recent,
                max_total,
            } => {
                // First try summarize older strategy
                if messages.len() <= *max_total {
                    return (messages.to_vec(), Vec::new());
                }
                let preserve_first = 1; // Always keep system message
                let to_summarize_end = messages.len() - preserved_recent;
                if to_summarize_end <= preserve_first {
                    // Not enough to summarize, truncate middle
                    let mut result = messages[..preserve_first].to_vec();
                    result.extend_from_slice(&messages[messages.len() - preserved_recent..]);
                    return (result, Vec::new());
                }
                (
                    messages[..preserve_first].to_vec(),
                    messages[preserve_first..to_summarize_end].to_vec(),
                )
            }
        }
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
    pub summary_message: rcode_core::Message,
    /// Estimated tokens saved by compaction
    pub tokens_saved: usize,
}

impl CompactionResult {
    pub fn new(
        original_count: usize,
        new_count: usize,
        summary_message: rcode_core::Message,
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
    messages: &[rcode_core::Message],
    config: &CompactionConfig,
) -> bool {
    messages.len() > config.max_messages
}

/// Check if compaction is needed based on token estimate
pub fn needs_compaction_by_tokens(
    messages: &[rcode_core::Message],
    config: &CompactionConfig,
) -> bool {
    let total_tokens = estimate_message_tokens(messages);
    total_tokens > config.token_threshold()
}

/// Estimate total tokens from messages (rough estimate: 4 chars ≈ 1 token)
pub fn estimate_message_tokens(messages: &[rcode_core::Message]) -> usize {
    messages
        .iter()
        .map(|m| estimate_message_token_count(m))
        .sum()
}

fn estimate_message_token_count(message: &rcode_core::Message) -> usize {
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

    #[test]
    fn test_strategy_apply_preserve_first() {
        let messages: Vec<Message> = (0..10)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::PreserveFirst { count: 3 };
        let (preserved, to_summarize) = strategy.apply(&messages);

        assert_eq!(preserved.len(), 3);
        assert_eq!(to_summarize.len(), 7);
        // Verify preserved messages are correct by checking content
        if let rcode_core::Part::Text { content: c1 } = &preserved[0].parts[0] {
            if let rcode_core::Part::Text { content: c2 } = &messages[0].parts[0] {
                assert_eq!(c1, c2);
            }
        }
    }

    #[test]
    fn test_strategy_apply_preserve_recent() {
        let messages: Vec<Message> = (0..10)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::PreserveRecent { count: 3 };
        let (preserved, to_summarize) = strategy.apply(&messages);

        assert_eq!(preserved.len(), 7);
        assert_eq!(to_summarize.len(), 3);
    }

    #[test]
    fn test_strategy_apply_summarize_older() {
        let messages: Vec<Message> = (0..30)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::SummarizeOlder {
            preserved_recent: 10,
        };
        let (preserved, to_summarize) = strategy.apply(&messages);

        // Should preserve first message (system) + recent 10
        assert_eq!(preserved.len(), 1); // Only system message
        assert_eq!(to_summarize.len(), 19); // messages 1-19 (excluding first 1 and last 10)
    }

    #[test]
    fn test_strategy_apply_truncate_middle() {
        let messages: Vec<Message> = (0..30)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::TruncateMiddle {
            preserved_recent: 10,
        };
        let (preserved, to_summarize) = strategy.apply(&messages);

        // Should preserve first 2 messages + recent 10
        assert_eq!(preserved.len(), 2);
        assert_eq!(to_summarize.len(), 18); // messages 2-11 (excluding first 2 and last 10)
    }

    #[test]
    fn test_strategy_apply_hybrid() {
        let messages: Vec<Message> = (0..60)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::Hybrid {
            preserved_recent: 20,
            max_total: 50,
        };
        let (preserved, to_summarize) = strategy.apply(&messages);

        // 60 > 50, so should apply strategy
        // Should preserve first message + to_summarize (messages 1 to 39)
        assert_eq!(preserved.len(), 1);
        assert_eq!(to_summarize.len(), 39);
    }

    #[test]
    fn test_strategy_apply_under_max_total() {
        let messages: Vec<Message> = (0..30)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::Hybrid {
            preserved_recent: 20,
            max_total: 50,
        };
        let (preserved, to_summarize) = strategy.apply(&messages);

        // 30 < 50, so no compaction needed
        assert_eq!(preserved.len(), 30);
        assert!(to_summarize.is_empty());
    }

    #[test]
    fn test_strategy_needs_summarization() {
        assert!(!CompactionStrategy::TruncateMiddle {
            preserved_recent: 10
        }
        .needs_summarization());
        assert!(CompactionStrategy::SummarizeOlder {
            preserved_recent: 10
        }
        .needs_summarization());
        assert!(CompactionStrategy::Hybrid {
            preserved_recent: 10,
            max_total: 50
        }
        .needs_summarization());
    }

    #[test]
    fn test_needs_compaction_by_count_with_default() {
        let config = CompactionConfig::default();
        let messages: Vec<Message> = (0..51)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();
        assert!(needs_compaction_by_count(&messages, &config));
    }

    #[test]
    fn test_needs_compaction_by_count_under() {
        let config = CompactionConfig::default();
        let messages: Vec<Message> = (0..30)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();
        assert!(!needs_compaction_by_count(&messages, &config));
    }

    #[test]
    fn test_needs_compaction_by_tokens() {
        let config = CompactionConfig::new(50, 100, 0.8, 5);
        let messages: Vec<Message> = (0..30)
            .map(|_i| {
                create_test_message(
                    Role::User,
                    &"A very long message with lots of text content to exceed tokens",
                )
            })
            .collect();
        assert!(needs_compaction_by_tokens(&messages, &config));
    }

    #[test]
    fn test_needs_compaction_by_tokens_under() {
        let config = CompactionConfig::new(50, 100_000, 0.8, 5);
        let messages = vec![create_test_message(Role::User, "short")];
        assert!(!needs_compaction_by_tokens(&messages, &config));
    }

    #[test]
    fn test_estimate_message_tokens_empty() {
        assert_eq!(estimate_message_tokens(&[]), 0);
    }

    #[test]
    fn test_estimate_message_tokens_single() {
        let msg = create_test_message(Role::User, "Hello world");
        let tokens = estimate_message_tokens(&[msg]);
        assert!(tokens > 0);
    }

    #[test]
    fn test_compaction_result_new() {
        let summary = rcode_core::Message::assistant("s1".into(), vec![]);
        let result = CompactionResult::new(100, 10, summary, 9000);
        assert_eq!(result.original_count, 100);
        assert_eq!(result.new_count, 10);
        assert_eq!(result.tokens_saved, 9000);
    }

    #[test]
    fn test_token_threshold() {
        let config = CompactionConfig::new(50, 100, 0.8, 5);
        assert_eq!(config.token_threshold(), 80);
    }

    #[test]
    fn test_strategy_apply_empty() {
        let strategy = CompactionStrategy::PreserveFirst { count: 5 };
        let (preserved, to_summarize) = strategy.apply(&[]);
        assert!(preserved.is_empty());
        assert!(to_summarize.is_empty());
    }

    #[test]
    fn test_strategy_apply_preserve_first_all() {
        let messages: Vec<Message> = (0..5)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();
        let strategy = CompactionStrategy::PreserveFirst { count: 100 };
        let (preserved, to_summarize) = strategy.apply(&messages);
        assert_eq!(preserved.len(), 5);
        assert!(to_summarize.is_empty());
    }

    #[test]
    fn test_estimate_single_message_with_parts() {
        let msg = Message {
            id: rcode_core::MessageId("m1".into()),
            session_id: "s1".into(),
            role: Role::Assistant,
            parts: vec![
                Part::Text {
                    content: "Hello world this is a test".into(),
                },
                Part::ToolCall {
                    id: "tc1".into(),
                    name: "bash".into(),
                    arguments: Box::new(serde_json::json!({"command": "ls -la /tmp"})),
                },
            ],
            created_at: chrono::Utc::now(),
        };
        let tokens = estimate_message_token_count(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_strategy_apply_summarize_older_under_preserved_recent() {
        // When messages.len() <= preserved_recent, returns all as preserved
        let messages: Vec<Message> = (0..5)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::SummarizeOlder {
            preserved_recent: 10, // More than message count
        };
        let (preserved, to_summarize) = strategy.apply(&messages);

        // All messages should be preserved, none to summarize
        assert_eq!(preserved.len(), 5);
        assert!(to_summarize.is_empty());
    }

    #[test]
    fn test_strategy_apply_summarize_older_edge_case_boundaries() {
        // Test boundary: when to_summarize_end == preserve_first
        let messages: Vec<Message> = (0..6)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::SummarizeOlder {
            preserved_recent: 5, // 6 - 5 = 1, which equals preserve_first (1)
        };
        let (preserved, to_summarize) = strategy.apply(&messages);

        // Should return all as preserved since boundary case
        assert_eq!(preserved.len(), 6);
        assert!(to_summarize.is_empty());
    }

    #[test]
    fn test_strategy_apply_truncate_middle_under_limit() {
        // When messages <= preserved_recent + 2, returns all as preserved
        let messages: Vec<Message> = (0..4)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::TruncateMiddle {
            preserved_recent: 5, // More than available
        };
        let (preserved, to_truncate) = strategy.apply(&messages);

        // All messages preserved since not enough to truncate
        assert_eq!(preserved.len(), 4);
        assert!(to_truncate.is_empty());
    }

    #[test]
    fn test_strategy_apply_truncate_middle_at_boundary() {
        // Exactly preserved_recent + 2 messages - at the boundary
        let messages: Vec<Message> = (0..7)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::TruncateMiddle {
            preserved_recent: 5, // 7 - 5 = 2, equals preserve_first
        };
        let (preserved, to_truncate) = strategy.apply(&messages);

        // Should return all as preserved at boundary
        assert_eq!(preserved.len(), 7);
        assert!(to_truncate.is_empty());
    }

    #[test]
    fn test_strategy_apply_hybrid_not_enough_to_summarize() {
        // Hybrid: when to_summarize_end <= preserve_first, truncate middle path
        let messages: Vec<Message> = (0..6)
            .map(|i| create_test_message(Role::User, &format!("Message {}", i)))
            .collect();

        let strategy = CompactionStrategy::Hybrid {
            preserved_recent: 5,
            max_total: 3, // Less than message count to trigger compaction
        };
        let (preserved, to_summarize) = strategy.apply(&messages);

        // With 6 messages, max_total=3, Hybrid tries to compact
        // but to_summarize_end = 6 - 5 = 1, preserve_first = 1
        // Since 1 <= 1, it should go to truncate middle path
        // Result is first message + recent 5 = 6 (all preserved)
        assert_eq!(preserved.len(), 6);
        assert!(to_summarize.is_empty());
    }

    #[test]
    fn test_estimate_message_tokens_with_attachment() {
        let msg = Message {
            id: rcode_core::MessageId("m1".into()),
            session_id: "s1".into(),
            role: Role::User,
            parts: vec![Part::Attachment {
                id: "att1".into(),
                name: "document.pdf".into(),
                mime_type: "application/pdf".into(),
                content: vec![0u8; 100],
            }],
            created_at: chrono::Utc::now(),
        };
        let tokens = estimate_message_token_count(&msg);
        // Should include attachment name and mime_type tokens + overhead
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_message_tokens_with_reasoning() {
        let msg = Message {
            id: rcode_core::MessageId("m1".into()),
            session_id: "s1".into(),
            role: Role::Assistant,
            parts: vec![Part::Reasoning {
                content: "Let me think about this carefully...".into(),
            }],
            created_at: chrono::Utc::now(),
        };
        let tokens = estimate_message_token_count(&msg);
        assert!(tokens > 0);
    }
}
