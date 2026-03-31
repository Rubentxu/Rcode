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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Primary agent mode - can spawn subagents
    Primary,
    /// Subagent mode - can only be spawned by other agents
    Subagent,
    /// All modes - can be used as primary or subagent
    All,
}

impl Default for AgentMode {
    fn default() -> Self {
        AgentMode::All
    }
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
            tools: if def.tools.is_empty() {
                None
            } else {
                Some(def.tools.clone())
            },
            hidden: if def.hidden { Some(true) } else { None },
        }
    }
}
