//! Session management
#![allow(
    clippy::collapsible_if,
    clippy::redundant_closure,
    clippy::option_map_unit_fn,
    clippy::unwrap_or_default,
    clippy::unnecessary_filter_map,
    clippy::needless_borrow,
    clippy::field_reassign_with_default,
    clippy::items_after_test_module,
    clippy::absurd_extreme_comparisons,
    unused_comparisons,
    clippy::clone_on_copy,
    unused_variables,
    unused_imports
)]

pub mod compaction;
pub mod compaction_service;
pub mod service;
pub mod summarizer;
pub mod title_generator;

pub use compaction::{CompactionConfig, CompactionResult, CompactionStrategy};
pub use compaction_service::{CompactionService, CompactionTrigger};
pub use service::SessionService;
pub use summarizer::Summarizer;
pub use title_generator::TitleGenerator;

use rcode_event::EventBus;
use rcode_storage::{SessionRepository, MessageRepository};
use rusqlite::Connection;
use std::sync::Arc;

/// Create a SessionService backed by SQLite storage at ~/.local/share/rcode/rcode.db
/// Falls back to in-memory service on any error with a warning printed to stderr.
pub fn create_default_session_service(event_bus: Arc<EventBus>) -> SessionService {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("rcode");
    
    // Create directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!("Warning: Failed to create data directory {:?}: {}", data_dir, e);
        return SessionService::new(event_bus);
    }
    
    let db_path = data_dir.join("rcode.db");
    
    // Open database connection and initialize schema
    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: Failed to open database at {:?}: {}", db_path, e);
            return SessionService::new(event_bus);
        }
    };
    
    if let Err(e) = rcode_storage::schema::init_schema(&conn) {
        eprintln!("Warning: Failed to initialize database schema: {}", e);
        return SessionService::new(event_bus);
    }
    
    let session_repo = SessionRepository::new(conn);
    
    // Open a second connection for messages (SQLite best practice for concurrent access)
    let message_conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: Failed to open message database connection: {}", e);
            return SessionService::new(event_bus);
        }
    };
    let message_repo = MessageRepository::new(message_conn);
    
    SessionService::with_storage(event_bus, session_repo, message_repo)
}
