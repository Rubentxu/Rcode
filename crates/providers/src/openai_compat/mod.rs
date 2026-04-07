//! OpenAI-compatible protocol adapter
//!
//! This module provides a shared OpenAI-compatible protocol adapter that can be
//! consumed by thin façades for OpenAI, MiniMax, OpenRouter, and ZAI providers.
//!
//! # Phased Implementation
//!
//! - **Phase 1 (config)**: `OpenAiCompatConfig` struct with static configuration
//! - **Phase 2 (request)**: Request codec: message/tool conversion and serialization
//! - **Phase 3 (response)**: Response codec: parsing SSE events and JSON payloads
//! - **Phase 4 (transport)**: Transport layer: HTTP client, auth, streaming, cancellation
//!
//! # Architecture
//!
//! The adapter is a **stateless codec+transport layer** — it handles protocol
//! encoding/decoding and HTTP transport but does NOT implement `LlmProvider`.
//! Each façade adds its identity (provider_id, model_info, capabilities) on top.
//!
//! # Example Usage (after Phase 4)
//!
//! ```ignore
//! use crate::openai_compat::{OpenAiCompatConfig, OpenAiCompatTransport};
//!
//! let config = OpenAiCompatConfig::new(
//!     "api-key".to_string(),
//!     "https://api.openai.com".to_string(),
//!     "openai".to_string(),
//! );
//!
//! let transport = OpenAiCompatTransport::new(config);
//! ```

pub mod config;
pub mod request;
pub mod response;
pub mod transport;

// Re-export for convenience
pub use config::OpenAiCompatConfig;
pub use transport::OpenAiCompatTransport;
