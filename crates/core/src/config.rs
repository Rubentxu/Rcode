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

use crate::provider::ProviderProtocol;

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

    /// Permission rules for declarative tool access control.
    /// Rules are evaluated in order, last matching rule wins (iptables-style).
    /// Empty rules list means all tools are allowed (backward compatible).
    #[serde(rename = "permissions", default)]
    pub permissions: Option<crate::permission::PermissionRulesConfig>,

    /// Tools configuration including truncation settings
    #[serde(rename = "tools", default)]
    pub tools: Option<ToolsConfig>,
}

/// Tools configuration including truncation settings
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ToolsConfig {
    /// Truncation configuration for tool outputs
    #[serde(default)]
    pub truncation: Option<TruncationConfig>,
}

/// Truncation configuration for tool outputs
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TruncationConfig {
    /// Maximum bytes before truncating tool output (default 50KB)
    #[serde(default = "default_max_output_bytes")]
    pub max_bytes: usize,
    
    /// Number of characters to include in preview (default 2000)
    #[serde(default = "default_preview_chars")]
    pub preview_chars: usize,
    
    /// Directory to store truncated output files
    /// Defaults to system temp dir / rcode-truncation
    #[serde(default)]
    pub truncation_dir: Option<String>,
    
    /// Per-tool truncation overrides (tool_id -> max_bytes)
    #[serde(default)]
    pub tool_overrides: Option<std::collections::HashMap<String, usize>>,
}

fn default_max_output_bytes() -> usize { 50 * 1024 }
fn default_preview_chars() -> usize { 2000 }

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

