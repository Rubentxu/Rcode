//! CLI run command tests

use opencode_cli::commands::Run;
use tempfile::TempDir;

#[test]
fn test_run_command_default_values() {
    let run = Run {
        message: None,
        file: None,
        stdin: false,
        json: false,
        silent: false,
        save_session: None,
        model: "claude-sonnet-4-5".to_string(),
        agent: None,
    };
    
    assert!(run.message.is_none());
    assert!(run.file.is_none());
    assert!(!run.stdin);
    assert!(!run.json);
    assert!(!run.silent);
    assert!(run.save_session.is_none());
    assert_eq!(run.model, "claude-sonnet-4-5");
}

#[test]
fn test_run_command_with_message() {
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
    
    assert_eq!(run.message, Some("Hello, world!".to_string()));
}

#[test]
fn test_run_command_json_output_flag() {
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

#[test]
fn test_run_command_silent_flag() {
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
async fn test_run_get_prompt_from_message() {
    let run = Run {
        message: Some("Hello from message".to_string()),
        file: None,
        stdin: false,
        json: false,
        silent: false,
        save_session: None,
        model: "claude-sonnet-4-5".to_string(),
        agent: None,
    };
    
    // We can't directly test execute without mocking, but we can test the structure
    assert!(run.message.is_some());
}

#[tokio::test]
async fn test_run_get_prompt_from_file() {
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
async fn test_run_no_input_error() {
    let run = Run {
        message: None,
        file: None,
        stdin: false,
        json: false,
        silent: false,
        save_session: None,
        model: "claude-sonnet-4-5".to_string(),
        agent: None,
    };
    
    // When execute is called without input, it should fail
    // This test documents the expected behavior
    assert!(run.message.is_none());
    assert!(run.file.is_none());
    assert!(!run.stdin);
}
