//! End-to-end tests for CLI

use std::sync::Arc;
use tempfile::TempDir;
use rcode_cli::commands::Run;

/// Helper to create a test app state (simplified for e2e tests)
async fn create_test_app_state() -> Arc<rcode_server::AppState> {
    let config = rcode_core::RcodeConfig::default();
    Arc::new(rcode_server::AppState::with_config(config))
}

#[tokio::test]
async fn test_run_command_struct_creation() {
    // Test that Run command struct can be created with various options
    let run = Run {
        message: Some("Hello, world!".to_string()),
        file: None,
        stdin: false,
        json: false,
        silent: false,
        save_session: None,
        model: "claude-sonnet-4-5".to_string(),
        agent: None,
    };
    
    assert!(run.message.is_some());
    assert_eq!(run.message.unwrap(), "Hello, world!");
    assert_eq!(run.model, "claude-sonnet-4-5");
}

#[tokio::test]
async fn test_run_command_with_file() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("prompt.txt");
    std::fs::write(&file_path, "Hello from file").unwrap();
    
    let run = Run {
        message: None,
        file: Some(file_path.to_string_lossy().to_string()),
        stdin: false,
        json: false,
        silent: false,
        save_session: None,
        model: "claude-sonnet-4-5".to_string(),
        agent: None,
    };
    
    assert!(run.file.is_some());
}

#[tokio::test]
async fn test_run_command_with_json_flag() {
    let run = Run {
        message: Some("test".to_string()),
        file: None,
        stdin: false,
        json: true,
        silent: false,
        save_session: None,
        model: "claude-sonnet-4-5".to_string(),
        agent: None,
    };
    
    assert!(run.json);
}

#[tokio::test]
async fn test_run_command_with_silent_flag() {
    let run = Run {
        message: Some("test".to_string()),
        file: None,
        stdin: false,
        json: false,
        silent: true,
        save_session: None,
        model: "claude-sonnet-4-5".to_string(),
        agent: None,
    };
    
    assert!(run.silent);
}

#[tokio::test]
async fn test_run_command_save_session_defaults() {
    let run = Run {
        message: Some("test".to_string()),
        file: None,
        stdin: false,
        json: false,
        silent: false,
        save_session: None,
        model: "claude-sonnet-4-5".to_string(),
        agent: None,
    };
    
    // save_session defaults to None (which means true in execute)
    assert!(run.save_session.is_none());
}

#[tokio::test]
async fn test_run_command_save_session_explicit() {
    let run = Run {
        message: Some("test".to_string()),
        file: None,
        stdin: false,
        json: false,
        silent: false,
        save_session: Some(false),
        model: "claude-sonnet-4-5".to_string(),
        agent: None,
    };
    
    assert_eq!(run.save_session, Some(false));
}

// Note: Full e2e tests with actual API calls would require ANTHROPIC_API_KEY
// These tests verify the structure but don't make actual API calls

#[tokio::test]
async fn test_app_state_creation() {
    // Test that AppState can be created (basic smoke test)
    let state = create_test_app_state().await;
    
    assert!(state.session_service.list_all().is_empty());
}
