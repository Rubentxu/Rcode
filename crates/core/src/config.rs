//! Configuration types for opencode

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Root configuration for opencode
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OpencodeConfig {
    pub providers: ProviderConfig,
    pub session: SessionConfig,
    pub tools: ToolsConfig,
    pub server: ServerConfig,
}

impl Default for OpencodeConfig {
    fn default() -> Self {
        Self {
            providers: ProviderConfig::default(),
            session: SessionConfig::default(),
            tools: ToolsConfig::default(),
            server: ServerConfig::default(),
        }
    }
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProviderConfig {
    pub default: String,
    pub anthropic: Option<ProviderCredentials>,
    pub openai: Option<ProviderCredentials>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default: "anthropic".to_string(),
            anthropic: None,
            openai: None,
        }
    }
}

/// Credentials for a provider
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProviderCredentials {
    pub api_key: String,
    pub model: Option<String>,
}

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionConfig {
    pub history_limit: usize,
    pub token_budget: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            history_limit: 100,
            token_budget: 100_000,
        }
    }
}

/// Tools configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolsConfig {
    pub timeout_seconds: u64,
    pub allowed_paths: Vec<String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 30,
            allowed_paths: vec![],
        }
    }
}

/// CORS configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CorsConfig {
    #[serde(default = "default_allow_origin")]
    pub allow_origin: String,
    #[serde(default = "default_allow_methods")]
    pub allow_methods: String,
    #[serde(default = "default_allow_headers")]
    pub allow_headers: String,
}

fn default_allow_origin() -> String {
    "*".to_string()
}

fn default_allow_methods() -> String {
    "*".to_string()
}

fn default_allow_headers() -> String {
    "*".to_string()
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allow_origin: default_allow_origin(),
            allow_methods: default_allow_methods(),
            allow_headers: default_allow_headers(),
        }
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub cors: CorsConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 4096,
            cors: CorsConfig::default(),
        }
    }
}

/// Run options for CLI run command
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub message: Option<String>,
    pub file: Option<String>,
    pub stdin: bool,
    pub json: bool,
    pub silent: bool,
    pub save_session: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            message: None,
            file: None,
            stdin: false,
            json: false,
            silent: false,
            save_session: true,
        }
    }
}
