//! Configuration types matching RCode's config schema
//!
//! RCode config follows a cascading merge strategy:
//! 1. ~/.config/opencode/config.json
//! 2. ~/.config/opencode/opencode.json
//! 3. ~/.config/opencode/opencode.jsonc
//! 4. Project .opencode/ configs
//! 5. OPENCODE_CONFIG file
//! 6. OPENCODE_CONFIG_CONTENT env var
//! 7. Account/remote config
//!
//! Model format is "provider/model" (e.g., "anthropic/claude-3-5-sonnet")

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct RcodeConfig {
    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub small_model: Option<String>,

    #[serde(default)]
    pub default_agent: Option<String>,

    #[serde(default)]
    pub username: Option<String>,

    #[serde(default)]
    pub log_level: Option<LogLevel>,

    #[serde(default)]
    pub server: Option<ServerConfig>,

    #[serde(default)]
    pub agent: Option<AgentConfigMap>,

    #[serde(rename = "mode", default)]
    pub mode: Option<AgentConfigMap>,

    #[serde(default)]
    pub command: Option<CommandConfigMap>,

    #[serde(default)]
    pub skills: Option<SkillsConfig>,

    #[serde(default)]
    pub watcher: Option<WatcherConfig>,

    #[serde(default)]
    pub snapshot: Option<bool>,

    #[serde(default)]
    pub plugin: Option<Vec<PluginEntry>>,

    #[serde(default)]
    pub share: Option<ShareMode>,

    #[serde(rename = "autoshare", default)]
    pub autoshare: Option<bool>,

    #[serde(default)]
    pub autoupdate: Option<AutoupdateConfig>,

    /// Enable automatic message compaction when session exceeds threshold
    /// Defaults to false for backwards compatibility
    #[serde(rename = "auto_compact", default)]
    pub auto_compact: bool,

    /// Maximum messages before triggering compaction.
    /// Only used when auto_compact is enabled.
    /// Defaults to 50.
    #[serde(rename = "compact_threshold_messages", default)]
    pub compact_threshold_messages: Option<usize>,

    /// Number of messages to keep after compaction.
    /// Only used when auto_compact is enabled.
    /// Defaults to 20.
    #[serde(rename = "compact_keep_messages", default)]
    pub compact_keep_messages: Option<usize>,

    #[serde(rename = "disabled_providers", default)]
    pub disabled_providers: Option<Vec<String>>,

    #[serde(rename = "enabled_providers", default)]
    pub enabled_providers: Option<Vec<String>>,

    #[serde(default)]
    pub instructions: Option<Vec<String>>,

    #[serde(flatten, default)]
    pub extra: serde_json::Value,

    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: None,
            disabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub enum LogLevel {
    #[serde(rename = "DEBUG")]
    Debug,
    #[serde(rename = "INFO")]
    Info,
    #[serde(rename = "WARN")]
    Warn,
    #[default]
    #[serde(rename = "ERROR")]
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ServerConfig {
    #[serde(default)]
    pub port: Option<u16>,

    #[serde(default)]
    pub hostname: Option<String>,

    #[serde(default)]
    pub mdns: Option<bool>,

    #[serde(rename = "mdnsDomain", default)]
    pub mdns_domain: Option<String>,

    #[serde(default)]
    pub cors: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct SkillsConfig {
    #[serde(default)]
    pub paths: Option<Vec<String>>,

    #[serde(default)]
    pub urls: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct WatcherConfig {
    #[serde(default)]
    pub ignore: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PluginEntry {
    String(String),
    Array(Vec<serde_json::Value>),
}

impl Default for PluginEntry {
    fn default() -> Self {
        PluginEntry::String(String::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShareMode {
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "auto")]
    Auto,
    #[default]
    #[serde(rename = "disabled")]
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AutoupdateConfig {
    Bool(bool),
    Notify,
}

impl Default for AutoupdateConfig {
    fn default() -> Self {
        AutoupdateConfig::Bool(true)
    }
}

pub type AgentConfigMap = std::collections::HashMap<String, AgentConfig>;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct AgentConfig {
    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub variant: Option<String>,

    #[serde(default)]
    pub temperature: Option<f64>,

    #[serde(rename = "top_p", default)]
    pub top_p: Option<f64>,

    #[serde(default)]
    pub prompt: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub mode: Option<AgentMode>,

    #[serde(default)]
    pub hidden: Option<bool>,

    #[serde(default)]
    pub steps: Option<u32>,

    #[serde(rename = "maxSteps", default)]
    pub max_steps: Option<u32>,

    /// Optional max_tokens override for this agent
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Optional reasoning effort for this agent (e.g. "low", "high")
    #[serde(default)]
    pub reasoning_effort: Option<String>,

    #[serde(default)]
    pub permission: Option<AgentPermissionConfig>,

    #[serde(flatten, default)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    #[serde(rename = "subagent")]
    Subagent,
    #[serde(rename = "primary")]
    Primary,
    #[default]
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct AgentPermissionConfig {
    #[serde(default)]
    pub read: Option<PermissionRule>,

    #[serde(default)]
    pub edit: Option<PermissionRule>,

    #[serde(default)]
    pub glob: Option<PermissionRule>,

    #[serde(default)]
    pub grep: Option<PermissionRule>,

    #[serde(default)]
    pub list: Option<PermissionRule>,

    #[serde(default)]
    pub bash: Option<PermissionRule>,

    #[serde(default)]
    pub task: Option<PermissionRule>,

    #[serde(rename = "external_directory", default)]
    pub external_directory: Option<PermissionRule>,

    #[serde(rename = "todowrite", default)]
    pub todowrite: Option<PermissionAction>,

    #[serde(default)]
    pub question: Option<PermissionAction>,

    #[serde(default)]
    pub webfetch: Option<PermissionAction>,

    #[serde(default)]
    pub websearch: Option<PermissionAction>,

    #[serde(default)]
    pub codesearch: Option<PermissionAction>,

    #[serde(default)]
    pub lsp: Option<PermissionRule>,

    #[serde(rename = "doom_loop", default)]
    pub doom_loop: Option<PermissionAction>,

    #[serde(default)]
    pub skill: Option<PermissionRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PermissionRule {
    Action(PermissionAction),
    Object(std::collections::HashMap<String, PermissionAction>),
}

impl Default for PermissionRule {
    fn default() -> Self {
        PermissionRule::Action(PermissionAction::Ask)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    #[serde(rename = "ask")]
    Ask,
    #[serde(rename = "allow")]
    Allow,
    #[default]
    #[serde(rename = "deny")]
    Deny,
}

pub type CommandConfigMap = std::collections::HashMap<String, CommandConfig>;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct CommandConfig {
    #[serde(default)]
    pub template: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub agent: Option<String>,

    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub subtask: Option<bool>,
}

impl RcodeConfig {
    pub fn model_for_agent(&self, agent_name: &str) -> Option<&str> {
        self.agent
            .as_ref()
            .and_then(|agents| agents.get(agent_name))
            .and_then(|agent| agent.model.as_deref())
            .or(self.model.as_deref())
    }

    pub fn max_tokens_for_agent(&self, agent_name: &str) -> Option<u32> {
        self.agent
            .as_ref()
            .and_then(|agents| agents.get(agent_name))
            .and_then(|agent| agent.max_tokens)
    }

    pub fn reasoning_effort_for_agent(&self, agent_name: &str) -> Option<&str> {
        self.agent
            .as_ref()
            .and_then(|agents| agents.get(agent_name))
            .and_then(|agent| agent.reasoning_effort.as_deref())
    }

    pub fn effective_model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn effective_small_model(&self) -> Option<&str> {
        self.small_model.as_deref()
    }

    pub fn effective_default_agent(&self) -> Option<&str> {
        self.default_agent.as_deref().or(Some("build"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rcode_config_default() {
        let config = RcodeConfig::default();
        assert!(config.model.is_none());
        assert!(config.agent.is_none());
        assert!(config.server.is_none());
    }

    #[test]
    fn test_model_for_agent_prefers_agent_specific() {
        let mut config = RcodeConfig::default();
        config.model = Some("anthropic/claude-3-5-sonnet".to_string());

        let mut agent_config = AgentConfig::default();
        agent_config.model = Some("openai/gpt-4o".to_string());

        let mut agents = AgentConfigMap::new();
        agents.insert("custom".to_string(), agent_config);
        config.agent = Some(agents);

        assert_eq!(config.model_for_agent("custom"), Some("openai/gpt-4o"));
        assert_eq!(
            config.model_for_agent("unknown"),
            Some("anthropic/claude-3-5-sonnet")
        );
    }

    #[test]
    fn test_effective_default_agent_falls_back_to_build() {
        let config = RcodeConfig::default();
        assert_eq!(config.effective_default_agent(), Some("build"));
    }

    #[test]
    fn test_max_tokens_for_agent() {
        let mut config = RcodeConfig::default();

        let mut agent_config = AgentConfig::default();
        agent_config.max_tokens = Some(8192);

        let mut agents = AgentConfigMap::new();
        agents.insert("coder".to_string(), agent_config);
        config.agent = Some(agents);

        assert_eq!(config.max_tokens_for_agent("coder"), Some(8192));
        assert_eq!(config.max_tokens_for_agent("unknown"), None);
    }

    #[test]
    fn test_reasoning_effort_for_agent() {
        let mut config = RcodeConfig::default();

        let mut agent_config = AgentConfig::default();
        agent_config.reasoning_effort = Some("high".to_string());

        let mut agents = AgentConfigMap::new();
        agents.insert("coder".to_string(), agent_config);
        config.agent = Some(agents);

        assert_eq!(config.reasoning_effort_for_agent("coder"), Some("high"));
        assert_eq!(config.reasoning_effort_for_agent("unknown"), None);
    }

    #[test]
    fn test_agent_config_deserialization_with_new_fields() {
        let json = r#"{
            "model": "openai/gpt-4o",
            "max_tokens": 8192,
            "reasoning_effort": "high"
        }"#;

        let agent_config: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(agent_config.model, Some("openai/gpt-4o".to_string()));
        assert_eq!(agent_config.max_tokens, Some(8192));
        assert_eq!(agent_config.reasoning_effort, Some("high".to_string()));
    }

    #[test]
    fn test_agent_config_deserialization_without_new_fields() {
        let json = r#"{
            "model": "openai/gpt-4o"
        }"#;

        let agent_config: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(agent_config.model, Some("openai/gpt-4o".to_string()));
        assert_eq!(agent_config.max_tokens, None);
        assert_eq!(agent_config.reasoning_effort, None);
    }

    #[test]
    fn test_rcode_config_auto_compact_defaults_to_false() {
        let config = RcodeConfig::default();
        assert!(!config.auto_compact);
    }

    #[test]
    fn test_rcode_config_auto_compact_deserialization() {
        let json = r#"{
            "auto_compact": true
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        assert!(config.auto_compact);
    }

    #[test]
    fn test_rcode_config_auto_compact_explicit_false() {
        let json = r#"{
            "auto_compact": false
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        assert!(!config.auto_compact);
    }

    #[test]
    fn test_rcode_config_compaction_thresholds_default_to_none() {
        let config = RcodeConfig::default();
        assert!(config.compact_threshold_messages.is_none());
        assert!(config.compact_keep_messages.is_none());
    }

    #[test]
    fn test_rcode_config_compaction_thresholds_deserialization() {
        let json = r#"{
            "auto_compact": true,
            "compact_threshold_messages": 100,
            "compact_keep_messages": 30
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        assert!(config.auto_compact);
        assert_eq!(config.compact_threshold_messages, Some(100));
        assert_eq!(config.compact_keep_messages, Some(30));
    }

    #[test]
    fn test_rcode_config_compaction_thresholds_partial_deserialization() {
        let json = r#"{
            "auto_compact": true,
            "compact_threshold_messages": 75
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        assert!(config.auto_compact);
        assert_eq!(config.compact_threshold_messages, Some(75));
        assert_eq!(config.compact_keep_messages, None);
    }

    #[test]
    fn test_rcode_config_compaction_thresholds_only_keep() {
        let json = r#"{
            "compact_keep_messages": 25
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        assert!(!config.auto_compact);
        assert_eq!(config.compact_threshold_messages, None);
        assert_eq!(config.compact_keep_messages, Some(25));
    }
}
