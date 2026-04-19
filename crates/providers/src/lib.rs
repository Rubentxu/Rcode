//! AI Provider abstractions and implementations
#![allow(
    clippy::collapsible_if,
    clippy::enum_variant_names,
    clippy::type_complexity,
    clippy::if_same_then_else,
    dead_code
)]

pub mod provider_trait;
pub mod anthropic;
pub mod google;
pub mod minimax;
pub mod openai;
pub mod openai_compat;
pub mod openrouter;
pub mod registry;
pub mod mock;
pub mod credentials;
pub mod rate_limit;
pub mod factory;
pub mod zai;
pub mod catalog;
pub mod resolution;

pub use provider_trait::LlmProvider;
pub use registry::{ProviderDefinition, ProviderRegistry, registry, lookup as lookup_provider};
pub use mock::MockLlmProvider;
pub use credentials::{load_api_key, resolve_api_key};
pub use rate_limit::TokenBucket;
pub use factory::{ProviderFactory, ModelInfo};
pub use catalog::{ModelCatalogService, CatalogModel, ListModelsResponse, ModelSource, CacheStore, DiscoveryIdentity};
pub use google::GoogleProvider;
pub use minimax::MiniMaxProvider;
pub use openrouter::OpenRouterProvider;
pub use zai::ZaiProvider;
pub use resolution::{resolve_auth, AuthSource, AuthKind, AuthState, AuthStateDto};

use std::sync::Arc;
use std::future::Future;
use std::time::Duration;
use rcode_core::{RcodeConfig, error::{RCodeError, Result}};

/// Retry with exponential backoff for transient errors
pub async fn retry_with_backoff<F, T, Fut>(mut f: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let max_retries = 5;
    let base_delay = Duration::from_secs(1);
    let max_delay = Duration::from_secs(32);
    
    let mut last_error = None;
    
    for attempt in 0..max_retries {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let is_transient = matches!(&e, 
                    RCodeError::Provider(p) if p.contains("Network") || p.contains("RateLimit")
                );
                
                if !is_transient || attempt == max_retries - 1 {
                    return Err(e);
                }
                
                last_error = Some(e);
                
                let delay = std::cmp::min(
                    base_delay * 2u32.pow(attempt as u32),
                    max_delay
                );
                
                tokio::time::sleep(delay).await;
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| 
        RCodeError::Provider("Max retries exceeded".into())
    ))
}

#[allow(dead_code)]
fn expand_env_var(value: &str) -> String {
    if let Some(var_name) = value.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        std::env::var(var_name).unwrap_or_default()
    } else {
        value.to_string()
    }
}

pub fn parse_model_id(model: &str) -> (String, String) {
    let parts: Vec<&str> = model.splitn(2, '/').collect();
    if parts.len() == 2 {
        (parts[0].to_string(), parts[1].to_string())
    } else {
        // Heuristic inference from well-known model name prefixes.
        // This preserves backward compatibility for bare model IDs used in configs and
        // session storage. The source is heuristic — callers that need provenance can
        // use parse_model_id_strict() which returns ("unknown", ...) for bare IDs.
        let model_lower = model.to_lowercase();
        if model_lower.starts_with("gpt-") || model_lower.starts_with("o1") || model_lower.starts_with("o3") {
            ("openai".to_string(), model.to_string())
        } else if model_lower.starts_with("claude-") {
            ("anthropic".to_string(), model.to_string())
        } else if model_lower.starts_with("gemini-") {
            ("google".to_string(), model.to_string())
        } else {
            ("unknown".to_string(), model.to_string())
        }
    }
}

pub fn model_from_model_string(model: &str) -> String {
    let (_, model_name) = parse_model_id(model);
    model_name
}

pub fn provider_id_from_model(model: &str) -> String {
    let (provider, _) = parse_model_id(model);
    provider
}

pub fn is_openai_model(model: &str) -> bool {
    let model_lower = model.to_lowercase();
    model_lower.starts_with("gpt-") || model_lower.starts_with("o1") || model_lower.starts_with("o3")
}

