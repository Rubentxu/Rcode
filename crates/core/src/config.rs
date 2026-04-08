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

    #[serde(default)]
    pub lsp: Option<HashMap<String, LspServerConfig>>,
}

/// LSP server configuration for a specific language
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct LspServerConfig {
    #[serde(default)]
    pub command: String,

    #[serde(default)]
    pub args: Vec<String>,

    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub disabled: bool,
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

    pub fn effective_model(&self) -> Option<String> {
        self.model.clone().or_else(|| {
            // Check environment-based model signals when no explicit config.model is set.
            // This respects environment-based model configuration for various providers
            // including MiniMax Anthropic-compatible setups.
            std::env::var("ANTHROPIC_MODEL")
                .ok()
                .filter(|v| !v.is_empty())
                .or_else(|| {
                    std::env::var("MINIMAX_MODEL")
                        .ok()
                        .filter(|v| !v.is_empty())
                })
        })
    }

    pub fn effective_small_model(&self) -> Option<&str> {
        self.small_model.as_deref()
    }

    pub fn effective_default_agent(&self) -> Option<&str> {
        self.default_agent.as_deref().or(Some("build"))
    }

    pub fn tools_for_agent(&self, agent_name: &str) -> Option<Vec<String>> {
        self.agent
            .as_ref()
            .and_then(|agents| agents.get(agent_name))
            .and_then(|agent| agent.extra.get("tools"))
            .and_then(|tools| tools.as_object())
            .map(|tools| {
                tools
                    .iter()
                    .filter_map(|(tool_name, enabled)| {
                        enabled
                            .as_bool()
                            .filter(|value| *value)
                            .map(|_| tool_name.clone())
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|tools| !tools.is_empty())
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
        let mut config = RcodeConfig {
            model: Some("anthropic/claude-3-5-sonnet".to_string()),
            ..Default::default()
        };

        let agent_config = AgentConfig {
            model: Some("openai/gpt-4o".to_string()),
            ..Default::default()
        };

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

        let agent_config = AgentConfig {
            max_tokens: Some(8192),
            ..Default::default()
        };

        let mut agents = AgentConfigMap::new();
        agents.insert("coder".to_string(), agent_config);
        config.agent = Some(agents);

        assert_eq!(config.max_tokens_for_agent("coder"), Some(8192));
        assert_eq!(config.max_tokens_for_agent("unknown"), None);
    }

    #[test]
    fn test_reasoning_effort_for_agent() {
        let mut config = RcodeConfig::default();

        let agent_config = AgentConfig {
            reasoning_effort: Some("high".to_string()),
            ..Default::default()
        };

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

    #[test]
    fn test_lsp_server_config_deserialization() {
        let json = r#"{
            "command": "rust-analyzer",
            "args": ["--verbose"],
            "cwd": "/project"
        }"#;

        let config: LspServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.command, "rust-analyzer");
        assert_eq!(config.args, vec!["--verbose"]);
        assert_eq!(config.cwd, Some("/project".to_string()));
    }

    #[test]
    fn test_lsp_server_config_default() {
        let config = LspServerConfig::default();
        assert!(config.command.is_empty());
        assert!(config.args.is_empty());
        assert!(config.cwd.is_none());
    }

    #[test]
    fn test_rcode_config_with_lsp_servers() {
        let json = r#"{
            "model": "anthropic/claude-3-5-sonnet",
            "lsp": {
                "rust": {
                    "command": "rust-analyzer",
                    "args": [],
                    "cwd": "/workspace"
                },
                "typescript": {
                    "command": "typescript-language-server",
                    "args": ["--stdio"],
                    "cwd": null
                }
            }
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.model,
            Some("anthropic/claude-3-5-sonnet".to_string())
        );

        let lsp_config = config.lsp.as_ref().unwrap();
        assert_eq!(lsp_config.len(), 2);

        let rust_config = lsp_config.get("rust").unwrap();
        assert_eq!(rust_config.command, "rust-analyzer");
        assert!(rust_config.args.is_empty());
        assert_eq!(rust_config.cwd, Some("/workspace".to_string()));

        let ts_config = lsp_config.get("typescript").unwrap();
        assert_eq!(ts_config.command, "typescript-language-server");
        assert_eq!(ts_config.args, vec!["--stdio"]);
        assert_eq!(ts_config.cwd, None);
    }

    #[test]
    fn test_rcode_config_lsp_field_defaults_to_none() {
        let config = RcodeConfig::default();
        assert!(config.lsp.is_none());
    }

    #[test]
    fn test_lsp_server_config_missing_optional_fields() {
        let json = r#"{
            "command": "rust-analyzer"
        }"#;

        let config: LspServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.command, "rust-analyzer");
        assert!(config.args.is_empty());
        assert_eq!(config.cwd, None);
    }

    #[test]
    fn test_effective_model_prefers_config_over_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("ANTHROPIC_MODEL", "claude-sonnet-test");
            std::env::set_var("MINIMAX_MODEL", "minimax-test");
        }

        let config = RcodeConfig {
            model: Some("anthropic/claude-3-5-sonnet".to_string()),
            ..Default::default()
        };

        // Config should take precedence over env vars
        assert_eq!(
            config.effective_model(),
            Some("anthropic/claude-3-5-sonnet".to_string())
        );

        // SAFETY: Clean up env vars after test
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
            std::env::remove_var("MINIMAX_MODEL");
        }
    }

    #[test]
    fn test_effective_model_falls_back_to_anthropic_model_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
            std::env::remove_var("MINIMAX_MODEL");
            std::env::set_var("ANTHROPIC_MODEL", "claude-sonnet-4-5");
        }

        let config = RcodeConfig::default();
        assert_eq!(
            config.effective_model(),
            Some("claude-sonnet-4-5".to_string())
        );

        // SAFETY: Clean up env vars after test
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
        }
    }

    #[test]
    fn test_effective_model_falls_back_to_minimax_model_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
            std::env::remove_var("MINIMAX_MODEL");
            std::env::set_var("MINIMAX_MODEL", "MiniMax-M2.7");
        }

        let config = RcodeConfig::default();
        assert_eq!(config.effective_model(), Some("MiniMax-M2.7".to_string()));

        // SAFETY: Clean up env vars after test
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
            std::env::remove_var("MINIMAX_MODEL");
        }
    }

    #[test]
    fn test_effective_model_prefers_anthropic_over_minimax() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
            std::env::remove_var("MINIMAX_MODEL");
            std::env::set_var("ANTHROPIC_MODEL", "claude-haiku-test");
            std::env::set_var("MINIMAX_MODEL", "MiniMax-M2.7");
        }

        let config = RcodeConfig::default();
        // ANTHROPIC_MODEL should take precedence over MINIMAX_MODEL
        assert_eq!(
            config.effective_model(),
            Some("claude-haiku-test".to_string())
        );

        // SAFETY: Clean up env vars after test
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
            std::env::remove_var("MINIMAX_MODEL");
        }
    }

    #[test]
    fn test_effective_model_returns_none_when_no_config_or_env() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
            std::env::remove_var("MINIMAX_MODEL");
        }

        let config = RcodeConfig::default();
        assert_eq!(config.effective_model(), None);
    }

    #[test]
    fn test_effective_model_ignores_empty_env_var() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("ANTHROPIC_MODEL", "");
            std::env::remove_var("MINIMAX_MODEL");
        }

        let config = RcodeConfig::default();
        // Empty ANTHROPIC_MODEL should be ignored
        assert_eq!(config.effective_model(), None);

        // SAFETY: Clean up env vars after test
        unsafe {
            std::env::remove_var("ANTHROPIC_MODEL");
        }
    }
}
