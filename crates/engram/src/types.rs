//! Observation types for Engram persistent memory

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// An observation saved to persistent memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Optional ID, set by the storage layer
    pub id: Option<i64>,
    pub title: String,
    pub content: String,
    #[serde(rename = "type")]
    pub obs_type: ObservationType,
    pub scope: Scope,
    pub topic_key: Option<String>,
    pub project: Option<String>,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Observation {
    /// Create a new observation with the current timestamp
    pub fn new(title: String, content: String, obs_type: ObservationType) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            title,
            content,
            obs_type,
            scope: Scope::Project,
            topic_key: None,
            project: None,
            session_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new observation with all fields
    pub fn with_all(
        title: String,
        content: String,
        obs_type: ObservationType,
        scope: Scope,
        topic_key: Option<String>,
        project: Option<String>,
        session_id: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            title,
            content,
            obs_type,
            scope,
            topic_key,
            project,
            session_id,
            created_at: now,
            updated_at: now,
        }
    }

    /// Set the project scope
    pub fn with_project(mut self, project: String) -> Self {
        self.project = Some(project);
        self
    }

    /// Set the topic key
    pub fn with_topic(mut self, topic: String) -> Self {
        self.topic_key = Some(topic);
        self
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Set the scope
    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self
    }
}

/// Type of observation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ObservationType {
    Decision,
    Architecture,
    Bugfix,
    Pattern,
    Config,
    Discovery,
    Learning,
    ToolUse,
    FileChange,
    Command,
    Search,
    Manual,
}

impl std::fmt::Display for ObservationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObservationType::Decision => write!(f, "decision"),
            ObservationType::Architecture => write!(f, "architecture"),
            ObservationType::Bugfix => write!(f, "bugfix"),
            ObservationType::Pattern => write!(f, "pattern"),
            ObservationType::Config => write!(f, "config"),
            ObservationType::Discovery => write!(f, "discovery"),
            ObservationType::Learning => write!(f, "learning"),
            ObservationType::ToolUse => write!(f, "tool_use"),
            ObservationType::FileChange => write!(f, "file_change"),
            ObservationType::Command => write!(f, "command"),
            ObservationType::Search => write!(f, "search"),
            ObservationType::Manual => write!(f, "manual"),
        }
    }
}

impl std::str::FromStr for ObservationType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "decision" => Ok(ObservationType::Decision),
            "architecture" => Ok(ObservationType::Architecture),
            "bugfix" => Ok(ObservationType::Bugfix),
            "pattern" => Ok(ObservationType::Pattern),
            "config" => Ok(ObservationType::Config),
            "discovery" => Ok(ObservationType::Discovery),
            "learning" => Ok(ObservationType::Learning),
            "tool_use" | "tooluse" => Ok(ObservationType::ToolUse),
            "file_change" | "filechange" => Ok(ObservationType::FileChange),
            "command" => Ok(ObservationType::Command),
            "search" => Ok(ObservationType::Search),
            "manual" => Ok(ObservationType::Manual),
            _ => Err(format!("Unknown observation type: {}", s)),
        }
    }
}

/// Scope of the observation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Project,
    Personal,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Project => write!(f, "project"),
            Scope::Personal => write!(f, "personal"),
        }
    }
}

impl std::str::FromStr for Scope {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "project" => Ok(Scope::Project),
            "personal" => Ok(Scope::Personal),
            _ => Err(format!("Unknown scope: {}", s)),
        }
    }
}
