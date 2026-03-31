//! Session management

pub mod compaction;
pub mod compaction_service;
pub mod service;
pub mod summarizer;

pub use compaction::{CompactionConfig, CompactionResult, CompactionStrategy};
pub use compaction_service::{CompactionService, CompactionTrigger};
pub use service::SessionService;
pub use summarizer::Summarizer;
