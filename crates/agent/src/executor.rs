//! Agent executor - main agent loop with streaming support

use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::interval;
use tokio_stream::StreamExt;
use rcode_core::{
    Agent, AgentContext, AgentResult, Message, Part, ToolContext, error::Result,
    permission::{Permission, PermissionRequest},
};
use rcode_core::agent::StopReason;
use rcode_core::provider::StreamingEvent;
use rcode_providers::LlmProvider;
use rcode_tools::ToolRegistryService;
use rcode_event::EventBus;
use tracing::{info, warn, error};

use super::permissions::{PermissionService, AutoPermissionService};

const MAX_ACCUMULATED_TEXT: usize = 10_000;
const FLUSH_INTERVAL_SECS: u64 = 2;

/// Permission service that denies all tool executions.
/// Used when Ask mode is set but no InteractivePermissionService is provided.
struct DenyAllPermissionService;

#[async_trait::async_trait]
impl PermissionService for DenyAllPermissionService {
    async fn check(&self, _request: &PermissionRequest) -> std::result::Result<bool, String> {
        Err("Ask mode requires InteractivePermissionService — call with_permission_service() before building".to_string())
    }
}

/// CancellationToken for abort support
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for AgentExecutor with fluent configuration
pub struct AgentExecutorBuilder {
    agent: Arc<dyn Agent>,
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistryService>,
    event_bus: Option<Arc<EventBus>>,
    model_override: Option<String>,
    max_tokens_override: Option<u32>,
    reasoning_effort: Option<String>,
    allowed_tools: Option<Vec<String>>,
    agent_definition: Option<Arc<rcode_core::AgentDefinition>>,
    permission_service: Option<Arc<dyn PermissionService>>,
    auto_compact: bool,
    max_messages_before_compact: usize,
    messages_to_keep_after_compact: usize,
}

impl AgentExecutorBuilder {
    pub fn new(agent: Arc<dyn Agent>, provider: Arc<dyn LlmProvider>, tools: Arc<ToolRegistryService>) -> Self {
        Self {
            agent,
            provider,
            tools,
            event_bus: None,
            model_override: None,
            max_tokens_override: None,
            reasoning_effort: None,
            allowed_tools: None,
            agent_definition: None,
            permission_service: None,
            auto_compact: false,
            max_messages_before_compact: 50,
            messages_to_keep_after_compact: 20,
        }
    }
    
    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }
    
    pub fn with_model_override(mut self, model: impl Into<String>) -> Self {
        self.model_override = Some(model.into());
        self
    }
    
    pub fn with_max_tokens_override(mut self, max_tokens: u32) -> Self {
        self.max_tokens_override = Some(max_tokens);
        self
    }
    
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }
    
    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = Some(tools);
        self
    }
    
    /// Apply settings from an AgentDefinition (model override + tool filter)
    pub fn with_definition(mut self, def: Arc<rcode_core::AgentDefinition>) -> Self {
        if let Some(ref model) = def.model {
            self.model_override = Some(model.clone());
        }
        if let Some(max_tokens) = def.max_tokens {
            self.max_tokens_override = Some(max_tokens);
        }
        if let Some(ref reasoning_effort) = def.reasoning_effort {
            self.reasoning_effort = Some(reasoning_effort.clone());
        }
        if !def.tools.is_empty() {
            self.allowed_tools = Some(def.tools.clone());
        }
        
        // Set up permission service based on permission mode
        let perm_mode = def.permission.task.pattern;
        match perm_mode {
            Permission::Allow => {
                self.permission_service = Some(Arc::new(AutoPermissionService::allow()));
            }
            Permission::Deny => {
                self.permission_service = Some(Arc::new(AutoPermissionService::deny()));
            }
            Permission::Ask => {
                // Ask mode requires InteractivePermissionService to be set
                // Use DenyAllPermissionService as fallback - caller must set InteractivePermissionService
                self.permission_service = Some(Arc::new(DenyAllPermissionService));
            }
        }
        
        self.agent_definition = Some(def);
        self
    }
    
    /// Set a custom permission service
    pub fn with_permission_service(mut self, svc: Arc<dyn PermissionService>) -> Self {
        self.permission_service = Some(svc);
        self
    }
    
    /// Enable or disable automatic message compaction
    pub fn with_auto_compact(mut self, enabled: bool) -> Self {
        self.auto_compact = enabled;
        self
    }
    
    /// Set compaction thresholds
    /// - max_before: maximum messages before triggering compaction (default 50)
    /// - keep_after: number of messages to keep after compaction (default 20)
    pub fn with_compaction_thresholds(mut self, max_before: usize, keep_after: usize) -> Self {
        self.max_messages_before_compact = max_before;
        self.messages_to_keep_after_compact = keep_after;
        self
    }
    
    pub fn build(self) -> AgentExecutor {
        AgentExecutor {
            agent: self.agent,
            provider: self.provider,
            tools: self.tools,
            event_bus: self.event_bus,
            model_override: self.model_override,
            max_tokens_override: self.max_tokens_override,
            reasoning_effort: self.reasoning_effort,
            allowed_tools: self.allowed_tools,
            permission_service: self.permission_service,
            auto_compact: self.auto_compact,
            max_messages_before_compact: self.max_messages_before_compact,
            messages_to_keep_after_compact: self.messages_to_keep_after_compact,
        }
    }
}