/// Build a provider from a model string (deprecated, use ProviderFactory::build instead)
#[deprecated(note = "Use ProviderFactory::build instead")]
pub fn build_provider_from_model(model: &str, config: Option<&RcodeConfig>) -> Result<(Arc<dyn LlmProvider>, String)> {
    ProviderFactory::build(model, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_retry_with_backoff_success_first_attempt() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);
        
        let result = retry_with_backoff(|| {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            future::ready(Ok(42))
        }).await;
        
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_success_after_retries() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);
        
        let result = retry_with_backoff(|| {
            let curr = attempts_clone.fetch_add(1, Ordering::SeqCst);
            if curr < 2 {
                future::ready(Err(RCodeError::Provider("Transient Network error".into())))
            } else {
                future::ready(Ok(42))
            }
        }).await;
        
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_max_retries_exceeded() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);
        
        let result: Result<i32> = retry_with_backoff(|| {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            future::ready(Err(RCodeError::Provider("Network error".into())))
        }).await;
        
        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_non_transient_error_no_retry() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);
        
        let result: Result<i32> = retry_with_backoff(|| {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            future::ready(Err(RCodeError::Provider("Invalid API key".into())))
        }).await;
        
        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_rate_limit_error_retries() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);
        
        let result = retry_with_backoff(|| {
            let curr = attempts_clone.fetch_add(1, Ordering::SeqCst);
            if curr < 3 {
                future::ready(Err(RCodeError::Provider("RateLimit: too many requests".into())))
            } else {
                future::ready(Ok(42))
            }
        }).await;
        
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_mixed_transient() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);
        
        let result = retry_with_backoff(|| {
            let curr = attempts_clone.fetch_add(1, Ordering::SeqCst);
            match curr {
                0 => future::ready(Err(RCodeError::Provider("Network timeout".into()))),
                1 => future::ready(Err(RCodeError::Provider("Network connection reset".into()))),
                _ => future::ready(Ok(42)),
            }
        }).await;
        
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_parse_model_id_full_format() {
        assert_eq!(
            parse_model_id("anthropic/claude-3-5-sonnet"),
            ("anthropic".to_string(), "claude-3-5-sonnet".to_string())
        );
        assert_eq!(
            parse_model_id("openai/gpt-4o"),
            ("openai".to_string(), "gpt-4o".to_string())
        );
    }

    #[test]
    fn test_parse_model_id_short_format_heuristic_inference() {
        // claude- prefix → anthropic
        assert_eq!(
            parse_model_id("claude-3-5-sonnet"),
            ("anthropic".to_string(), "claude-3-5-sonnet".to_string())
        );
        // gpt- prefix → openai
        assert_eq!(
            parse_model_id("gpt-4o"),
            ("openai".to_string(), "gpt-4o".to_string())
        );
        // gemini- prefix → google
        assert_eq!(
            parse_model_id("gemini-pro"),
            ("google".to_string(), "gemini-pro".to_string())
        );
        // truly unknown bare ID
        assert_eq!(
            parse_model_id("MiniMax-M2.7-highspeed"),
            ("unknown".to_string(), "MiniMax-M2.7-highspeed".to_string())
        );
    }

    #[test]
    fn test_is_openai_model() {
        assert!(is_openai_model("gpt-4o"));
        assert!(is_openai_model("GPT-4"));
        assert!(is_openai_model("o1-preview"));
        assert!(is_openai_model("o1-mini"));
        assert!(is_openai_model("o3"));
        assert!(!is_openai_model("claude-3-5-sonnet"));
        assert!(!is_openai_model("gemini-pro"));
    }

    #[test]
    fn test_model_from_model_string() {
        assert_eq!(model_from_model_string("anthropic/claude-3-5-sonnet"), "claude-3-5-sonnet");
        assert_eq!(model_from_model_string("gpt-4o"), "gpt-4o");
        assert_eq!(model_from_model_string("claude-sonnet-4-5"), "claude-sonnet-4-5");
    }

    #[test]
    fn test_provider_id_from_model() {
        assert_eq!(provider_id_from_model("anthropic/claude-3-5-sonnet"), "anthropic");
        assert_eq!(provider_id_from_model("openai/gpt-4o"), "openai");
        assert_eq!(provider_id_from_model("gpt-4o"), "openai");
        assert_eq!(provider_id_from_model("claude-3-5-sonnet"), "anthropic");
    }

    #[test]
    fn test_expand_env_var() {
        // SAFETY: Test-only environment variable manipulation
        unsafe {
            std::env::set_var("TEST_API_KEY", "secret123");
            assert_eq!(expand_env_var("${TEST_API_KEY}"), "secret123");
            assert_eq!(expand_env_var("plain-key"), "plain-key");
            std::env::remove_var("TEST_API_KEY");
        }
    }

    #[tokio::test]
    async fn test_retry_with_backoff_all_failures_returns_last_error() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);
        
        let result: Result<i32> = retry_with_backoff(|| {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            future::ready(Err(RCodeError::Provider("Final error".into())))
        }).await;
        
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Final error") || err_msg.contains("Max retries"));
    }
}
