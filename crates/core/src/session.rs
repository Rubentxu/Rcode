//! Session types

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub project_path: std::path::PathBuf,
    pub agent_id: String,
    pub model_id: String,
    pub parent_id: Option<String>,
    pub title: Option<String>,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // G3: Usage tracking fields
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_cost_usd: f64,
    pub summary_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Idle,
    Running,
    Completed,
    Aborted,
}

impl SessionStatus {
    /// Valid status transitions
    pub fn can_transition_to(&self, new_status: SessionStatus) -> bool {
        match self {
            // From Idle, can go to Running
            SessionStatus::Idle => matches!(new_status, SessionStatus::Running),
            // From Running, can go to Idle (for reuse), Completed, Aborted
            SessionStatus::Running => {
                matches!(
                    new_status,
                    SessionStatus::Idle | SessionStatus::Completed | SessionStatus::Aborted
                )
            }
            // Terminal states - no transitions allowed
            SessionStatus::Completed | SessionStatus::Aborted => false,
        }
    }
}

impl Session {
    pub fn new(project_path: std::path::PathBuf, agent_id: String, model_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::new(),
            project_path,
            agent_id,
            model_id,
            parent_id: None,
            title: None,
            status: SessionStatus::Idle,
            created_at: now,
            updated_at: now,
            // G3: Initialize usage tracking fields
            prompt_tokens: 0,
            completion_tokens: 0,
            total_cost_usd: 0.0,
            summary_message_id: None,
        }
    }

    pub fn with_parent(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Set the model ID for this session
    pub fn set_model(&mut self, model_id: String) {
        self.model_id = model_id;
    }

    /// G3: Add token usage to session metadata
    pub fn add_usage(&mut self, prompt_tokens: u64, completion_tokens: u64, cost_usd: f64) {
        self.prompt_tokens += prompt_tokens;
        self.completion_tokens += completion_tokens;
        self.total_cost_usd += cost_usd;
    }
}