/// AgentExecutor with streaming and cancellation support
pub struct AgentExecutor {
    agent: Arc<dyn Agent>,
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistryService>,
    event_bus: Option<Arc<EventBus>>,
    model_override: Option<String>,
    max_tokens_override: Option<u32>,
    reasoning_effort: Option<String>,
    allowed_tools: Option<Vec<String>>,
    permission_service: Option<Arc<dyn PermissionService>>,
    auto_compact: bool,
    max_messages_before_compact: usize,
    messages_to_keep_after_compact: usize,
}

impl AgentExecutor {
    pub fn new(
        agent: Arc<dyn Agent>,
        provider: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistryService>,
    ) -> Self {
        Self {
            agent,
            provider,
            tools,
            event_bus: None,
            model_override: None,
            max_tokens_override: None,
            reasoning_effort: None,
            allowed_tools: None,
            permission_service: None,
            auto_compact: false,
            max_messages_before_compact: 50,
            messages_to_keep_after_compact: 20,
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_model_override(mut self, model: String) -> Self {
        self.model_override = Some(model);
        self
    }

    pub fn with_max_tokens_override(mut self, max_tokens: u32) -> Self {
        self.max_tokens_override = Some(max_tokens);
        self
    }

    pub fn with_reasoning_effort(mut self, effort: String) -> Self {
        self.reasoning_effort = Some(effort);
        self
    }

    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = Some(tools);
        self
    }

    pub fn with_permission_service(mut self, svc: Arc<dyn PermissionService>) -> Self {
        self.permission_service = Some(svc);
        self
    }

    /// Enable or disable automatic message compaction
    pub fn with_auto_compact(mut self, enabled: bool) -> Self {
        self.auto_compact = enabled;
        self
    }

    /// Set compaction thresholds
    /// - max_before: maximum messages before triggering compaction (default 50)
    /// - keep_after: number of messages to keep after compaction (default 20)
    pub fn with_compaction_thresholds(mut self, max_before: usize, keep_after: usize) -> Self {
        self.max_messages_before_compact = max_before;
        self.messages_to_keep_after_compact = keep_after;
        self
    }

    /// Run the agent with the given context and cancellation token
    pub async fn run(&self, ctx: &mut AgentContext) -> Result<AgentResult> {
        self.run_with_cancellation(ctx, CancellationToken::new()).await
    }

    /// Run the agent with explicit cancellation token
    pub async fn run_with_cancellation(
        &self,
        ctx: &mut AgentContext,
        cancellation_token: CancellationToken,
    ) -> Result<AgentResult> {
        let mut step_count = 0;
        let max_steps = 100;

        loop {
            // Check for cancellation before starting a new step
            if cancellation_token.is_cancelled() {
                info!("Agent execution cancelled");
                return Ok(AgentResult {
                    message: Message::assistant(ctx.session_id.clone(), vec![Part::Text {
                        content: "Execution cancelled by user".to_string(),
                    }]),
                    should_continue: false,
                    stop_reason: StopReason::UserStopped,
                    usage: None,
                });
            }

            step_count += 1;

            if step_count > max_steps {
                warn!("Maximum steps {} reached", max_steps);
                return Ok(AgentResult {
                    message: Message::assistant(ctx.session_id.clone(), vec![Part::Text {
                        content: "Maximum steps reached".to_string(),
                    }]),
                    should_continue: false,
                    stop_reason: StopReason::MaxSteps,
                    usage: None,
                });
            }

            info!("Starting step {} of agent execution", step_count);

            // Process a single streaming turn
            match self.process_streaming_turn(ctx, &cancellation_token).await {
                Ok(ShouldContinue(should_continue, stop_reason, usage)) => {
                    if !should_continue {
                        return Ok(AgentResult {
                            message: Message::assistant(ctx.session_id.clone(), vec![]),
                            should_continue: false,
                            stop_reason,
                            usage,
                        });
                    }
                    // Continue to next step
                }
                Err(e) => {
                    error!("Error in streaming turn: {}", e);
                    return Ok(AgentResult {
                        message: Message::assistant(ctx.session_id.clone(), vec![Part::Text {
                            content: format!("Error: {}", e),
                        }]),
                        should_continue: false,
                        stop_reason: StopReason::Error,
                        usage: None,
                    });
                }
            }
        }
    }

    async fn process_streaming_turn(
        &self,
        ctx: &mut AgentContext,
        cancellation_token: &CancellationToken,
    ) -> Result<ShouldContinue> {
        let model = self.model_override.as_deref().unwrap_or(&ctx.model_id).to_string();
        
        // Get tools list, applying filter if allowed_tools is set
        let tools_list = self.tools.list();
        let filtered_tools: Vec<_> = if let Some(ref allowed) = self.allowed_tools {
            tools_list.into_iter().filter(|t| allowed.contains(&t.id)).collect()
        } else {
            tools_list
        };
        
        // Auto-compaction: truncate messages if threshold exceeded (simple truncation v1)
        // For v1, we do simple truncation rather than LLM-based summarization
        if self.auto_compact && ctx.messages.len() > self.max_messages_before_compact {
            warn!("Message count {} exceeds threshold {}, performing simple compaction",
                  ctx.messages.len(), self.max_messages_before_compact);
            
            let keep_count = self.messages_to_keep_after_compact;
            if keep_count < ctx.messages.len() {
                // Keep the last N messages and insert a summary marker
                let summary_message = Message::assistant(
                    ctx.session_id.clone(),
                    vec![Part::Text {
                        content: format!(
                            "[Previous {} messages summarized due to length]", 
                            ctx.messages.len() - keep_count
                        )
                    }],
                );
                
                // Truncate to last N messages and prepend summary
                let new_messages: Vec<Message> = std::iter::once(summary_message)
                    .chain(ctx.messages.iter().skip(ctx.messages.len() - keep_count).cloned())
                    .collect();
                ctx.messages = new_messages;
                
                warn!("Compacted messages to {} (summary + last {} messages)", 
                      ctx.messages.len(), keep_count);
            }
        }
        
        let request = rcode_core::CompletionRequest {
            model,
            messages: ctx.messages.clone(), // TODO: CompletionRequest owns messages, cannot take slice without significant refactoring
            system_prompt: Some(self.agent.system_prompt()),
            tools: filtered_tools.into_iter().map(|t| {
                let params = self.tools.get(&t.id)
                    .map(|tool| tool.parameters())
                    .unwrap_or_else(|| serde_json::json!({}));
                rcode_core::ToolDefinition {
                    name: t.id,
                    description: t.description,
                    parameters: params,
                }
            }).collect(),
            temperature: None,
            max_tokens: self.max_tokens_override.or(Some(4096)),
            reasoning_effort: self.reasoning_effort.clone(),
        };

        let response = self.provider.stream(request).await?;

        let mut accumulated_text = Arc::new(String::new());
        let mut accumulated_reasoning = Arc::new(String::new());
        let mut tool_calls = Vec::new();
        let mut active_tool_call: Option<(String, String, String)> = None;
        let mut final_stop_reason = StopReason::EndOfTurn;
        let mut final_usage = None;
        let mut stream_ended = false;
        let mut last_flush_len = 0;

        let mut flush_timer = interval(Duration::from_secs(FLUSH_INTERVAL_SECS));
        let mut stream = response.events;

        loop {
            tokio::select! {
                _ = flush_timer.tick() => {
                    if accumulated_text.len() > last_flush_len || !accumulated_reasoning.is_empty() {
                        last_flush_len = accumulated_text.len();
                        if let Some(event_bus) = &self.event_bus {
                            event_bus.publish(rcode_event::Event::StreamingProgress {
                                session_id: ctx.session_id.clone(),
                                accumulated_text: (*Arc::clone(&accumulated_text)).clone(),
                                accumulated_reasoning: (*Arc::clone(&accumulated_reasoning)).clone(),
                            });
                        }
                    }
                }
                result = stream.next() => {
                    match result {
                        Some(event) => {
                            match event {
                                StreamingEvent::Text { delta } => {
                                    let new_text = Arc::clone(&accumulated_text);
                                    let mut text = (*new_text).clone();
                                    text.push_str(&delta);
                                    accumulated_text = Arc::new(text);
                                    if accumulated_text.len() > MAX_ACCUMULATED_TEXT {
                                        if let Some(event_bus) = &self.event_bus {
                                            event_bus.publish(rcode_event::Event::StreamingProgress {
                                                session_id: ctx.session_id.clone(),
                                                accumulated_text: (*Arc::clone(&accumulated_text)).clone(),
                                                accumulated_reasoning: (*Arc::clone(&accumulated_reasoning)).clone(),
                                            });
                                        }
                                        last_flush_len = accumulated_text.len();
                                    }
                                }
                                StreamingEvent::Reasoning { delta } => {
                                    let new_reasoning = Arc::clone(&accumulated_reasoning);
                                    let mut reasoning = (*new_reasoning).clone();
                                    reasoning.push_str(&delta);
                                    accumulated_reasoning = Arc::new(reasoning);
                                    if accumulated_reasoning.len() > MAX_ACCUMULATED_TEXT {
                                        if let Some(event_bus) = &self.event_bus {
                                            event_bus.publish(rcode_event::Event::StreamingProgress {
                                                session_id: ctx.session_id.clone(),
                                                accumulated_text: (*Arc::clone(&accumulated_text)).clone(),
                                                accumulated_reasoning: (*Arc::clone(&accumulated_reasoning)).clone(),
                                            });
                                        }
                                    }
                                }
                                StreamingEvent::ToolCallStart { id, name } => {
                                    active_tool_call = Some((id, name, String::new()));
                                }
                                StreamingEvent::ToolCallArg { id, name: _, value } => {
                                    if let Some(ref mut active) = active_tool_call {
                                        if active.0 == id {
                                            active.2.push_str(&value);
                                        }
                                    }
                                }
                                StreamingEvent::ToolCallEnd { id } => {
                                    if let Some(ref active) = active_tool_call {
                                        if active.0 == id {
                                            let args: serde_json::Value = serde_json::from_str(&active.2).unwrap_or_else(|e| {
                                                tracing::warn!("Failed to parse tool call arguments: {}", e);
                                                serde_json::json!({})
                                            });
                                            tool_calls.push((active.0.clone(), active.1.clone(), args));
                                            active_tool_call = None;
                                        }
                                    }
                                }
                                StreamingEvent::ContentBlock { content: _ } => {}
                                StreamingEvent::Finish { stop_reason, usage } => {
                                    final_stop_reason = match stop_reason {
                                        rcode_core::provider::StopReason::EndTurn => StopReason::EndOfTurn,
                                        rcode_core::provider::StopReason::MaxTokens => StopReason::MaxSteps,
                                        rcode_core::provider::StopReason::StopSequence => StopReason::EndOfTurn,
                                    };
                                    final_usage = Some(usage);
                                    stream_ended = true;
                                }
                            }
                        }
                        None => {
                            stream_ended = true;
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)), if cancellation_token.is_cancelled() => {
                    return Ok(ShouldContinue(false, StopReason::UserStopped, None));
                }
            }

            if stream_ended {
                break;
            }
        }

        // Check cancellation after streaming
        if cancellation_token.is_cancelled() {
            return Ok(ShouldContinue(false, StopReason::UserStopped, None));
        }

        // Build the assistant message with accumulated content
        let mut assistant_parts = Vec::new();
        let final_text = (*accumulated_text).clone();
        let final_reasoning = (*accumulated_reasoning).clone();

        if !final_text.is_empty() {
            assistant_parts.push(Part::Text {
                content: final_text,
            });
        }

        if !final_reasoning.is_empty() {
            assistant_parts.push(Part::Reasoning {
                content: final_reasoning,
            });
        }

        // Add tool calls if any
        let has_tool_calls = !tool_calls.is_empty();
        for (id, name, arguments) in &tool_calls {
            assistant_parts.push(Part::ToolCall {
                id: id.clone(),
                name: name.clone(),
                arguments: Box::new(arguments.clone()),
            });
        }

        // If no content and no tool calls, return early
        if assistant_parts.is_empty() {
            return Ok(ShouldContinue(false, final_stop_reason, final_usage.clone()));
        }

        // Add assistant message to context
        let assistant_msg = Message::assistant(ctx.session_id.clone(), assistant_parts.clone());
        ctx.messages.push(assistant_msg.clone());

        // Publish MessageAdded event
        if let Some(event_bus) = &self.event_bus {
            event_bus.publish(rcode_event::Event::MessageAdded {
                session_id: ctx.session_id.clone(),
                message_id: assistant_msg.id.0.clone(),
            });
        }

        // If no tool calls, we're done
        if !has_tool_calls {
            return Ok(ShouldContinue(false, final_stop_reason, final_usage.clone()));
        }

        // Check cancellation before starting tool execution
        if cancellation_token.is_cancelled() {
            return Ok(ShouldContinue(false, StopReason::UserStopped, final_usage.clone()));
        }

        let tool_call_names: Vec<String> = tool_calls.iter().map(|(_, name, _)| name.clone()).collect();
        let mut join_set = JoinSet::new();
        let mut tool_results: Vec<Part> = Vec::new();
        
        // Check permissions before executing tools
        let permission_service = self.permission_service.clone();

        for (id, name, arguments) in tool_calls {
            let tools = Arc::clone(&self.tools);
            let ctx = ToolContext {
                session_id: ctx.session_id.clone(),
                project_path: ctx.project_path.clone(),
                cwd: ctx.cwd.clone(),
                user_id: ctx.user_id.clone(),
                agent: self.agent.id().to_string(),
            };
            
            // Check permission if service is configured
            if let Some(ref perm_svc) = permission_service {
                let request = PermissionRequest {
                    tool_name: name.clone(),
                    tool_input: arguments.clone(),
                    reason: None,
                };
                let allowed = perm_svc.check(&request).await.unwrap_or(false);
                if !allowed {
                    // Permission denied - return error result
                    info!("Permission denied for tool: {}", name);
                    tool_results.push(Part::ToolResult {
                        tool_call_id: id,
                        content: format!("Error: Permission denied for tool '{}'", name),
                        is_error: true,
                    });
                    continue;
                }
            }

            // Runtime check: enforce allowed_tools at execution time
            if let Some(ref allowed) = self.allowed_tools {
                if !allowed.iter().any(|t| t == &name) {
                    info!("Tool '{}' is not in allowed_tools list", name);
                    tool_results.push(Part::ToolResult {
                        tool_call_id: id,
                        content: format!("Error: Tool '{}' is not allowed for this agent", name),
                        is_error: true,
                    });
                    continue;
                }
            }

            join_set.spawn(async move {
                let result = tools.execute(&name, arguments, &ctx).await;
                (id, name, result)
            });
        }
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((id, name, Ok(r))) => {
                    info!("Tool {} executed successfully", name);
                    tool_results.push(Part::ToolResult {
                        tool_call_id: id,
                        content: r.content,
                        is_error: false,
                    });
                }
                Ok((id, _, Err(e))) => {
                    warn!("Tool {} failed: {}", id, e);
                    tool_results.push(Part::ToolResult {
                        tool_call_id: id,
                        content: format!("Error: {}", e),
                        is_error: true,
                    });
                }
                Err(e) => {
                    error!("Tool join error: {:?}", e);
                }
            }
        }

        // Add tool results message
        let results_msg = Message::assistant(ctx.session_id.clone(), tool_results.clone());
        ctx.messages.push(results_msg.clone());

        // Publish MessageAdded event for tool results
        if let Some(event_bus) = &self.event_bus {
            event_bus.publish(rcode_event::Event::MessageAdded {
                session_id: ctx.session_id.clone(),
                message_id: results_msg.id.0.clone(),
            });
        }

        // Continue for another step if we had tool calls
        Ok(ShouldContinue(true, StopReason::ToolCalls(tool_call_names), final_usage.clone()))
    }
}

