//! Terminal execution routes

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::error::ServerError;

/// Command allow-list for terminal execution
const ALLOWED_COMMANDS: &[&str] = &[
    "ls", "cat", "pwd", "echo", "head", "tail", "wc", "grep", "find", 
    "git", "diff", "sort", "uniq", "xxd", "file", "which", "env", 
    "printenv", "date", "whoami", "id", "uname",
];

/// Shell operators that are not allowed
const SHELL_OPERATORS: &[char] = &['|', ';', '&', '>', '<', '`'];

/// Request body for terminal execution
#[derive(Debug, Deserialize)]
pub struct ExecRequest {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// Response body for terminal execution
#[derive(Debug, Serialize)]
pub struct ExecResponse {
    pub output: String,
    pub exit_code: i32,
}

/// Forbidden response for disallowed commands or shell operators
#[derive(Debug, Serialize)]
pub struct ForbiddenResponse {
    pub code: String,
    pub message: String,
}

impl IntoResponse for ForbiddenResponse {
    fn into_response(self) -> Response {
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::to_value(self).unwrap()),
        ).into_response()
    }
}

/// Timeout response for long-running commands
#[derive(Debug, Serialize)]
pub struct TimeoutResponse {
    pub code: String,
    pub message: String,
}

impl IntoResponse for TimeoutResponse {
    fn into_response(self) -> Response {
        (
            StatusCode::REQUEST_TIMEOUT,
            Json(serde_json::to_value(self).unwrap()),
        ).into_response()
    }
}

/// Check if command contains shell operators
fn contains_shell_operators(command: &str) -> bool {
    SHELL_OPERATORS.iter().any(|op| command.contains(*op))
}

/// Extract the binary name from a command string
fn extract_binary_name(command: &str) -> Option<String> {
    command.split_whitespace().next().map(|s| s.to_string())
}

/// Check if a binary is in the allow-list
fn is_command_allowed(binary: &str) -> bool {
    ALLOWED_COMMANDS.iter().any(|&cmd| cmd == binary)
}

/// POST /terminal/exec - Execute a terminal command
pub async fn exec_terminal_command(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResponse>, ServerError> {
    // Validate command is not empty
    if req.command.trim().is_empty() {
        return Err(ServerError::bad_request("Command cannot be empty"));
    }

    // Check for shell operators
    if contains_shell_operators(&req.command) {
        return Err(ServerError::forbidden("Shell operators are not allowed"));
    }

    // Extract binary name and check against allow-list
    let binary = extract_binary_name(&req.command)
        .ok_or_else(|| ServerError::bad_request("Invalid command"))?;
    
    if !is_command_allowed(&binary) {
        return Err(ServerError::forbidden(&format!(
            "Command '{}' is not allowed. Allowed commands: {}",
            binary,
            ALLOWED_COMMANDS.join(", ")
        )));
    }

    // Parse command into binary and arguments
    let mut parts = req.command.split_whitespace();
    let cmd_binary = parts.next().unwrap();
    let args: Vec<&str> = parts.collect();

    // Execute the command with timeout
    let output_result = timeout(
        Duration::from_secs(30),
        async {
            let mut cmd = Command::new(cmd_binary);
            cmd.args(&args);
            
            // Set working directory if provided
            if let Some(cwd) = &req.cwd {
                cmd.current_dir(cwd);
            }
            
            cmd.output().await
        }
    ).await;

    match output_result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);
            
            // Combine stdout and stderr for output
            let combined_output = if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n{}", stdout, stderr)
            };
            
            Ok(Json(ExecResponse {
                output: combined_output,
                exit_code,
            }))
        }
        Ok(Err(e)) => {
            Err(ServerError::internal(format!("Failed to execute command: {}", e)))
        }
        Err(_) => {
            // Timeout
            Err(ServerError::request_timeout("Command timed out after 30 seconds"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_shell_operators_pipe() {
        assert!(contains_shell_operators("ls | grep foo"));
    }

    #[test]
    fn test_contains_shell_operators_semicolon() {
        assert!(contains_shell_operators("ls ; cat file"));
    }

    #[test]
    fn test_contains_shell_operators_ampersand() {
        assert!(contains_shell_operators("ls & cat file"));
    }

    #[test]
    fn test_contains_shell_operators_redirect() {
        assert!(contains_shell_operators("ls > output.txt"));
    }

    #[test]
    fn test_contains_shell_operators_backtick() {
        assert!(contains_shell_operators("ls `cat file`"));
    }

    #[test]
    fn test_contains_shell_operators_none() {
        assert!(!contains_shell_operators("ls -la"));
        assert!(!contains_shell_operators("git status"));
        assert!(!contains_shell_operators("echo hello world"));
    }

    #[test]
    fn test_extract_binary_name() {
        assert_eq!(extract_binary_name("ls -la"), Some("ls".to_string()));
        assert_eq!(extract_binary_name("git status"), Some("git".to_string()));
        assert_eq!(extract_binary_name("echo hello world"), Some("echo".to_string()));
        assert_eq!(extract_binary_name(""), None);
        assert_eq!(extract_binary_name("  "), None);
    }

    #[test]
    fn test_is_command_allowed() {
        assert!(is_command_allowed("ls"));
        assert!(is_command_allowed("cat"));
        assert!(is_command_allowed("git"));
        assert!(is_command_allowed("diff"));
        assert!(!is_command_allowed("rm"));
        assert!(!is_command_allowed("sudo"));
        assert!(!is_command_allowed("bash"));
        assert!(!is_command_allowed("python"));
    }

    #[test]
    fn test_allowed_commands_list() {
        // Verify all expected commands are in the allow-list
        let expected = vec![
            "ls", "cat", "pwd", "echo", "head", "tail", "wc", "grep", 
            "find", "git", "diff", "sort", "uniq", "xxd", "file", 
            "which", "env", "printenv", "date", "whoami", "id", "uname"
        ];
        
        for cmd in &expected {
            assert!(
                ALLOWED_COMMANDS.contains(&cmd),
                "Command '{}' should be in allow-list",
                cmd
            );
        }
        
        assert_eq!(ALLOWED_COMMANDS.len(), expected.len(), "Allow-list should have exactly {} commands", expected.len());
    }
}
