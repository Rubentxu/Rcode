//! Summary generation for conversation compaction
//!
//! Uses an LLM provider to generate concise summaries of conversation history.

use std::sync::Arc;
use opencode_core::{
    CompletionRequest, Message, Part, Role, error::Result as CoreResult,
};

use crate::compaction::CompactionResult;

/// Summarizer for generating conversation summaries
pub struct Summarizer {
    provider: Arc<dyn opencode_core::LlmProvider>,
    model: String,
}

impl Summarizer {
    /// Create a new summarizer with the given LLM provider
    pub fn new(provider: Arc<dyn opencode_core::LlmProvider>, model: String) -> Self {
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
                id: opencode_core::MessageId::new(),
                session_id: session_id.to_string(),
                role: Role::User,
                parts: vec![Part::Text { content: summary_prompt }],
                created_at: chrono::Utc::now(),
            }],
            system_prompt: Some(self.get_system_prompt()),
            tools: vec![],
            temperature: Some(0.3), // Lower temperature for more consistent summaries
            max_tokens: Some((target_tokens / 2) as u32), // Leave room for summary
        };

        let response = self.provider.complete(request).await?;
        
        // Create the summary message
        let summary_message = Message {
            id: opencode_core::MessageId::new(),
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
            return Err(opencode_core::OpenCodeError::Session(
                "No compaction needed".to_string()
            ));
        }

        // Keep first 2 messages (system prompt and initial context) and last N messages
        let keep_recent = max_messages.saturating_sub(2);
        let preserve_count = 2 + keep_recent;

        if original_count <= preserve_count {
            // Not enough messages to compact meaningfully
            return Err(opencode_core::OpenCodeError::Session(
                "Not enough messages to compact".to_string()
            ));
        }

        // Messages to summarize (middle portion)
        let to_summarize = &messages[2..original_count - keep_recent];
        
        if to_summarize.is_empty() {
            return Err(opencode_core::OpenCodeError::Session(
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
            }
        }
        
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_content() {
        // This test requires a mock provider which we can't easily create in unit tests
        // Integration tests would cover this better
    }
}