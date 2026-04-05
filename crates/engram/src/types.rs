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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observation_new() {
        let obs = Observation::new(
            "Test Title".to_string(),
            "Test Content".to_string(),
            ObservationType::Discovery,
        );
        assert_eq!(obs.title, "Test Title");
        assert_eq!(obs.content, "Test Content");
        assert_eq!(obs.obs_type, ObservationType::Discovery);
        assert_eq!(obs.scope, Scope::Project);
        assert!(obs.id.is_none());
        assert!(obs.topic_key.is_none());
        assert!(obs.project.is_none());
        assert!(obs.session_id.is_none());
    }

    #[test]
    fn test_observation_with_all() {
        let obs = Observation::with_all(
            "Title".to_string(),
            "Content".to_string(),
            ObservationType::Decision,
            Scope::Personal,
            Some("topic/key".to_string()),
            Some("myproject".to_string()),
            Some("session-123".to_string()),
        );
        assert_eq!(obs.title, "Title");
        assert_eq!(obs.obs_type, ObservationType::Decision);
        assert_eq!(obs.scope, Scope::Personal);
        assert_eq!(obs.topic_key, Some("topic/key".to_string()));
        assert_eq!(obs.project, Some("myproject".to_string()));
        assert_eq!(obs.session_id, Some("session-123".to_string()));
    }

    #[test]
    fn test_observation_builder_methods() {
        let obs = Observation::new("T".to_string(), "C".to_string(), ObservationType::Learning)
            .with_project("proj".to_string())
            .with_topic("topic".to_string())
            .with_session("sess".to_string())
            .with_scope(Scope::Personal);

        assert_eq!(obs.project, Some("proj".to_string()));
        assert_eq!(obs.topic_key, Some("topic".to_string()));
        assert_eq!(obs.session_id, Some("sess".to_string()));
        assert_eq!(obs.scope, Scope::Personal);
    }

    #[test]
    fn test_observation_type_parse_valid() {
        assert_eq!(
            "decision".parse::<ObservationType>().unwrap(),
            ObservationType::Decision
        );
        assert_eq!(
            "architecture".parse::<ObservationType>().unwrap(),
            ObservationType::Architecture
        );
        assert_eq!(
            "bugfix".parse::<ObservationType>().unwrap(),
            ObservationType::Bugfix
        );
        assert_eq!(
            "pattern".parse::<ObservationType>().unwrap(),
            ObservationType::Pattern
        );
        assert_eq!(
            "config".parse::<ObservationType>().unwrap(),
            ObservationType::Config
        );
        assert_eq!(
            "discovery".parse::<ObservationType>().unwrap(),
            ObservationType::Discovery
        );
        assert_eq!(
            "learning".parse::<ObservationType>().unwrap(),
            ObservationType::Learning
        );
        assert_eq!(
            "tool_use".parse::<ObservationType>().unwrap(),
            ObservationType::ToolUse
        );
        assert_eq!(
            "tooluse".parse::<ObservationType>().unwrap(),
            ObservationType::ToolUse
        );
        assert_eq!(
            "file_change".parse::<ObservationType>().unwrap(),
            ObservationType::FileChange
        );
        assert_eq!(
            "filechange".parse::<ObservationType>().unwrap(),
            ObservationType::FileChange
        );
        assert_eq!(
            "command".parse::<ObservationType>().unwrap(),
            ObservationType::Command
        );
        assert_eq!(
            "search".parse::<ObservationType>().unwrap(),
            ObservationType::Search
        );
        assert_eq!(
            "manual".parse::<ObservationType>().unwrap(),
            ObservationType::Manual
        );
    }

    #[test]
    fn test_observation_type_parse_case_insensitive() {
        assert_eq!(
            "DECISION".parse::<ObservationType>().unwrap(),
            ObservationType::Decision
        );
        assert_eq!(
            "Discovery".parse::<ObservationType>().unwrap(),
            ObservationType::Discovery
        );
    }

    #[test]
    fn test_observation_type_parse_invalid() {
        let result = "invalid".parse::<ObservationType>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown observation type"));
    }

    #[test]
    fn test_observation_type_display() {
        assert_eq!(ObservationType::Decision.to_string(), "decision");
        assert_eq!(ObservationType::Architecture.to_string(), "architecture");
        assert_eq!(ObservationType::Bugfix.to_string(), "bugfix");
        assert_eq!(ObservationType::Pattern.to_string(), "pattern");
        assert_eq!(ObservationType::Config.to_string(), "config");
        assert_eq!(ObservationType::Discovery.to_string(), "discovery");
        assert_eq!(ObservationType::Learning.to_string(), "learning");
        assert_eq!(ObservationType::ToolUse.to_string(), "tool_use");
        assert_eq!(ObservationType::FileChange.to_string(), "file_change");
        assert_eq!(ObservationType::Command.to_string(), "command");
        assert_eq!(ObservationType::Search.to_string(), "search");
        assert_eq!(ObservationType::Manual.to_string(), "manual");
    }

    #[test]
    fn test_scope_parse_valid() {
        assert_eq!("project".parse::<Scope>().unwrap(), Scope::Project);
        assert_eq!("personal".parse::<Scope>().unwrap(), Scope::Personal);
    }

    #[test]
    fn test_scope_parse_case_insensitive() {
        assert_eq!("PROJECT".parse::<Scope>().unwrap(), Scope::Project);
        assert_eq!("Personal".parse::<Scope>().unwrap(), Scope::Personal);
    }

    #[test]
    fn test_scope_parse_invalid() {
        let result = "invalid".parse::<Scope>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown scope"));
    }

    #[test]
    fn test_scope_display() {
        assert_eq!(Scope::Project.to_string(), "project");
        assert_eq!(Scope::Personal.to_string(), "personal");
    }

    #[test]
    fn test_observation_serialization() {
        let obs = Observation::new(
            "Test".to_string(),
            "Content".to_string(),
            ObservationType::Discovery,
        );
        let json = serde_json::to_value(&obs).unwrap();
        assert_eq!(json["title"], "Test");
        assert_eq!(json["content"], "Content");
        assert_eq!(json["type"], "discovery");
        assert_eq!(json["scope"], "project");
    }

    #[test]
    fn test_observation_deserialization() {
        use chrono::Utc;
        let now = Utc::now();
        let json = serde_json::json!({
            "id": 1,
            "title": "Test",
            "content": "Content",
            "type": "decision",
            "scope": "personal",
            "topic_key": "test/topic",
            "project": "myproject",
            "session_id": "sess-123",
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });
        let obs: Observation = serde_json::from_value(json).unwrap();
        assert_eq!(obs.title, "Test");
        assert_eq!(obs.obs_type, ObservationType::Decision);
        assert_eq!(obs.scope, Scope::Personal);
        assert_eq!(obs.topic_key, Some("test/topic".to_string()));
    }
}
