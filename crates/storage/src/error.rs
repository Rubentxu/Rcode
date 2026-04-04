//! Error types for storage operations

use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),

    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),

    #[error("Failed to create directory: {0}")]
    DirectoryCreation(String),
}
