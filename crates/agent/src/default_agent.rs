//! Default agent implementation

use async_trait::async_trait;
use opencode_core::agent::{Agent, AgentContext, AgentResult, StopReason};
use opencode_core::error::Result;

/// Default agent with basic behavior
pub struct DefaultAgent {
    system_prompt: String,
}

impl DefaultAgent {
    pub fn new() -> Self {
        Self {
            system_prompt: Self::default_system_prompt(),
        }
    }
    
    pub fn with_system_prompt(prompt: impl Into<String>) -> Self {
        Self {
            system_prompt: prompt.into(),
        }
    }
    
    fn default_system_prompt() -> String {
        r#"You are a helpful AI coding assistant. You have access to various tools to help you complete tasks.
When using tools, make sure to provide clear descriptions of what you're doing.
If a tool fails, try to understand why and suggest alternatives.
Always aim to provide the most helpful and accurate response possible."#.to_string()
    }
}

impl Default for DefaultAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for DefaultAgent {
    fn id(&self) -> &str {
        "default"
    }
    
    fn name(&self) -> &str {
        "Default Agent"
    }
    
    fn description(&self) -> &str {
        "A helpful AI coding assistant"
    }
    
    async fn run(&self, ctx: &mut AgentContext) -> Result<AgentResult> {
        // The actual agent logic is in the executor
        // This method is not called directly when using AgentExecutor
        Ok(AgentResult {
            message: ctx.messages.last().cloned().unwrap_or_else(|| {
                opencode_core::Message::assistant(ctx.session_id.clone(), vec![])
            }),
            should_continue: false,
            stop_reason: StopReason::EndOfTurn,
        })
    }
    
    fn system_prompt(&self) -> String {
        self.system_prompt.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_agent_creation() {
        let agent = DefaultAgent::new();
        assert_eq!(agent.id(), "default");
        assert_eq!(agent.name(), "Default Agent");
    }

    #[test]
    fn test_default_agent_with_custom_prompt() {
        let agent = DefaultAgent::with_system_prompt("Custom prompt");
        assert_eq!(agent.system_prompt(), "Custom prompt");
    }
}
