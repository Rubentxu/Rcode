//! Static provider registry with built-in provider definitions
//! Also serves as runtime instance registry for server state.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use rcode_core::ProviderProtocol;

use super::LlmProvider;

/// A built-in provider definition with static metadata
pub struct ProviderDefinition {
    /// Unique provider identifier (e.g., "anthropic", "openai")
    pub id: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// The wire protocol this provider uses
    pub protocol: ProviderProtocol,
    /// Default base URL for this provider
    pub default_base_url: &'static str,
    /// Alternative credential keys to try for this provider (e.g., "minimax-coding-plan")
    pub credential_aliases: &'static [&'static str],
    /// Fallback models when discovery fails
    pub fallback_models: &'static [&'static str],
    /// Environment variable prefix for API key (e.g., "ANTHROPIC" for ANTHROPIC_API_KEY)
    pub env_key_prefix: &'static str,
}

/// Combined metadata + runtime provider registry.
///
/// - Static metadata: built-in `ProviderDefinition` records (protocol, base_url, fallback models)
/// - Runtime instances: `Arc<dyn LlmProvider>` registered by the server/CLI at startup
pub struct ProviderRegistry {
    definitions: HashMap<&'static str, ProviderDefinition>,
    /// Runtime provider instances keyed by provider_id
    instances: HashMap<String, Arc<dyn LlmProvider>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    /// Returns a reference to the singleton metadata-only registry instance.
    pub fn get() -> &'static ProviderRegistry {
        static REGISTRY: OnceLock<ProviderRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| ProviderRegistry {
            definitions: built_in_registry(),
            instances: HashMap::new(),
        })
    }

    /// Create a new registry pre-loaded with the 7 built-in provider definitions.
    ///
    /// This is the correct constructor for `AppState.providers` — it ensures that
    /// metadata (protocol, base_url, fallback models) is always available even before
    /// any runtime instance is registered.
    pub fn new() -> Self {
        Self {
            definitions: built_in_registry(),
            instances: HashMap::new(),
        }
    }

    /// Create an empty registry (useful for tests that need a blank slate).
    pub fn empty() -> Self {
        Self {
            definitions: HashMap::new(),
            instances: HashMap::new(),
        }
    }

    /// Look up a provider definition by id
    pub fn get_def(&self, id: &str) -> Option<&ProviderDefinition> {
        self.definitions.get(id)
    }

    /// Iterate over all provider definitions
    pub fn iter(&self) -> impl Iterator<Item = &ProviderDefinition> {
        self.definitions.values()
    }

    /// Number of registered definitions
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    // ── Runtime instance management ──────────────────────────────────────────

    /// Register a runtime provider instance keyed by a logical provider id.
    ///
    /// The `logical_id` should be the external identifier used to request
    /// this provider (e.g. `"github-copilot"`), **not** the internal
    /// `provider.provider_id()` which may be a generic backend name like
    /// `"openai"`.
    pub fn register(&mut self, logical_id: impl Into<String>, provider: Arc<dyn LlmProvider>) {
        self.instances.insert(logical_id.into(), provider);
    }

    /// Retrieve a runtime provider instance by its logical provider id.
    pub fn get_instance(&self, provider_id: &str) -> Option<Arc<dyn LlmProvider>> {
        self.instances.get(provider_id).cloned()
    }
}

/// The global provider registry
pub static PROVIDER_REGISTRY: OnceLock<HashMap<&'static str, ProviderDefinition>> = OnceLock::new();

/// Returns the built-in provider registry
pub fn registry() -> &'static HashMap<&'static str, ProviderDefinition> {
    PROVIDER_REGISTRY.get_or_init(built_in_registry)
}

