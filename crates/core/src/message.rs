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
        arguments: serde_json::Value,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
