//! Default agent implementation

use async_trait::async_trait;
use rcode_core::agent::{Agent, AgentContext, AgentResult, StopReason};
use rcode_core::error::Result;

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
        r#"You are a helpful AI coding assistant. You have access to tools and you should use them when the user asks for concrete system, shell, filesystem, search, or codebase actions.

When the user asks for something like `pwd`, `ls`, reading files, searching text, or making changes, prefer using the appropriate tool instead of answering from memory.
If a request is a direct shell-style action, use the bash tool unless a more specific tool is better.
When using tools, explain briefly what you are doing.
If a tool fails, surface the failure clearly and try the next sensible approach.
Always aim to provide the most helpful and accurate response possible."#.to_string()
    }

    pub fn system_prompt_text() -> String {
        Self::default_system_prompt()
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
                rcode_core::Message::assistant(ctx.session_id.clone(), vec![])
            }),
            should_continue: false,
            stop_reason: StopReason::EndOfTurn,
            usage: None,
        })
    }
    
    fn system_prompt(&self) -> String {
        self.system_prompt.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{Message, Part};
    use std::path::PathBuf;

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

    #[tokio::test]
    async fn test_default_agent_run_with_messages() {
        let agent = DefaultAgent::new();
        let mut ctx = rcode_core::AgentContext {
            session_id: "test-session".to_string(),
            project_path: PathBuf::from("/tmp"),
            cwd: PathBuf::from("/tmp"),
            user_id: None,
            model_id: "claude-sonnet-4-5".to_string(),
            messages: vec![
                Message::user("test-session".to_string(), vec![
                    Part::Text { content: "Hello".to_string() }
                ]),
            ],
        };
        
        let result = agent.run(&mut ctx).await.unwrap();
        
        assert!(!result.should_continue);
        assert!(matches!(result.stop_reason, StopReason::EndOfTurn));
        // Message should be the last one - which is the user message since that's what was added
        assert_eq!(result.message.role, rcode_core::message::Role::User);
    }

    #[tokio::test]
    async fn test_default_agent_run_with_empty_messages() {
        let agent = DefaultAgent::new();
        let mut ctx = rcode_core::AgentContext {
            session_id: "test-session".to_string(),
            project_path: PathBuf::from("/tmp"),
            cwd: PathBuf::from("/tmp"),
            user_id: None,
            model_id: "claude-sonnet-4-5".to_string(),
            messages: vec![],
        };
        
        let result = agent.run(&mut ctx).await.unwrap();
        
        assert!(!result.should_continue);
        // Should create an empty assistant message
        assert_eq!(result.message.role, rcode_core::message::Role::Assistant);
        assert!(result.message.parts.is_empty());
    }

    #[test]
    fn test_default_agent_description() {
        let agent = DefaultAgent::new();
        assert_eq!(agent.description(), "A helpful AI coding assistant");
    }

    #[test]
    fn test_default_agent_default_impl() {
        let agent = DefaultAgent::default();
        assert_eq!(agent.id(), "default");
    }

    #[test]
    fn test_default_agent_supported_tools() {
        let agent = DefaultAgent::new();
        // Default returns empty vec
        let tools = agent.supported_tools();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_default_agent_is_hidden() {
        let agent = DefaultAgent::new();
        // Default returns false
        assert!(!agent.is_hidden());
    }

    #[test]
    fn test_default_agent_system_prompt_contains_guidance() {
        let agent = DefaultAgent::new();
        let prompt = agent.system_prompt();
        assert!(prompt.contains("helpful"));
        assert!(prompt.contains("tools"));
    }
}
