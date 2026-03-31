//! Session management

pub mod compaction;
pub mod service;
pub mod summarizer;

pub use compaction::{CompactionConfig, CompactionResult, CompactionStrategy};
pub use service::SessionService;
pub use summarizer::Summarizer;
