//! Dynamic agent implementation from agent definitions

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::{Agent, AgentContext, AgentResult, StopReason};
use crate::agent_definition::AgentDefinition;
use crate::error::Result;

/// Dynamic agent created from an AgentDefinition
pub struct DynamicAgent {
    definition: AgentDefinition,
}

impl DynamicAgent {
    /// Create a new DynamicAgent from a definition
    pub fn from_definition(definition: AgentDefinition) -> Arc<Self> {
        Arc::new(Self { definition })
    }

    /// Get a reference to the underlying definition
    pub fn definition(&self) -> &AgentDefinition {
        &self.definition
    }
}

#[async_trait]
impl Agent for DynamicAgent {
    fn id(&self) -> &str {
        &self.definition.identifier
    }

    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn system_prompt(&self) -> String {
        self.definition.system_prompt.clone()
    }

    fn supported_tools(&self) -> Vec<String> {
        self.definition.tools.clone()
    }
    
    fn is_hidden(&self) -> bool {
        self.definition.hidden
    }

    async fn run(&self, ctx: &mut AgentContext) -> Result<AgentResult> {
        // Dynamic agents delegate to the executor
        // The actual execution logic is handled by AgentExecutor
        // This method exists to satisfy the Agent trait
        Ok(AgentResult {
            message: ctx.messages.last().cloned().unwrap_or_else(|| {
                crate::Message::assistant(ctx.session_id.clone(), vec![])
            }),
            should_continue: false,
            stop_reason: StopReason::EndOfTurn,
            usage: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_definition() -> AgentDefinition {
        AgentDefinition {
            identifier: "test-dynamic".to_string(),
            name: "Test Dynamic Agent".to_string(),
            description: "A dynamic test agent".to_string(),
            when_to_use: "For testing dynamic agents".to_string(),
            system_prompt: "You are a dynamic test agent.".to_string(),
            mode: crate::agent_definition::AgentMode::All,
            hidden: false,
            permission: Default::default(),
            tools: vec!["read".to_string(), "write".to_string()],
            model: Some("claude-sonnet-4".to_string()),
        }
    }

    #[test]
    fn test_dynamic_agent_from_definition() {
        let definition = create_test_definition();
        let agent = DynamicAgent::from_definition(definition.clone());

        assert_eq!(agent.id(), "test-dynamic");
        assert_eq!(agent.name(), "Test Dynamic Agent");
        assert_eq!(agent.description(), "A dynamic test agent");
        assert_eq!(agent.system_prompt(), "You are a dynamic test agent.");
        assert_eq!(agent.supported_tools(), vec!["read", "write"]);
    }

    #[test]
    fn test_dynamic_agent_hidden_flag() {
        let mut definition = create_test_definition();
        definition.hidden = true;
        let agent = DynamicAgent::from_definition(definition);

        // The hidden flag is on the definition, accessible via definition()
        assert!(agent.definition().hidden);
    }
}