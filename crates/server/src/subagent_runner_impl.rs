//! Server-side implementation of SubagentRunner
//!
//! This implementation lives in rcode-server because it needs access to:
//! - RcodeConfig for model resolution
//! - ProviderFactory for building providers
//! - AgentExecutor for running the agent
//! - SessionService for session management

use std::sync::Arc;
use async_trait::async_trait;

use rcode_core::{
    SubagentRunner, SubagentResult, AgentContext, Message, Part, Role,
    error::RCodeError,
};
use rcode_agent::{AgentExecutor, DefaultAgent};
use rcode_event::EventBus;
use rcode_providers::ProviderFactory;
use rcode_session::SessionService;
use rcode_tools::{ToolRegistryService, task::TASK_SYSTEM_PROMPT};
use crate::state::AppState;

/// Server implementation of SubagentRunner
/// 
/// This delegates actual agent execution to the server's composition root,
/// using the same provider building and execution patterns as submit_prompt().
pub struct ServerSubagentRunner {
    session_service: Arc<SessionService>,
    event_bus: Arc<EventBus>,
    config: Arc<std::sync::Mutex<rcode_core::RcodeConfig>>,
    tools: Arc<ToolRegistryService>,
    cognicode_service: Option<Arc<std::sync::Mutex<Option<rcode_cognicode::service::CogniCodeService>>>>,
}

impl ServerSubagentRunner {
    /// Create a new ServerSubagentRunner from AppState
    pub fn new(state: &Arc<AppState>) -> Self {
        Self {
            session_service: Arc::clone(&state.session_service),
            event_bus: Arc::clone(&state.event_bus),
            config: Arc::clone(&state.config),
            tools: Arc::clone(&state.tools),
            cognicode_service: Some(Arc::clone(&state.cognicode_service)),
        }
    }

    /// Create a new ServerSubagentRunner with explicit dependencies
    pub fn with_deps(
        session_service: Arc<SessionService>,
        event_bus: Arc<EventBus>,
        config: Arc<std::sync::Mutex<rcode_core::RcodeConfig>>,
        tools: Arc<ToolRegistryService>,
    ) -> Self {
        Self {
            session_service,
            event_bus,
            config,
            tools,
            cognicode_service: None,
        }
    }
}