struct ShouldContinue(bool, StopReason, Option<rcode_core::provider::TokenUsage>);

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{
        Message, Role, Part, AgentContext, AgentResult,
        provider::{StreamingEvent, StopReason as CoreStopReason, TokenUsage},
    };
    use rcode_core::agent::Agent;
    use rcode_providers::MockLlmProvider;
    use rcode_tools::ToolRegistryService;
    use rcode_event::EventBus;
    use std::sync::Arc;
    use std::path::PathBuf;

    /// Mock agent for testing
    struct MockTestAgent {
        id: String,
        name: String,
        description: String,
        system_prompt: String,
    }

    impl MockTestAgent {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                name: format!("{} Agent", id),
                description: "Mock agent for testing".to_string(),
                system_prompt: "You are a test agent".to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl Agent for MockTestAgent {
        fn id(&self) -> &str { &self.id }
        fn name(&self) -> &str { &self.name }
        fn description(&self) -> &str { &self.description }
        
        async fn run(&self, _ctx: &mut AgentContext) -> rcode_core::error::Result<AgentResult> {
            Ok(AgentResult {
                message: Message::assistant("test-session".to_string(), vec![]),
                should_continue: false,
                stop_reason: StopReason::EndOfTurn,
                usage: None,
            })
        }
        
        fn system_prompt(&self) -> String {
            self.system_prompt.clone()
        }
    }

    fn create_test_agent_context() -> AgentContext {
        AgentContext {
            session_id: "test-session".to_string(),
            project_path: PathBuf::from("/tmp/test"),
            cwd: PathBuf::from("/tmp/test"),
            user_id: Some("test-user".to_string()),
            model_id: "claude-sonnet-4-5".to_string(),
            messages: vec![
                Message::user("test-session".to_string(), vec![
                    Part::Text { content: "Hello".to_string() }
                ]),
            ],
        }
    }

    #[test]
    fn test_cancellation_token_default_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_cancel() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_clone() {
        let token = CancellationToken::new();
        let cloned = token.clone();
        assert!(!cloned.is_cancelled());
        token.cancel();
        assert!(cloned.is_cancelled());
    }

    #[test]
    fn test_agent_executor_new() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutor::new(agent, provider, tools);
        assert!(executor.event_bus.is_none());
    }

    #[test]
    fn test_agent_executor_with_event_bus() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        let event_bus = Arc::new(EventBus::new(10));
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_event_bus(event_bus);
        assert!(executor.event_bus.is_some());
    }

    #[tokio::test]
    async fn test_executor_run_with_text_response() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Configure provider to return a simple text response
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Text { delta: " world".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        // Should have ended because no tool calls
        assert!(!result.should_continue);
    }

    #[tokio::test]
    async fn test_executor_run_with_tool_call() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Configure provider to return a tool call
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Let me search for that".to_string() },
            StreamingEvent::ToolCallStart { id: "call_123".to_string(), name: "bash".to_string() },
            StreamingEvent::ToolCallArg { id: "call_123".to_string(), name: "command".to_string(), value: "echo hello".to_string() },
            StreamingEvent::ToolCallEnd { id: "call_123".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 20, 
                    total_tokens: Some(30) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        // Should continue because tool call was made
        // Actually the run() loops until should_continue is false
        // Since we have a tool call, it will try to continue but max_steps will be hit
        // Let's verify the result depends on loop behavior
    }

    #[tokio::test]
    async fn test_executor_run_with_cancellation_before_start() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        let cancellation_token = CancellationToken::new();
        
        // Cancel before running
        cancellation_token.cancel();
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run_with_cancellation(&mut ctx, cancellation_token).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.should_continue);
        assert!(matches!(result.stop_reason, StopReason::UserStopped));
    }

    #[tokio::test]
    async fn test_executor_run_with_max_steps() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Configure provider to return text without tool calls (will continue forever)
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        // Add enough messages so the request model defaults to claude-sonnet-4-5
        // and doesn't trigger early return due to empty messages.first()
        ctx.messages.push(Message::user("test-session".to_string(), vec![
            Part::Text { content: "Hello".to_string() }
        ]));
        
        let result = executor.run(&mut ctx).await;
        // Should hit max_steps limit
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_run_with_reasoning() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Configure provider with reasoning
        provider.set_stream_events(vec![
            StreamingEvent::Reasoning { delta: "Let me think about this".to_string() },
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_run_with_streaming_error() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Configure provider to return an error
        provider.set_error(rcode_core::error::RCodeError::Provider("Stream error".to_string()));
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok()); // Returns Ok even on error, with error in message
        let result = result.unwrap();
        assert!(!result.should_continue);
        assert!(matches!(result.stop_reason, StopReason::Error));
    }

    #[tokio::test]
    async fn test_executor_with_empty_messages_gets_default_model() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Provider returns simple text
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = AgentContext {
            session_id: "test-session".to_string(),
            project_path: PathBuf::from("/tmp/test"),
            cwd: PathBuf::from("/tmp/test"),
            user_id: None,
            model_id: "claude-sonnet-4-5".to_string(),
            messages: vec![], // Empty messages - should still work
        };
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_cancellation_during_streaming() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        let cancellation_token = CancellationToken::new();
        
        // Provider returns slow streaming
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Slow".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        // Cancel immediately 
        let executor_clone = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        
        // Spawn a task to cancel after a brief delay
        let cancel_token = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            cancel_token.cancel();
        });
        
        let result = executor.run_with_cancellation(&mut ctx, cancellation_token).await;
        // Result depends on timing - may or may not complete
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_content_block_event() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Configure provider with ContentBlock
        use rcode_core::provider::ContentBlock;
        provider.set_stream_events(vec![
            StreamingEvent::ContentBlock { content: Box::new(ContentBlock::Text { text: "Hello".to_string() }) },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_should_continue_struct() {
        let should_continue = ShouldContinue(true, StopReason::EndOfTurn, None);
        assert!(should_continue.0);
        assert!(matches!(should_continue.1, StopReason::EndOfTurn));
        assert!(should_continue.2.is_none());
        
        let should_continue = ShouldContinue(false, StopReason::MaxSteps, None);
        assert!(!should_continue.0);
        assert!(matches!(should_continue.1, StopReason::MaxSteps));
        assert!(should_continue.2.is_none());
    }

    #[tokio::test]
    async fn test_executor_text_accumulation_over_limit() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        let event_bus = Arc::new(EventBus::new(10));
        
        // Create many text events to exceed MAX_ACCUMULATED_TEXT
        let mut events = vec![];
        let long_text = "x".repeat(500);
        for _ in 0..25 {
            events.push(StreamingEvent::Text { delta: long_text.clone() });
        }
        events.push(StreamingEvent::Finish { 
            stop_reason: CoreStopReason::EndTurn, 
            usage: TokenUsage { 
                input_tokens: 10, 
                output_tokens: 5, 
                total_tokens: Some(15) 
            },
        });
        
        provider.set_stream_events(events);
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_event_bus(event_bus);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_with_event_bus_publishes_events() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        let event_bus = Arc::new(EventBus::new(10));
        
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_event_bus(event_bus);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_stop_reason_max_tokens() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::MaxTokens, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
        // Result should have MaxSteps stop reason when provider returns MaxTokens
    }

    #[tokio::test]
    async fn test_executor_stop_reason_stop_sequence() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::StopSequence, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_empty_final_response() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Stream with just a finish, no text
        provider.set_stream_events(vec![
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        // Should not continue since there was no content
        assert!(!result.should_continue);
    }

    #[tokio::test]
    async fn test_executor_only_reasoning_no_text() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Only reasoning, no text
        provider.set_stream_events(vec![
            StreamingEvent::Reasoning { delta: "thinking...".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
        // Should not continue since there was only reasoning
        let result = result.unwrap();
        assert!(!result.should_continue);
    }

    #[tokio::test]
    async fn test_executor_tool_call_with_empty_args() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Tool call with empty arguments
        provider.set_stream_events(vec![
            StreamingEvent::ToolCallStart { id: "call_1".to_string(), name: "bash".to_string() },
            StreamingEvent::ToolCallArg { id: "call_1".to_string(), name: "command".to_string(), value: "".to_string() },
            StreamingEvent::ToolCallEnd { id: "call_1".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_multiple_tool_calls() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Multiple tool calls
        provider.set_stream_events(vec![
            StreamingEvent::ToolCallStart { id: "call_1".to_string(), name: "bash".to_string() },
            StreamingEvent::ToolCallArg { id: "call_1".to_string(), name: "command".to_string(), value: "echo 1".to_string() },
            StreamingEvent::ToolCallEnd { id: "call_1".to_string() },
            StreamingEvent::ToolCallStart { id: "call_2".to_string(), name: "read".to_string() },
            StreamingEvent::ToolCallArg { id: "call_2".to_string(), name: "path".to_string(), value: "/tmp/file".to_string() },
            StreamingEvent::ToolCallEnd { id: "call_2".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_with_reasoning_over_limit() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        let event_bus = Arc::new(EventBus::new(10));
        
        // Many reasoning events to exceed MAX_ACCUMULATED_TEXT
        let mut events = vec![];
        let long_text = "x".repeat(500);
        for _ in 0..25 {
            events.push(StreamingEvent::Reasoning { delta: long_text.clone() });
        }
        events.push(StreamingEvent::Finish { 
            stop_reason: CoreStopReason::EndTurn, 
            usage: TokenUsage { 
                input_tokens: 10, 
                output_tokens: 5, 
                total_tokens: Some(15) 
            },
        });
        
        provider.set_stream_events(events);
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_event_bus(event_bus);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_agent_executor_new_sets_fields() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutor::new(agent, provider, tools);
        
        // Just verify it compiles and runs
        assert!(executor.event_bus.is_none());
    }

    #[test]
    fn test_cancellation_token_default() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_clone_shares_state() {
        let token1 = CancellationToken::default();
        let token2 = token1.clone();
        
        // Clone shares the same underlying state via Arc
        token1.cancel();
        assert!(token1.is_cancelled());
        assert!(token2.is_cancelled()); // Clone shares state
    }

    #[tokio::test]
    async fn test_executor_run_with_both_text_and_tool_call() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Text followed by tool call
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "I'll run that command".to_string() },
            StreamingEvent::ToolCallStart { id: "call_1".to_string(), name: "bash".to_string() },
            StreamingEvent::ToolCallArg { id: "call_1".to_string(), name: "command".to_string(), value: "ls".to_string() },
            StreamingEvent::ToolCallEnd { id: "call_1".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_cancel_during_tool_execution() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        let cancellation_token = CancellationToken::new();
        
        // Provider returns tool call that will be cancelled
        provider.set_stream_events(vec![
            StreamingEvent::ToolCallStart { id: "call_1".to_string(), name: "bash".to_string() },
            StreamingEvent::ToolCallArg { id: "call_1".to_string(), name: "command".to_string(), value: "sleep 10".to_string() },
            StreamingEvent::ToolCallEnd { id: "call_1".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();
        
        // Cancel after a short delay
        let token_clone = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            token_clone.cancel();
        });
        
        let result = executor.run_with_cancellation(&mut ctx, cancellation_token).await;
        // Should complete, but may or may not be cancelled depending on timing
        assert!(result.is_ok());
    }

    #[test]
    fn test_executor_with_model_override() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_model_override("claude-opus-4".to_string());
        
        // Verify model_override is set
        assert!(executor.model_override.is_some());
        assert_eq!(executor.model_override.as_deref().unwrap(), "claude-opus-4");
    }

    #[test]
    fn test_executor_with_tool_filtering() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_allowed_tools(vec!["bash".to_string(), "read".to_string()]);
        
        // Verify allowed_tools is set
        assert!(executor.allowed_tools.is_some());
        let allowed = executor.allowed_tools.as_ref().unwrap();
        assert_eq!(allowed.len(), 2);
        assert!(allowed.contains(&"bash".to_string()));
        assert!(allowed.contains(&"read".to_string()));
    }

    #[test]
    fn test_agent_executor_builder_new() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let builder = AgentExecutorBuilder::new(agent, provider, tools);
        
        // Verify builder creates executor correctly
        let executor = builder.build();
        assert!(executor.event_bus.is_none());
        assert!(executor.model_override.is_none());
        assert!(executor.allowed_tools.is_none());
    }

    #[test]
    fn test_agent_executor_builder_with_all_options() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        let event_bus = Arc::new(EventBus::new(10));
        
        let executor = AgentExecutorBuilder::new(agent, provider, tools)
            .with_event_bus(event_bus)
            .with_model_override("claude-sonnet-5".to_string())
            .with_allowed_tools(vec!["bash".to_string()])
            .build();
        
        assert!(executor.event_bus.is_some());
        assert_eq!(executor.model_override.as_deref().unwrap(), "claude-sonnet-5");
        assert_eq!(executor.allowed_tools.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_agent_executor_builder_with_definition() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let def = rcode_core::AgentDefinition {
            identifier: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            when_to_use: "Testing".to_string(),
            system_prompt: "You are a test".to_string(),
            mode: rcode_core::agent_definition::AgentMode::All,
            hidden: false,
            permission: rcode_core::agent_definition::AgentPermissionConfig::default(),
            tools: vec!["bash".to_string(), "read".to_string()],
            model: Some("claude-opus-5".to_string()),
            max_tokens: Some(8192),
            reasoning_effort: Some("high".to_string()),
        };
        
        let executor = AgentExecutorBuilder::new(agent, provider, tools)
            .with_definition(Arc::new(def))
            .build();
        
        assert_eq!(executor.model_override.as_deref().unwrap(), "claude-opus-5");
        assert_eq!(executor.max_tokens_override.unwrap(), 8192);
        assert_eq!(executor.reasoning_effort.as_deref().unwrap(), "high");
        assert!(executor.allowed_tools.is_some());
        assert_eq!(executor.allowed_tools.unwrap().len(), 2);
    }

    #[test]
    fn test_agent_executor_builder_with_definition_empty_tools() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Empty tools means all tools allowed - should not set allowed_tools
        let def = rcode_core::AgentDefinition {
            identifier: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            when_to_use: "Testing".to_string(),
            system_prompt: "You are a test".to_string(),
            mode: rcode_core::agent_definition::AgentMode::All,
            hidden: false,
            permission: rcode_core::agent_definition::AgentPermissionConfig::default(),
            tools: vec![],  // Empty = all tools
            model: None,
            max_tokens: None,
            reasoning_effort: None,
        };
        
        let executor = AgentExecutorBuilder::new(agent, provider, tools)
            .with_definition(Arc::new(def))
            .build();
        
        // Model should still be None, allowed_tools should be None (empty tools list doesn't set filter)
        assert!(executor.model_override.is_none());
        assert!(executor.max_tokens_override.is_none());
        assert!(executor.reasoning_effort.is_none());
        assert!(executor.allowed_tools.is_none());
    }

    #[test]
    fn test_agent_executor_builder_with_max_tokens_and_reasoning_effort() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutorBuilder::new(agent, provider, tools)
            .with_max_tokens_override(16384)
            .with_reasoning_effort("low")
            .build();
        
        assert_eq!(executor.max_tokens_override.unwrap(), 16384);
        assert_eq!(executor.reasoning_effort.as_deref().unwrap(), "low");
    }

    #[test]
    fn test_agent_executor_with_max_tokens_override() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_max_tokens_override(8192);
        
        assert_eq!(executor.max_tokens_override.unwrap(), 8192);
    }

    #[test]
    fn test_agent_executor_with_reasoning_effort() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_reasoning_effort("high".to_string());
        
        assert_eq!(executor.reasoning_effort.as_deref().unwrap(), "high");
    }

    #[test]
    fn test_agent_executor_auto_compact_defaults_to_false() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutor::new(agent, provider, tools);
        
        assert!(!executor.auto_compact);
        assert_eq!(executor.max_messages_before_compact, 50);
        assert_eq!(executor.messages_to_keep_after_compact, 20);
    }

    #[test]
    fn test_agent_executor_with_auto_compact_enabled() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_auto_compact(true);
        
        assert!(executor.auto_compact);
    }

    #[test]
    fn test_agent_executor_with_compaction_thresholds() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutor::new(agent, provider, tools)
            .with_compaction_thresholds(100, 30);
        
        assert_eq!(executor.max_messages_before_compact, 100);
        assert_eq!(executor.messages_to_keep_after_compact, 30);
    }

    #[test]
    fn test_agent_executor_builder_auto_compact_defaults() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutorBuilder::new(agent, provider, tools).build();
        
        assert!(!executor.auto_compact);
        assert_eq!(executor.max_messages_before_compact, 50);
        assert_eq!(executor.messages_to_keep_after_compact, 20);
    }

    #[test]
    fn test_agent_executor_builder_with_auto_compact() {
        let agent: Arc<dyn Agent> = Arc::new(MockTestAgent::new("test"));
        let provider: Arc<dyn rcode_providers::LlmProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        let executor = AgentExecutorBuilder::new(agent, provider, tools)
            .with_auto_compact(true)
            .with_compaction_thresholds(75, 25)
            .build();
        
        assert!(executor.auto_compact);
        assert_eq!(executor.max_messages_before_compact, 75);
        assert_eq!(executor.messages_to_keep_after_compact, 25);
    }

    #[tokio::test]
    async fn test_executor_compacts_messages_when_threshold_exceeded_and_enabled() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Set threshold to 5 messages, keep 3
        let executor = AgentExecutor::new(agent, provider.clone(), tools)
            .with_auto_compact(true)
            .with_compaction_thresholds(5, 3);
        
        // Create context with 10 messages (exceeds threshold of 5)
        let mut ctx = AgentContext {
            session_id: "test-session".to_string(),
            project_path: PathBuf::from("/tmp/test"),
            cwd: PathBuf::from("/tmp/test"),
            user_id: None,
            model_id: "claude-sonnet-4-5".to_string(),
            messages: (0..10).map(|i| {
                Message::user("test-session".to_string(), vec![
                    Part::Text { content: format!("Message {}", i) }
                ])
            }).collect(),
        };
        
        // Provider returns simple text response
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
        
        // After compaction, messages should be: 1 summary + 3 kept = 4 total
        // Original: 10 messages, threshold: 5, keep: 3, so 10 - 3 = 7 summarized
        // Messages after compaction: [summary marker] + [last 3 messages (index 7, 8, 9)]
        // Then the assistant response is added, making it 5 total
        assert_eq!(ctx.messages.len(), 5, "Should have summary + 3 kept + 1 assistant response");
        
        // First message should be the summary marker
        assert!(matches!(
            ctx.messages[0].parts[0],
            Part::Text { content: ref s } if s.contains("Previous 7 messages summarized")
        ));
    }

    #[tokio::test]
    async fn test_executor_does_not_compact_when_disabled() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // auto_compact is false by default
        let executor = AgentExecutor::new(agent, provider.clone(), tools);
        
        // Create context with 10 messages (exceeds default threshold of 50)
        let mut ctx = AgentContext {
            session_id: "test-session".to_string(),
            project_path: PathBuf::from("/tmp/test"),
            cwd: PathBuf::from("/tmp/test"),
            user_id: None,
            model_id: "claude-sonnet-4-5".to_string(),
            messages: (0..10).map(|i| {
                Message::user("test-session".to_string(), vec![
                    Part::Text { content: format!("Message {}", i) }
                ])
            }).collect(),
        };
        
        // Provider returns simple text response
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
        
        // Messages should NOT be compacted - we started with 10, added 1 assistant = 11
        // Plus tool results were not added since no tool calls
        assert_eq!(ctx.messages.len(), 11, "Should not compact when disabled");
    }

    #[tokio::test]
    async fn test_executor_does_not_compact_when_under_threshold() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Enable compaction but set high threshold
        let executor = AgentExecutor::new(agent, provider.clone(), tools)
            .with_auto_compact(true)
            .with_compaction_thresholds(100, 20); // threshold 100, we have only 3
        
        // Create context with only 3 messages (under threshold of 100)
        let mut ctx = AgentContext {
            session_id: "test-session".to_string(),
            project_path: PathBuf::from("/tmp/test"),
            cwd: PathBuf::from("/tmp/test"),
            user_id: None,
            model_id: "claude-sonnet-4-5".to_string(),
            messages: (0..3).map(|i| {
                Message::user("test-session".to_string(), vec![
                    Part::Text { content: format!("Message {}", i) }
                ])
            }).collect(),
        };
        
        // Provider returns simple text response
        provider.set_stream_events(vec![
            StreamingEvent::Text { delta: "Hello".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
        
        // Messages should NOT be compacted since under threshold
        // 3 original + 1 assistant response = 4 messages
        assert_eq!(ctx.messages.len(), 4, "Should not compact when under threshold");
    }
}