fn built_in_registry() -> HashMap<&'static str, ProviderDefinition> {
    let mut m = HashMap::new();

    // Anthropic
    m.insert(
        "anthropic",
        ProviderDefinition {
            id: "anthropic",
            display_name: "Anthropic",
            protocol: ProviderProtocol::AnthropicCompat,
            default_base_url: "https://api.anthropic.com",
            credential_aliases: &[],
            fallback_models: &[
                "claude-sonnet-4-5-20250514",
                "claude-opus-4-5-20250514",
                "claude-haiku-3-5-20241022",
            ],
            env_key_prefix: "ANTHROPIC",
        },
    );

    // OpenAI
    m.insert(
        "openai",
        ProviderDefinition {
            id: "openai",
            display_name: "OpenAI",
            protocol: ProviderProtocol::OpenAiCompat,
            default_base_url: "https://api.openai.com/v1",
            credential_aliases: &[],
            fallback_models: &[
                "gpt-4o-2024-11-20",
                "gpt-4o-mini-2024-07-18",
                "o3-mini-2025-01-31",
                "o4-mini-2025-04-16",
            ],
            env_key_prefix: "OPENAI",
        },
    );

    // Google
    m.insert(
        "google",
        ProviderDefinition {
            id: "google",
            display_name: "Google",
            protocol: ProviderProtocol::Google,
            default_base_url: "https://generativelanguage.googleapis.com",
            credential_aliases: &[],
            fallback_models: &[
                "gemini-2.5-pro-preview-05-06",
                "gemini-2.5-flash-preview-05-20",
                "gemini-2.0-flash",
            ],
            env_key_prefix: "GOOGLE",
        },
    );

    // OpenRouter
    m.insert(
        "openrouter",
        ProviderDefinition {
            id: "openrouter",
            display_name: "OpenRouter",
            protocol: ProviderProtocol::OpenAiCompat,
            default_base_url: "https://openrouter.ai/api/v1",
            credential_aliases: &[],
            fallback_models: &[
                "anthropic/claude-sonnet-4",
                "openai/gpt-4o",
                "google/gemini-2.5-pro",
            ],
            env_key_prefix: "OPENROUTER",
        },
    );

    // MiniMax
    m.insert(
        "minimax",
        ProviderDefinition {
            id: "minimax",
            display_name: "MiniMax",
            protocol: ProviderProtocol::OpenAiCompat,
            default_base_url: "https://api.minimax.chat/v1",
            credential_aliases: &["minimax-coding-plan"],
            fallback_models: &[
                "MiniMax-M2.7",
                "MiniMax-M2.7-highspeed",
                "MiniMax-M2.5",
                "MiniMax-M2.1",
            ],
            env_key_prefix: "MINIMAX",
        },
    );

    // ZAI
    m.insert(
        "zai",
        ProviderDefinition {
            id: "zai",
            display_name: "ZAI",
            protocol: ProviderProtocol::OpenAiCompat,
            default_base_url: "https://api.zai.chat/v1",
            credential_aliases: &[],
            fallback_models: &[
                "zai-coding-plan",
                "zai-coding-standard",
                "zai-coding-premium",
            ],
            env_key_prefix: "ZAI",
        },
    );

    // GitHub Copilot
    m.insert(
        "github-copilot",
        ProviderDefinition {
            id: "github-copilot",
            display_name: "GitHub Copilot",
            protocol: ProviderProtocol::OpenAiCompat,
            default_base_url: "https://api.githubcopilot.com",
            credential_aliases: &["github-copilot"],
            fallback_models: &["gpt-4o", "gpt-4o-mini", "claude-sonnet-4-5", "o3-mini"],
            env_key_prefix: "GITHUB_COPILOT",
        },
    );

    m
}

/// Lookup a provider definition by id
pub fn lookup(id: &str) -> Option<&'static ProviderDefinition> {
    registry().get(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_seven_builtin_providers_registered() {
        let reg = registry();
        assert!(reg.contains_key("anthropic"));
        assert!(reg.contains_key("openai"));
        assert!(reg.contains_key("google"));
        assert!(reg.contains_key("openrouter"));
        assert!(reg.contains_key("minimax"));
        assert!(reg.contains_key("zai"));
        assert!(reg.contains_key("github-copilot"));
        assert_eq!(reg.len(), 7);
    }

    #[test]
    fn test_lookup_anthropic_returns_correct_protocol() {
        let def = lookup("anthropic").expect("anthropic should be registered");
        assert_eq!(def.protocol, ProviderProtocol::AnthropicCompat);
        assert_eq!(def.display_name, "Anthropic");
    }

    #[test]
    fn test_lookup_openai_returns_openai_compat() {
        let def = lookup("openai").expect("openai should be registered");
        assert_eq!(def.protocol, ProviderProtocol::OpenAiCompat);
    }

    #[test]
    fn test_lookup_google_returns_google_protocol() {
        let def = lookup("google").expect("google should be registered");
        assert_eq!(def.protocol, ProviderProtocol::Google);
    }

    #[test]
    fn test_lookup_minimax_has_credential_aliases() {
        let def = lookup("minimax").expect("minimax should be registered");
        assert!(def.credential_aliases.contains(&"minimax-coding-plan"));
    }

    #[test]
    fn test_lookup_openrouter_is_openai_compat() {
        let def = lookup("openrouter").expect("openrouter should be registered");
        assert_eq!(def.protocol, ProviderProtocol::OpenAiCompat);
    }

    #[test]
    fn test_lookup_zai_is_openai_compat() {
        let def = lookup("zai").expect("zai should be registered");
        assert_eq!(def.protocol, ProviderProtocol::OpenAiCompat);
    }

    #[test]
    fn test_lookup_github_copilot_is_openai_compat() {
        let def = lookup("github-copilot").expect("github-copilot should be registered");
        assert_eq!(def.protocol, ProviderProtocol::OpenAiCompat);
        assert!(def.credential_aliases.contains(&"github-copilot"));
    }

    #[test]
    fn test_lookup_unknown_returns_none() {
        assert!(lookup("unknown-provider").is_none());
        assert!(lookup("nonexistent").is_none());
    }

    #[test]
    fn test_fallback_models_are_populated() {
        let def = lookup("anthropic").expect("anthropic should be registered");
        assert!(!def.fallback_models.is_empty());
        assert!(def.fallback_models.contains(&"claude-sonnet-4-5-20250514"));
    }

    #[test]
    fn test_env_key_prefix_is_populated() {
        let def = lookup("anthropic").expect("anthropic should be registered");
        assert_eq!(def.env_key_prefix, "ANTHROPIC");

        let def = lookup("minimax").expect("minimax should be registered");
        assert_eq!(def.env_key_prefix, "MINIMAX");
    }

    #[test]
    fn test_default_base_urls() {
        let def = lookup("anthropic").expect("anthropic should be registered");
        assert_eq!(def.default_base_url, "https://api.anthropic.com");

        let def = lookup("google").expect("google should be registered");
        assert_eq!(
            def.default_base_url,
            "https://generativelanguage.googleapis.com"
        );
    }
}
