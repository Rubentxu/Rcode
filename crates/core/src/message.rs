//! Message and Part types for conversation state

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub session_id: String,
    pub role: Role,
    pub parts: Vec<Part>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text {
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Box<serde_json::Value>,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
        is_error: bool,
    },
    Reasoning {
        content: String,
    },
    Attachment {
        id: String,
        name: String,
        mime_type: String,
        content: Vec<u8>,
    },
}

impl Message {
    pub fn user(session_id: String, parts: Vec<Part>) -> Self {
        Self {
            id: MessageId::new(),
            session_id,
            role: Role::User,
            parts,
            created_at: Utc::now(),
        }
    }

    pub fn assistant(session_id: String, parts: Vec<Part>) -> Self {
        Self {
            id: MessageId::new(),
            session_id,
            role: Role::Assistant,
            parts,
            created_at: Utc::now(),
        }
    }
}

/// Pagination parameters for message retrieval
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PaginationParams {
    pub offset: usize,
    pub limit: usize,
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 50,
        }
    }
}

impl PaginationParams {
    pub fn new(offset: usize, limit: usize) -> Self {
        Self { offset, limit }
    }
}

/// Paginated message response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedMessages {
    pub messages: Vec<Message>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

/// Token budget for context management
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TokenBudget {
    pub max_tokens: usize,
    pub used_tokens: usize,
}

impl TokenBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            used_tokens: 0,
        }
    }

    pub fn remaining(&self) -> usize {
        self.max_tokens.saturating_sub(self.used_tokens)
    }

    pub fn is_exhausted(&self) -> bool {
        self.remaining() == 0
    }

    /// Estimate tokens from text (rough estimate: 4 chars ≈ 1 token)
    pub fn estimate_tokens(text: &str) -> usize {
        text.len() / 4
    }

    pub fn add_tokens(&mut self, tokens: usize) {
        self.used_tokens += tokens;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_new() {
        let id1 = MessageId::new();
        let id2 = MessageId::new();
        assert_ne!(id1.0, id2.0); // Should be unique
        assert!(!id1.0.is_empty());
    }

    #[test]
    fn test_message_id_default() {
        let id = MessageId::default();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn test_message_user() {
        let msg = Message::user(
            "session1".into(),
            vec![Part::Text {
                content: "Hello".into(),
            }],
        );
        assert_eq!(msg.session_id, "session1");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.parts.len(), 1);
        assert!(!msg.id.0.is_empty());
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant(
            "session1".into(),
            vec![Part::Text {
                content: "Hi".into(),
            }],
        );
        assert_eq!(msg.session_id, "session1");
        assert_eq!(msg.role, Role::Assistant);
    }

    #[test]
    fn test_pagination_params_default() {
        let params = PaginationParams::default();
        assert_eq!(params.offset, 0);
        assert_eq!(params.limit, 50);
    }

    #[test]
    fn test_pagination_params_new() {
        let params = PaginationParams::new(10, 25);
        assert_eq!(params.offset, 10);
        assert_eq!(params.limit, 25);
    }

    #[test]
    fn test_token_budget_new() {
        let budget = TokenBudget::new(1000);
        assert_eq!(budget.max_tokens, 1000);
        assert_eq!(budget.used_tokens, 0);
    }

    #[test]
    fn test_token_budget_remaining() {
        let mut budget = TokenBudget::new(100);
        assert_eq!(budget.remaining(), 100);
        budget.add_tokens(30);
        assert_eq!(budget.remaining(), 70);
    }

    #[test]
    fn test_token_budget_is_exhausted() {
        let mut budget = TokenBudget::new(100);
        assert!(!budget.is_exhausted());
        budget.add_tokens(100);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn test_token_budget_exhausted_at_limit() {
        let mut budget = TokenBudget::new(50);
        budget.add_tokens(50);
        assert!(budget.is_exhausted());
        assert_eq!(budget.remaining(), 0);
    }

    #[test]
    fn test_token_budget_estimate_tokens() {
        assert_eq!(TokenBudget::estimate_tokens("abcd"), 1); // 4 chars = 1 token
        assert_eq!(TokenBudget::estimate_tokens("12345678"), 2); // 8 chars = 2 tokens
        assert_eq!(TokenBudget::estimate_tokens(""), 0); // empty
    }

    #[test]
    fn test_token_budget_add_tokens() {
        let mut budget = TokenBudget::new(1000);
        budget.add_tokens(100);
        assert_eq!(budget.used_tokens, 100);
        budget.add_tokens(200);
        assert_eq!(budget.used_tokens, 300);
    }

    #[test]
    fn test_role_serialization() {
        let user = Role::User;
        let json = serde_json::to_string(&user).unwrap();
        assert_eq!(json, "\"user\"");

        let assistant = Role::Assistant;
        let json = serde_json::to_string(&assistant).unwrap();
        assert_eq!(json, "\"assistant\"");

        let system = Role::System;
        let json = serde_json::to_string(&system).unwrap();
        assert_eq!(json, "\"system\"");
    }

    #[test]
    fn test_role_deserialization() {
        let user: Role = serde_json::from_str("\"user\"").unwrap();
        assert_eq!(user, Role::User);

        let assistant: Role = serde_json::from_str("\"assistant\"").unwrap();
        assert_eq!(assistant, Role::Assistant);

        let system: Role = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(system, Role::System);
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = Message::user(
            "session1".into(),
            vec![Part::Text {
                content: "Hello".into(),
            }],
        );
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, msg.session_id);
        assert_eq!(parsed.role, msg.role);
        assert_eq!(parsed.parts.len(), msg.parts.len());
    }

    #[test]
    fn test_part_text_serialization() {
        let part = Part::Text {
            content: "test".into(),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("text"));
        assert!(json.contains("test"));
    }

    #[test]
    fn test_part_tool_call_serialization() {
        let part = Part::ToolCall {
            id: "call_123".into(),
            name: "bash".into(),
            arguments: Box::new(serde_json::json!({"cmd": "ls"})),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("tool_call"));
        assert!(json.contains("call_123"));
        assert!(json.contains("bash"));
    }

    #[test]
    fn test_part_reasoning_serialization() {
        let part = Part::Reasoning {
            content: "thinking".into(),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("reasoning"));
        assert!(json.contains("thinking"));
    }

    #[test]
    fn test_paginated_messages_serialization() {
        let pm = PaginatedMessages {
            messages: vec![],
            total: 0,
            offset: 0,
            limit: 50,
        };
        let json = serde_json::to_string(&pm).unwrap();
        assert!(json.contains("total"));
        assert!(json.contains("offset"));
        assert!(json.contains("limit"));
    }
}
