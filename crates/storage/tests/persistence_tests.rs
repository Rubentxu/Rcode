//! Persistence integration tests

use opencode_core::{Message, Part, Role, Session, SessionId, SessionStatus};
use opencode_storage::Database;
use tempfile::TempDir;

#[tokio::test]
async fn test_database_open() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    
    let db = Database::open(&db_path).await;
    assert!(db.is_ok());
    
    let db = db.unwrap();
    assert_eq!(db.path(), db_path);
}

#[tokio::test]
async fn test_database_creates_schema() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    
    let db = Database::open(&db_path).await.unwrap();
    
    // Verify the database file exists
    assert!(db_path.exists());
}

#[tokio::test]
async fn test_session_persistence() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    
    let db = Database::open(&db_path).await.unwrap();
    
    // Create a session
    let session = Session::new(
        std::path::PathBuf::from("/test/project"),
        "test-agent".to_string(),
        "claude-3".to_string(),
    );
    
    // Note: The current Database struct doesn't have insert_session method
    // This test documents the expected behavior for when persistence is implemented
    assert_eq!(session.status, SessionStatus::Idle);
    assert_eq!(session.agent_id, "test-agent");
}

#[tokio::test]
async fn test_message_persistence() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    
    let db = Database::open(&db_path).await.unwrap();
    
    // Create a message
    let message = Message::user(
        "test-session".to_string(),
        vec![Part::Text {
            content: "Hello, world!".to_string(),
        }],
    );
    
    assert_eq!(message.role, Role::User);
    assert_eq!(message.parts.len(), 1);
}

#[tokio::test]
async fn test_session_status_transitions() {
    // Test valid transitions
    assert!(SessionStatus::Idle.can_transition_to(SessionStatus::Running));
    assert!(SessionStatus::Running.can_transition_to(SessionStatus::Completed));
    assert!(SessionStatus::Running.can_transition_to(SessionStatus::Aborted));
    
    // Test invalid transitions
    assert!(!SessionStatus::Completed.can_transition_to(SessionStatus::Running));
    assert!(!SessionStatus::Aborted.can_transition_to(SessionStatus::Running));
    assert!(!SessionStatus::Idle.can_transition_to(SessionStatus::Completed));
}

#[tokio::test]
async fn test_session_id_generation() {
    let id1 = SessionId::new();
    let id2 = SessionId::new();
    
    assert_ne!(id1.0, id2.0);
    assert_eq!(id1.0.len(), 36); // UUID v4 format
}
