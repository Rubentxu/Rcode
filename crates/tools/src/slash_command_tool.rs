//! Slash command tool - executes slash commands

use std::sync::Arc;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

/// Parameters for executing a slash command
#[derive(Debug, Deserialize, Serialize)]
pub struct CommandParams {
    pub name: String,
}

/// Tool for executing slash commands
pub struct SlashCommandTool {
    registry: Arc<super::command_registry::CommandRegistry>,
}

impl SlashCommandTool {
    /// Create a new SlashCommandTool with the given registry
    pub fn new(registry: Arc<super::command_registry::CommandRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for SlashCommandTool {
    fn id(&self) -> &str {
        "slash_command"
    }

    fn name(&self) -> &str {
        "Execute Slash Command"
    }

    fn description(&self) -> &str {
        "Execute a slash command to get specialized instructions"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the slash command to execute"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, args: Value, _context: &ToolContext) -> Result<ToolResult> {
        let params: CommandParams = serde_json::from_value(args)
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Invalid arguments: {}", e)))?;

        let command = self.registry.get(&params.name)
            .ok_or_else(|| rcode_core::RCodeError::Tool(
                format!("Command '{}' not found. Available commands: {:?}", 
                    params.name, 
                    self.registry.list_all().iter().map(|c| &c.name).collect::<Vec<_>>()
                )
            ))?;

        Ok(ToolResult {
            title: command.name.clone(),
            content: command.instructions.clone(),
            metadata: Some(serde_json::json!({
                "command_name": command.name,
                "description": command.description,
            })),
            attachments: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::SlashCommand;
    use crate::CommandRegistry;

    fn test_context() -> ToolContext {
        ToolContext {
            session_id: "test".to_string(),
            project_path: std::path::PathBuf::from("/tmp"),
            cwd: std::path::PathBuf::from("/tmp"),
            user_id: None,
            agent: "test-agent".to_string(),
        }
    }

    fn test_command(name: &str, description: &str, instructions: &str) -> SlashCommand {
        SlashCommand {
            name: name.to_string(),
            description: description.to_string(),
            instructions: instructions.to_string(),
        }
    }

    #[tokio::test]
    async fn test_execute_command() {
        let registry = Arc::new(CommandRegistry::new());
        registry.register(test_command(
            "greet",
            "Greet the user",
            "# Greeting\n\nHello, user!"
        ));
        
        let tool = SlashCommandTool::new(registry);
        let args = serde_json::json!({ "name": "greet" });
        
        let result = tool.execute(args, &test_context()).await.unwrap();
        assert_eq!(result.title, "greet");
        assert!(result.content.contains("Greeting"));
    }

    #[tokio::test]
    async fn test_command_not_found() {
        let registry = Arc::new(CommandRegistry::new());
        let tool = SlashCommandTool::new(registry);
        let args = serde_json::json!({ "name": "nonexistent" });
        
        let result = tool.execute(args, &test_context()).await;
        assert!(result.is_err());
    }
}