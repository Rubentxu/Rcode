//! Bash tool implementation

use std::process::Stdio;
use async_trait::async_trait;
use tokio::process::Command;
use tokio::io::AsyncReadExt;

use opencode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct BashTool {
    max_timeout_ms: u64,
}

impl BashTool {
    pub fn new() -> Self {
        Self { max_timeout_ms: 300_000 }
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BashTool {
    fn id(&self) -> &str { "bash" }
    fn name(&self) -> &str { "Bash" }
    fn description(&self) -> &str { "Execute shell commands" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                }
            },
            "required": ["command"]
        })
    }
    
    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'command' argument".into()))?;
        
        let cwd = context.cwd.clone();
        
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to execute: {}", e)))?;
        
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        
        Ok(ToolResult {
            title: format!("Bash: {}", &command[..command.len().min(50)]),
            content: if stdout.is_empty() { stderr.clone() } else { stdout },
            metadata: Some(serde_json::json!({
                "exit_code": output.status.code(),
                "stderr": stderr,
            })),
            attachments: vec![],
        })
    }
}
