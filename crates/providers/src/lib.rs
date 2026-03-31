//! AI Provider abstractions and implementations

pub mod provider_trait;
pub mod anthropic;
pub mod openai;
pub mod registry;
pub mod mock;
pub mod credentials;

pub use provider_trait::LlmProvider;
pub use registry::ProviderRegistry;
pub use mock::MockLlmProvider;
pub use credentials::{load_api_key, resolve_api_key};

use std::future::Future;
use std::time::Duration;
use opencode_core::error::{OpenCodeError, Result};

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
                // Check if error is transient (Network or RateLimit)
                let is_transient = matches!(&e, 
                    OpenCodeError::Provider(p) if p.contains("Network") || p.contains("RateLimit")
                );
                
                if !is_transient || attempt == max_retries - 1 {
                    return Err(e);
                }
                
                last_error = Some(e);
                
                // Exponential backoff: 1s, 2s, 4s, 8s, max 32s
                let delay = std::cmp::min(
                    base_delay * 2u32.pow(attempt as u32),
                    max_delay
                );
                
                tokio::time::sleep(delay).await;
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| 
        OpenCodeError::Provider("Max retries exceeded".into())
    ))
}