/// Per-model configuration for a provider.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ProviderModelConfig {
    /// Whether this model is enabled. Defaults to true when absent.
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Protocol hint for custom/unknown providers.
    /// Built-in providers get their protocol from the registry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<ProviderProtocol>,
    /// Whether this provider is enabled. Defaults to true when absent.
    #[serde(default)]
    pub enabled: bool,
    /// Legacy field - use `enabled` instead (inverted logic)
    #[serde(default)]
    pub disabled: bool,
    /// Display name for this provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "name")]
    pub display_name: Option<String>,
    /// Per-model configuration for this provider.
    #[serde(default)]
    pub models: Option<std::collections::HashMap<String, ProviderModelConfig>>,
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
            // Generic approach: check {PROVIDER}_MODEL env vars for all configured providers.
            // Iterate through configured providers and check each one's _MODEL env var.
            for provider_id in self.providers.keys() {
                let env_var = format!("{}_MODEL", provider_id.to_uppercase().replace('-', "_"));
                if let Ok(model) = std::env::var(&env_var)
                    && !model.is_empty()
                {
                    return Some(format!("{}/{}", provider_id, model));
                }
            }
            None
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
    use std::sync::Mutex;

    /// Global mutex to serialize env-sensitive tests.
    /// This ensures only one env-sensitive test runs at a time, preventing
    /// interference between parallel test runs.
    static ENVVAR_LOCK: Mutex<()> = Mutex::new(());

    /// RAII guard for environment variable isolation in tests.
    /// Saves the original values of specified env vars, sets new values,
    /// and restores originals on drop (even on panic).
    /// Uses a global lock to serialize access when tests run in parallel.
    struct EnvGuard {
        originals: std::collections::HashMap<String, Option<String>>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        /// Create a new guard that:
        /// 1. Acquires global lock to serialize env access
        /// 2. Records current values of the given env vars
        /// 3. Sets each var to the provided value (if Some) or removes it (if None)
        fn new(vars: &[(&str, Option<&str>)]) -> Self {
            // Acquire lock to serialize with other env-sensitive tests
            let lock = ENVVAR_LOCK.lock().unwrap();

            let originals: std::collections::HashMap<String, Option<String>> = vars
                .iter()
                .map(|(name, _)| {
                    let original = std::env::var(*name).ok();
                    (name.to_string(), original)
                })
                .collect();

            // Set new values (unsafe in test mode)
            for (name, value) in vars {
                // SAFETY: Test-only env var manipulation - values are static strings without NUL
                unsafe {
                    match value {
                        Some(v) => std::env::set_var(name, v),
                        None => std::env::remove_var(name),
                    }
                }
            }

            Self {
                originals,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, original) in &self.originals {
                // SAFETY: Test-only env var cleanup - restoring original state
                unsafe {
                    match original {
                        Some(v) => std::env::set_var(name, v),
                        None => std::env::remove_var(name),
                    }
                }
            }
        }
    }

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
        // Use EnvGuard to isolate env vars - guard drops at end of scope
        let _guard = EnvGuard::new(&[
            ("ANTHROPIC_MODEL", Some("claude-sonnet-test")),
            ("MINIMAX_MODEL", Some("minimax-test")),
            ("ZAI_MODEL", None), // removed
        ]);

        let config = RcodeConfig {
            model: Some("anthropic/claude-3-5-sonnet".to_string()),
            ..Default::default()
        };

        // Config should take precedence over env vars
        assert_eq!(
            config.effective_model(),
            Some("anthropic/claude-3-5-sonnet".to_string())
        );
    }

    #[test]
    fn test_effective_model_falls_back_to_provider_model_env() {
        // Test with minimax in providers
        let _guard = EnvGuard::new(&[("MINIMAX_MODEL", Some("minimax-01")), ("ZAI_MODEL", None)]);

        let mut config = RcodeConfig::default();
        config
            .providers
            .insert("minimax".to_string(), ProviderConfig::default());

        assert_eq!(
            config.effective_model(),
            Some("minimax/minimax-01".to_string())
        );
    }

    #[test]
    fn test_effective_model_returns_none_when_no_provider_configured() {
        let _guard = EnvGuard::new(&[
            ("ANTHROPIC_MODEL", Some("claude-sonnet-4-5")),
            ("MINIMAX_MODEL", Some("MiniMax-M2.7")),
        ]);

        // No providers configured - should return None
        let config = RcodeConfig::default();
        assert_eq!(config.effective_model(), None);
    }

    #[test]
    fn test_effective_model_ignores_env_var_for_unconfigured_provider() {
        let _guard = EnvGuard::new(&[
            ("ANTHROPIC_MODEL", Some("claude-sonnet-4-5")),
            ("MINIMAX_MODEL", None),
            ("ZAI_MODEL", None),
        ]);

        // Only minimax is configured - ANTHROPIC_MODEL should be ignored
        let mut config = RcodeConfig::default();
        config
            .providers
            .insert("minimax".to_string(), ProviderConfig::default());

        assert_eq!(config.effective_model(), None);
    }

    #[test]
    fn test_effective_model_returns_none_when_no_config_or_env() {
        let _guard = EnvGuard::new(&[
            ("ANTHROPIC_MODEL", Some("")),
            ("MINIMAX_MODEL", Some("")),
            ("ZAI_MODEL", Some("")),
        ]);

        let config = RcodeConfig::default();
        assert_eq!(config.effective_model(), None);
    }

    #[test]
    fn test_effective_model_ignores_empty_env_var() {
        let _guard = EnvGuard::new(&[
            ("ANTHROPIC_MODEL", Some("")),
            ("MINIMAX_MODEL", Some("")),
            ("ZAI_MODEL", Some("")),
        ]);

        let mut config = RcodeConfig::default();
        config
            .providers
            .insert("minimax".to_string(), ProviderConfig::default());

        // Empty MINIMAX_MODEL should be ignored
        assert_eq!(config.effective_model(), None);
    }

    #[test]
    fn test_effective_model_first_configured_provider_wins() {
        let _guard = EnvGuard::new(&[
            ("MINIMAX_MODEL", Some("MiniMax-M2.7")),
            ("ZAI_MODEL", Some("zai-coding-standard")),
        ]);

        // Both minimax and zai configured - minimax comes first in iteration
        let mut config = RcodeConfig::default();
        config
            .providers
            .insert("minimax".to_string(), ProviderConfig::default());
        config
            .providers
            .insert("zai".to_string(), ProviderConfig::default());

        // HashMap iteration order is not guaranteed, so we just verify it returns one of them
        let result = config.effective_model();
        assert!(result.is_some());
        // The result should be either minimax/minimax-01 or zai/zai-coding-standard
        let val = result.unwrap();
        assert!(val == "minimax/MiniMax-M2.7" || val == "zai/zai-coding-standard");
    }

    // =============================================================================
    // ProviderModelConfig tests
    // =============================================================================

    #[test]
    fn test_provider_model_config_enabled_defaults_to_none() {
        // When enabled is None, it means "not explicitly set" - use provider default
        let model_config = ProviderModelConfig::default();
        assert!(model_config.enabled.is_none());
    }

    #[test]
    fn test_provider_model_config_enabled_can_be_set_false() {
        let model_config = ProviderModelConfig { enabled: Some(false) };
        assert_eq!(model_config.enabled, Some(false));
    }

    #[test]
    fn test_provider_model_config_enabled_can_be_set_true() {
        let model_config = ProviderModelConfig { enabled: Some(true) };
        assert_eq!(model_config.enabled, Some(true));
    }

    #[test]
    fn test_provider_config_models_hashmap_works() {
        use std::collections::HashMap;
        
        let mut providers = HashMap::new();
        providers.insert("openai".to_string(), ProviderConfig::default());
        
        // Add a model config
        let model_config = ProviderModelConfig {
            enabled: Some(false),
        };
        
        if let Some(provider) = providers.get_mut("openai") {
            let mut models = HashMap::new();
            models.insert("gpt-4o".to_string(), model_config);
            provider.models = Some(models);
        }
        
        // Verify the model config is stored correctly
        let provider = providers.get("openai").unwrap();
        let models = provider.models.as_ref().unwrap();
        let model = models.get("gpt-4o").unwrap();
        assert_eq!(model.enabled, Some(false));
    }

    #[test]
    fn test_provider_config_models_multiple_models() {
        use std::collections::HashMap;
        
        let mut provider = ProviderConfig::default();
        
        let mut models = HashMap::new();
        models.insert("gpt-4o".to_string(), ProviderModelConfig { enabled: Some(true) });
        models.insert("gpt-4o-mini".to_string(), ProviderModelConfig { enabled: Some(false) });
        provider.models = Some(models);
        
        let models = provider.models.as_ref().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models.get("gpt-4o").unwrap().enabled, Some(true));
        assert_eq!(models.get("gpt-4o-mini").unwrap().enabled, Some(false));
    }

    #[test]
    fn test_provider_config_serde_with_models() {
        let json = r#"{
            "providers": {
                "openai": {
                    "api_key": "sk-test",
                    "models": {
                        "gpt-4o": { "enabled": false },
                        "gpt-4o-mini": { "enabled": true }
                    }
                }
            }
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        
        let provider = config.providers.get("openai").unwrap();
        let models = provider.models.as_ref().unwrap();
        
        assert_eq!(models.get("gpt-4o").unwrap().enabled, Some(false));
        assert_eq!(models.get("gpt-4o-mini").unwrap().enabled, Some(true));
    }

    #[test]
    fn test_provider_config_serde_models_defaults() {
        // When models exist but enabled is not specified, it should be None
        let json = r#"{
            "providers": {
                "openai": {
                    "models": {
                        "gpt-4o": {}
                    }
                }
            }
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        
        let provider = config.providers.get("openai").unwrap();
        let models = provider.models.as_ref().unwrap();
        
        assert!(models.get("gpt-4o").unwrap().enabled.is_none());
    }

    #[test]
    fn test_rcode_config_permission_rules_default_to_none() {
        let config = RcodeConfig::default();
        assert!(config.permissions.is_none());
    }

    #[test]
    fn test_rcode_config_permission_rules_deserialization() {
        let json = r#"{
            "permissions": {
                "rules": [
                    { "tool": "bash", "pattern": "git push", "action": "deny" },
                    { "tool": "bash", "pattern": "rm -rf", "action": "ask" },
                    { "tool": "bash", "pattern": "ls", "action": "allow" }
                ]
            }
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        
        let permissions = config.permissions.unwrap();
        assert_eq!(permissions.rules.len(), 3);
        
        // Check first rule
        assert_eq!(permissions.rules[0].tool, "bash");
        assert_eq!(permissions.rules[0].pattern, "git push");
        assert_eq!(permissions.rules[0].action, crate::permission::PermissionRuleAction::Deny);
        
        // Check second rule
        assert_eq!(permissions.rules[1].tool, "bash");
        assert_eq!(permissions.rules[1].pattern, "rm -rf");
        assert_eq!(permissions.rules[1].action, crate::permission::PermissionRuleAction::Ask);
        
        // Check third rule
        assert_eq!(permissions.rules[2].tool, "bash");
        assert_eq!(permissions.rules[2].pattern, "ls");
        assert_eq!(permissions.rules[2].action, crate::permission::PermissionRuleAction::Allow);
    }

    #[test]
    fn test_rcode_config_permission_rules_empty_rules_is_allowed() {
        let json = r#"{
            "permissions": {
                "rules": []
            }
        }"#;

        let config: RcodeConfig = serde_json::from_str(json).unwrap();
        
        let permissions = config.permissions.unwrap();
        assert!(permissions.rules.is_empty());
    }
}