#[async_trait]
impl SubagentRunner for ServerSubagentRunner {
    async fn run_subagent(
        &self,
        parent_session_id: &str,
        prompt: &str,
        allowed_tools: &[&str],
    ) -> Result<SubagentResult, Box<dyn std::error::Error + Send + Sync>> {
        // Get the session to find project_path
        let session = self.session_service.get(&rcode_core::SessionId(parent_session_id.to_string()))
            .ok_or_else(|| {
                Box::new(RCodeError::Session(format!(
                    "Session '{}' not found for subagent execution", parent_session_id
                ))) as Box<dyn std::error::Error + Send + Sync>
            })?;

        // Resolve model from config
        // Use a block to ensure config_guard is dropped before the async part
        let (provider, effective_model) = {
            let config_guard = self.config.lock().map_err(|e| {
                Box::new(RCodeError::Config(format!("Config lock error: {}", e))) as Box<dyn std::error::Error + Send + Sync>
            })?;

            // Use model_for_agent("task") or fall back to effective_small_model()
            let model_id = config_guard
                .model_for_agent("task")
                .map(|s| s.to_string())
                .or_else(|| config_guard.effective_small_model().map(|s| s.to_string()))
                .unwrap_or_else(|| "claude-sonnet-4-5".to_string());

            // Build provider
            ProviderFactory::build(&model_id, Some(&*config_guard))
                .map_err(|e| {
                    Box::new(RCodeError::Provider(format!("Failed to build provider: {}", e))) as Box<dyn std::error::Error + Send + Sync>
                })?
        }; // config_guard is dropped here, before any async operations

        // Create agent with task system prompt
        let agent: Arc<dyn rcode_core::Agent> = Arc::new(
            DefaultAgent::with_system_prompt(TASK_SYSTEM_PROMPT.to_string())
        );

        // Create executor with allowed tools restriction
        let allowed_tools_vec: Vec<String> = allowed_tools.iter().map(|s| s.to_string()).collect();
        let mut executor = AgentExecutor::new(
            agent,
            provider,
            Arc::clone(&self.tools),
        )
        .with_event_bus(Arc::clone(&self.event_bus))
        .with_allowed_tools(allowed_tools_vec);

        // Inject CogniCode intelligence XML provider for proactive context
        if let Some(ref svc_holder) = self.cognicode_service {
            let svc_clone = Arc::clone(svc_holder);
            let xml_provider: Arc<dyn Fn() -> String + Send + Sync> = Arc::new(move || {
                svc_clone
                    .lock()
                    .ok()
                    .and_then(|guard| guard.as_ref().map(|svc| svc.to_xml()))
                    .unwrap_or_default()
            });
            executor = executor.with_intelligence_xml_provider(xml_provider);
        }

        // Create context for the subagent
        let cwd = std::env::current_dir().unwrap_or_else(|_| session.project_path.clone());
        
        // Create the initial user message
        let user_message = Message::user(parent_session_id.to_string(), vec![Part::Text {
            content: prompt.to_string(),
        }]);
        
        // Persist the user message to the child session BEFORE running the executor
        self.session_service.add_message(parent_session_id, user_message.clone());
        
        let messages = vec![user_message];

        let mut ctx = AgentContext {
            session_id: parent_session_id.to_string(),
            project_path: session.project_path.clone(),
            cwd,
            user_id: None, // Session doesn't track user_id
            model_id: effective_model,
            messages,
        };

        // Run the executor (blocks until complete)
        let result = executor.run(&mut ctx).await.map_err(|e| {
            Box::new(RCodeError::Agent(format!("Agent execution failed: {}", e))) as Box<dyn std::error::Error + Send + Sync>
        })?;

        // Persist all messages from ctx.messages to the child session
        // This includes the user message (already added), tool calls, tool results, and final assistant response
        for msg in &ctx.messages {
            // Skip the first message if it's the same user message we already added
            // (to avoid duplicates - though add_message is idempotent for different messages)
            if msg.role == Role::User && msg.parts.iter().any(|p| matches!(p, Part::Text { content } if content == prompt)) {
                // Already added this user message, skip
                continue;
            }
            self.session_service.add_message(parent_session_id, msg.clone());
        }
        
        // Also persist the final assistant message if it's different from what we already saved
        let final_message = &result.message;
        if !ctx.messages.iter().any(|m| m.id == final_message.id) {
            self.session_service.add_message(parent_session_id, final_message.clone());
        }

        // Extract assistant text from the result
        let response_text = extract_assistant_text(&result.message);
        
        Ok(SubagentResult {
            response_text,
            child_session_id: parent_session_id.to_string(),
        })
    }
}

/// Extract text content from an assistant message
fn extract_assistant_text(message: &Message) -> String {
    let mut text_parts = Vec::new();
    
    for part in &message.parts {
        match part {
            Part::Text { content } => {
                text_parts.push(content.clone());
            }
            Part::ToolCall { name, arguments, .. } => {
                // Include tool calls in the response for debugging/traceability
                text_parts.push(format!("[Tool: {}]\nArgs: {}", name, arguments));
            }
            Part::ToolResult { content, .. } => {
                text_parts.push(format!("[Tool Result]: {}", content));
            }
            Part::Reasoning { content } => {
                // Optionally include reasoning - for now just note it exists
                text_parts.push(format!("[Reasoning]: {}...", content.chars().take(100).collect::<String>()));
            }
            Part::Attachment { name, .. } => {
                text_parts.push(format!("[Attachment: {}]", name));
            }
            Part::TaskChecklist { items } => {
                text_parts.push(format!(
                    "[Checklist]: {}",
                    items
                        .iter()
                        .map(|item| format!("{} ({})", item.content, item.status))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }

    text_parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_assistant_text_with_text() {
        let msg = Message::assistant("session1".to_string(), vec![
            Part::Text { content: "Hello world".to_string() },
        ]);
        assert_eq!(extract_assistant_text(&msg), "Hello world");
    }

    #[test]
    fn test_extract_assistant_text_with_tool_calls() {
        let msg = Message::assistant("session1".to_string(), vec![
            Part::Text { content: "Let me search for that".to_string() },
            Part::ToolCall {
                id: "call_1".to_string(),
                name: "grep".to_string(),
                arguments: Box::new(serde_json::json!({"pattern": "test"})),
            },
        ]);
        let text = extract_assistant_text(&msg);
        assert!(text.contains("Let me search for that"));
        assert!(text.contains("grep"));
    }
}
