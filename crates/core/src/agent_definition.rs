//! Agent definition types for custom agents loaded from JSON

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::agent::AgentInfo;
use crate::permission::Permission;

/// Task permission configuration for agents
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskPermission {
    /// Permission pattern: "allow", "deny", "ask"
    #[serde(default)]
    pub pattern: Permission,
    /// List of allowed subagent types
    #[serde(default)]
    pub subagents: Vec<String>,
}

/// Agent-specific permission configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentPermissionConfig {
    /// Task-related permissions
    #[serde(default)]
    pub task: TaskPermission,
}

/// Agent mode determining how the agent can be used
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Primary agent mode - can spawn subagents
    Primary,
    /// Subagent mode - can only be spawned by other agents
    Subagent,
    /// All modes - can be used as primary or subagent
    #[default]
    All,
}

/// Agent definition loaded from JSON configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Unique identifier for this agent
    pub identifier: String,
    /// Human-readable name
    pub name: String,
    /// Detailed description of the agent's purpose
    #[serde(default)]
    pub description: String,
    /// When to use this agent
    #[serde(default)]
    pub when_to_use: String,
    /// System prompt for the agent
    pub system_prompt: String,
    /// Agent mode (default: All)
    #[serde(default = "default_mode")]
    pub mode: AgentMode,
    /// Whether to hide from agent list (default: false)
    #[serde(default)]
    pub hidden: bool,
    /// Permission configuration for this agent
    #[serde(default)]
    pub permission: AgentPermissionConfig,
    /// List of tools this agent can use (empty = all)
    #[serde(default)]
    pub tools: Vec<String>,
    /// Optional model override for this agent
    #[serde(default)]
    pub model: Option<String>,

    /// Optional max_tokens override for this agent
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Optional reasoning effort for this agent (e.g. "low", "high")
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

fn default_mode() -> AgentMode {
    AgentMode::All
}

impl From<&AgentDefinition> for AgentInfo {
    fn from(def: &AgentDefinition) -> Self {
        Self {
            id: def.identifier.clone(),
            name: def.name.clone(),
            description: def.description.clone(),
            model: def.model.clone(),
            max_tokens: def.max_tokens,
            reasoning_effort: def.reasoning_effort.clone(),
            tools: if def.tools.is_empty() {
                None
            } else {
                Some(def.tools.clone())
            },
            hidden: if def.hidden { Some(true) } else { None },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_definition_deserialization_with_new_fields() {
        let json = r#"{
            "identifier": "test-agent",
            "name": "Test Agent",
            "system_prompt": "You are a test",
            "max_tokens": 8192,
            "reasoning_effort": "high"
        }"#;

        let def: AgentDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(def.identifier, "test-agent");
        assert_eq!(def.max_tokens, Some(8192));
        assert_eq!(def.reasoning_effort, Some("high".to_string()));
    }

    #[test]
    fn test_agent_definition_deserialization_without_new_fields() {
        let json = r#"{
            "identifier": "test-agent",
            "name": "Test Agent",
            "system_prompt": "You are a test"
        }"#;

        let def: AgentDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(def.identifier, "test-agent");
        assert_eq!(def.max_tokens, None);
        assert_eq!(def.reasoning_effort, None);
    }

    #[test]
    fn test_agent_definition_to_agent_info_includes_new_fields() {
        let def = AgentDefinition {
            identifier: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            when_to_use: "Testing".to_string(),
            system_prompt: "You are a test".to_string(),
            mode: AgentMode::All,
            hidden: false,
            permission: AgentPermissionConfig::default(),
            tools: vec![],
            model: Some("claude-sonnet-4".to_string()),
            max_tokens: Some(8192),
            reasoning_effort: Some("high".to_string()),
        };

        let info: AgentInfo = (&def).into();
        assert_eq!(info.id, "test-agent");
        assert_eq!(info.model, Some("claude-sonnet-4".to_string()));
        assert_eq!(info.max_tokens, Some(8192));
        assert_eq!(info.reasoning_effort, Some("high".to_string()));
    }
}
