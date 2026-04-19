//! Agent executor - main agent loop with streaming support

use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::interval;
use tokio_stream::StreamExt;
use rcode_core::{
    Agent, AgentContext, AgentResult, Message, Part, TaskChecklistItem, ToolContext, error::Result,
    permission::{Permission, PermissionRequest},
};
use rcode_core::agent::StopReason;
use rcode_core::provider::StreamingEvent;
use rcode_providers::LlmProvider;
use rcode_tools::ToolRegistryService;
use rcode_event::EventBus;
use tracing::{debug, error, info, warn};

use super::permissions::{PermissionService, AutoPermissionService};

const MAX_ACCUMULATED_TEXT: usize = 10_000;
const FLUSH_INTERVAL_SECS: u64 = 2;

/// Build a workspace context header injected before the agent's system prompt.
/// This gives the LLM structured information about the project, git state, and OS.
/// Optionally includes CogniCode intelligence snapshot if available.
fn build_workspace_context_sync(
    ctx: &AgentContext,
    intelligence_xml: &str,
) -> String {
    let mut lines = vec![];

    lines.push("<env>".to_string());

    // Working directory
    lines.push(format!("  Working directory: {}", ctx.cwd.display()));

    // Project path (if different from cwd)
    if ctx.project_path != ctx.cwd {
        lines.push(format!("  Project root: {}", ctx.project_path.display()));
    }

    // Project name (last component of project_path)
    if let Some(name) = ctx.project_path.file_name().and_then(|n| n.to_str()) {
        lines.push(format!("  Project: {}", name));
    }

    // OS
    lines.push(format!("  OS: {}", std::env::consts::OS));

    // Session
    lines.push(format!("  Session ID: {}", ctx.session_id));

    lines.push("</env>".to_string());

    // Append code intelligence if available
    if !intelligence_xml.is_empty() {
        lines.push(String::new());
        lines.push(intelligence_xml.to_string());
    }

    lines.join("\n")
}

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
    privacy: Option<rcode_privacy::service::PrivacyService>,
    intelligence_snapshot: Option<Arc<parking_lot::RwLock<rcode_cognicode::snapshot::IntelligenceSnapshot>>>,
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
            privacy: None,
            intelligence_snapshot: None,
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

    /// Set the privacy service for sanitizing sensitive data
    pub fn with_privacy_service(mut self, privacy: rcode_privacy::service::PrivacyService) -> Self {
        self.privacy = Some(privacy);
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
            privacy: self.privacy,
            intelligence_snapshot: self.intelligence_snapshot,
        }
    }
    
    /// Set the CogniCode intelligence snapshot for proactive context injection
    pub fn with_intelligence_snapshot(mut self, snapshot: Arc<parking_lot::RwLock<rcode_cognicode::snapshot::IntelligenceSnapshot>>) -> Self {
        self.intelligence_snapshot = Some(snapshot);
        self
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
    privacy: Option<rcode_privacy::service::PrivacyService>,
    intelligence_snapshot: Option<Arc<parking_lot::RwLock<rcode_cognicode::snapshot::IntelligenceSnapshot>>>,
}

impl AgentExecutor {
    /// Helper to publish an event to the event bus if available
    fn publish_event(&self, event: rcode_event::Event) {
        if let Some(ref event_bus) = self.event_bus {
            event_bus.publish(event);
        }
    }

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
            privacy: None,
            intelligence_snapshot: None,
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

    /// Set the privacy service for sanitizing sensitive data
    pub fn with_privacy_service(mut self, privacy: rcode_privacy::service::PrivacyService) -> Self {
        self.privacy = Some(privacy);
        self
    }

    /// Set the CogniCode intelligence snapshot for proactive context injection
    pub fn with_intelligence_snapshot(mut self, snapshot: Arc<parking_lot::RwLock<rcode_cognicode::snapshot::IntelligenceSnapshot>>) -> Self {
        self.intelligence_snapshot = Some(snapshot);
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
        info!(session_id = %ctx.session_id, model_id = %ctx.model_id, message_count = ctx.messages.len(), "agent execution starting");
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

            info!(session_id = %ctx.session_id, step = step_count, "starting agent step");

            // Process a single streaming turn
            match self.process_streaming_turn(ctx, &cancellation_token).await {
                Ok(ShouldContinue(should_continue, stop_reason, usage)) => {
                    if !should_continue {
                        info!(session_id = %ctx.session_id, stop_reason = ?stop_reason, has_usage = usage.is_some(), "agent execution completed");
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
        debug!(session_id = %ctx.session_id, model = %model, input_messages = ctx.messages.len(), "processing streaming turn");
        
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
        
        // Check if provider supports tool calling
        let caps = self.provider.capabilities();
        let provider_supports_tools = caps.supports_tool_calling && !filtered_tools.is_empty();
        let latest_user_text = ctx.messages.iter().rev().find_map(|m| {
            if m.role == rcode_core::Role::User {
                Some(
                    m.parts
                        .iter()
                        .filter_map(|p| match p {
                            Part::Text { content } => Some(content.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                        .to_lowercase(),
                )
            } else {
                None
            }
        }).unwrap_or_default();
        let prompt_explicitly_requests_tools = [
            "use a tool",
            "use the tool",
            "bash tool",
            "read tool",
            "grep tool",
            "glob tool",
            "run `pwd`",
            "run pwd",
            "current working directory",
            "do not answer from memory",
        ].iter().any(|needle| latest_user_text.contains(needle));
        
        // If the user explicitly asked for tool use but the provider doesn't support it,
        // return a visible explanation instead of silently failing.
        // Otherwise, omit tools and allow normal chat-only prompts to continue.
        if !filtered_tools.is_empty() && !caps.supports_tool_calling && prompt_explicitly_requests_tools {
            warn!(session_id = %ctx.session_id, provider = %self.provider.provider_id(), 
                  "Provider does not support tool calling, returning informative error");
            
            let unsupported_msg = Message::assistant(ctx.session_id.clone(), vec![Part::Text {
                content: format!(
                    "The current model/provider ({}) does not support tool calling in this configuration. \
                    This commonly happens with proxy or OpenAI/Anthropic-compatible backends that only support chat completions. \
                    Please switch to a provider/model with native tool calling support (for example Claude, GPT-4, or compatible OpenRouter models), \
                    or use a chat-only prompt.",
                    self.provider.provider_id()
                ),
            }]);
            ctx.messages.push(unsupported_msg.clone());
            if let Some(event_bus) = &self.event_bus {
                event_bus.publish(rcode_event::Event::MessageAdded {
                    session_id: ctx.session_id.clone(),
                    message_id: unsupported_msg.id.0.clone(),
                });
            }
            return Ok(ShouldContinue(false, StopReason::EndOfTurn, None));
        }
        
        // Hook 1: Sanitize system prompt before sending to LLM
        // This ensures any sensitive data in the system prompt is masked before it leaves the executor
        let agent_prompt = if let Some(ref privacy) = self.privacy {
            privacy.sanitize_prompt(&ctx.session_id, &self.agent.system_prompt()).await
        } else {
            self.agent.system_prompt()
        };

        // Inject workspace context header before the agent's system prompt
        let intelligence_xml = self.intelligence_snapshot
            .as_ref()
            .map(|snap| snap.read().to_xml())
            .unwrap_or_default();
        let workspace_context = build_workspace_context_sync(ctx, &intelligence_xml);
        let system_prompt = format!("{}\n\n{}", workspace_context, agent_prompt);

        let request = rcode_core::CompletionRequest {
            model,
            messages: ctx.messages.clone(), // TODO: CompletionRequest owns messages, cannot take slice without significant refactoring
            system_prompt: Some(system_prompt),
            tools: if provider_supports_tools {
                filtered_tools.into_iter().map(|t| {
                    let params = self.tools.get(&t.id)
                        .map(|tool| tool.parameters())
                        .unwrap_or_else(|| serde_json::json!({}));
                    rcode_core::ToolDefinition {
                        name: t.id,
                        description: t.description,
                        parameters: params,
                    }
                }).collect()
            } else {
                vec![]
            },
            temperature: None,
            max_tokens: self.max_tokens_override.or(Some(4096)),
            reasoning_effort: self.reasoning_effort.clone(),
        };

        let response = self.provider.stream(request).await?;
        debug!(session_id = %ctx.session_id, "provider stream opened");

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
                                    debug!(session_id = %ctx.session_id, delta_len = delta.len(), "received text delta");
                                    // Publish immediate text delta event (additive to legacy StreamingProgress)
                                    self.publish_event(rcode_event::Event::StreamTextDelta {
                                        session_id: ctx.session_id.clone(),
                                        delta: delta.clone(),
                                    });
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
                                    debug!(session_id = %ctx.session_id, delta_len = delta.len(), "received reasoning delta");
                                    // Publish immediate reasoning delta event (additive to legacy StreamingProgress)
                                    self.publish_event(rcode_event::Event::StreamReasoningDelta {
                                        session_id: ctx.session_id.clone(),
                                        delta: delta.clone(),
                                    });
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
                                    info!(session_id = %ctx.session_id, tool_call_id = %id, tool_name = %name, "tool call started");
                                    // Publish immediate tool call start event
                                    self.publish_event(rcode_event::Event::StreamToolCallStart {
                                        session_id: ctx.session_id.clone(),
                                        tool_call_id: id.clone(),
                                        name: name.clone(),
                                    });
                                    active_tool_call = Some((id, name, String::new()));
                                }
                                StreamingEvent::ToolCallArg { id, name: _, value } => {
                                    if let Some(ref mut active) = active_tool_call {
                                        if active.0 == id {
                                            // Publish immediate tool call arg event
                                            self.publish_event(rcode_event::Event::StreamToolCallArg {
                                                session_id: ctx.session_id.clone(),
                                                tool_call_id: id.clone(),
                                                value: value.clone(),
                                            });
                                            active.2.push_str(&value);
                                        }
                                    }
                                }
                                StreamingEvent::ToolCallEnd { id } => {
                                    info!(session_id = %ctx.session_id, tool_call_id = %id, "tool call completed in stream");
                                    if let Some(ref active) = active_tool_call {
                                        if active.0 == id {
                                            // Publish immediate tool call end event
                                            self.publish_event(rcode_event::Event::StreamToolCallEnd {
                                                session_id: ctx.session_id.clone(),
                                                tool_call_id: id.clone(),
                                            });
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
                                    info!(session_id = %ctx.session_id, provider_stop_reason = ?stop_reason, output_tokens = usage.output_tokens, "provider stream finished");
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

        // If no content and no tool calls, the model returned an empty response.
        // This can happen when the provider doesn't support tool calling.
        // Return a synthetic assistant message so the user gets feedback.
        if assistant_parts.is_empty() {
            warn!(session_id = %ctx.session_id, "stream finished without assistant content or tool calls — model may not support tool calling");
            let fallback_msg = Message::assistant(ctx.session_id.clone(), vec![Part::Text {
                content: "The model returned an empty response. This may happen if the provider does not support tool calling. Try a simpler prompt or switch to a different model.".to_string(),
            }]);
            ctx.messages.push(fallback_msg.clone());
            if let Some(event_bus) = &self.event_bus {
                event_bus.publish(rcode_event::Event::MessageAdded {
                    session_id: ctx.session_id.clone(),
                    message_id: fallback_msg.id.0.clone(),
                });
            }
            return Ok(ShouldContinue(false, StopReason::EndOfTurn, final_usage.clone()));
        }

        // Hook 2: Sanitize assistant message content before storing
        // This prevents sensitive data in the LLM response from being persisted
        let assistant_parts = if let Some(ref privacy) = self.privacy {
            let mut sanitized = Vec::new();
            for part in assistant_parts {
                match part {
                    Part::Text { content } => {
                        let sanitized_content = privacy.sanitize_response(&ctx.session_id, &content).await;
                        sanitized.push(Part::Text { content: sanitized_content });
                    }
                    Part::Reasoning { content } => {
                        let sanitized_content = privacy.sanitize_response(&ctx.session_id, &content).await;
                        sanitized.push(Part::Reasoning { content: sanitized_content });
                    }
                    other => sanitized.push(other),
                }
            }
            sanitized
        } else {
            assistant_parts
        };

        // Add assistant message to context
        let assistant_msg = Message::assistant(ctx.session_id.clone(), assistant_parts.clone());
        ctx.messages.push(assistant_msg.clone());

        // Publish MessageAdded event
        if let Some(event_bus) = &self.event_bus {
            debug!(session_id = %ctx.session_id, message_id = %assistant_msg.id.0, part_count = assistant_msg.parts.len(), "publishing assistant message added event");
            event_bus.publish(rcode_event::Event::MessageAdded {
                session_id: ctx.session_id.clone(),
                message_id: assistant_msg.id.0.clone(),
            });
            // Publish stream assistant committed event
            event_bus.publish(rcode_event::Event::StreamAssistantCommitted {
                session_id: ctx.session_id.clone(),
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
                    // Hook 3: Sanitize tool result content before storing
                    // Tool output may contain sensitive data (file contents, shell output with IPs, etc.)
                    let sanitized_content = if let Some(ref privacy) = self.privacy {
                        privacy.sanitize_response(&ctx.session_id, &r.content).await
                    } else {
                        r.content.clone()
                    };
                    tool_results.push(Part::ToolResult {
                        tool_call_id: id.clone(),
                        content: sanitized_content.clone(),
                        is_error: false,
                    });
                    if name == "todowrite" {
                        if let Some(items) = extract_task_checklist_items(r.metadata.as_ref()) {
                            tool_results.push(Part::TaskChecklist { items });
                        }
                    }
                    // Publish stream tool result event (use original r.content for real-time event)
                    self.publish_event(rcode_event::Event::StreamToolResult {
                        session_id: ctx.session_id.clone(),
                        tool_call_id: id,
                        content: r.content,
                        is_error: false,
                    });
                }
                Ok((id, _, Err(e))) => {
                    warn!("Tool {} failed: {}", id, e);
                    let err_content = format!("Error: {}", e);
                    // Hook 3: Sanitize error content before storing (errors may leak sensitive info)
                    let sanitized_err_content = if let Some(ref privacy) = self.privacy {
                        privacy.sanitize_response(&ctx.session_id, &err_content).await
                    } else {
                        err_content.clone()
                    };
                    tool_results.push(Part::ToolResult {
                        tool_call_id: id.clone(),
                        content: sanitized_err_content.clone(),
                        is_error: true,
                    });
                    // Publish stream tool result event for error case
                    self.publish_event(rcode_event::Event::StreamToolResult {
                        session_id: ctx.session_id.clone(),
                        tool_call_id: id,
                        content: err_content,
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

fn extract_task_checklist_items(metadata: Option<&serde_json::Value>) -> Option<Vec<TaskChecklistItem>> {
    let items = metadata?
        .get("checklist")?
        .get("items")?
        .as_array()?;

    Some(
        items
            .iter()
            .filter_map(|item| {
                Some(TaskChecklistItem {
                    id: item.get("id")?.as_str()?.to_string(),
                    content: item.get("content")?.as_str()?.to_string(),
                    status: item.get("status")?.as_str()?.to_string(),
                    priority: item.get("priority")?.as_str()?.to_string(),
                })
            })
            .collect(),
    )
}

struct ShouldContinue(bool, StopReason, Option<rcode_core::provider::TokenUsage>);

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{
        Message, Role, Part, AgentContext, AgentResult, ToolResult,
        provider::{StreamingEvent, StopReason as CoreStopReason, TokenUsage},
    };
    use rcode_core::agent::Agent;
    use rcode_providers::MockLlmProvider;
    use rcode_tools::mock::MockTool;
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
    fn test_extract_task_checklist_items_parses_metadata() {
        let metadata = serde_json::json!({
            "checklist": {
                "items": [
                    {
                        "id": "task-1",
                        "content": "Do thing",
                        "status": "pending",
                        "priority": "high"
                    }
                ]
            }
        });

        let items = extract_task_checklist_items(Some(&metadata)).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "task-1");
        assert_eq!(items[0].content, "Do thing");
    }

    #[test]
    fn test_extract_task_checklist_items_ignores_missing_metadata() {
        let metadata = serde_json::json!({"foo": "bar"});
        assert!(extract_task_checklist_items(Some(&metadata)).is_none());
    }

    #[tokio::test]
    async fn test_executor_appends_task_checklist_for_todowrite_results() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());

        provider.set_stream_events(vec![
            StreamingEvent::ToolCallStart { id: "call_1".to_string(), name: "todowrite".to_string() },
            StreamingEvent::ToolCallArg { id: "call_1".to_string(), name: "payload".to_string(), value: r#"{"action":"create","todos":[{"content":"Ship it","status":"pending","priority":"high"}]}"#.to_string() },
            StreamingEvent::ToolCallEnd { id: "call_1".to_string() },
            StreamingEvent::Finish {
                stop_reason: CoreStopReason::EndTurn,
                usage: TokenUsage { input_tokens: 1, output_tokens: 1, total_tokens: Some(2) },
            },
        ]);

        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();

        let _result = executor.run(&mut ctx).await.unwrap();

        let tool_result_message = ctx
            .messages
            .iter()
            .rev()
            .find(|message| matches!(message.parts.first(), Some(Part::ToolResult { .. })))
            .expect("expected tool result message");
        assert!(matches!(tool_result_message.parts[0], Part::ToolResult { .. }));
        let checklist_part = tool_result_message
            .parts
            .iter()
            .find(|part| matches!(part, Part::TaskChecklist { .. }))
            .expect("expected task checklist part");
        match checklist_part {
            Part::TaskChecklist { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].content, "Ship it");
            }
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn test_executor_does_not_append_task_checklist_for_non_todowrite_tools() {
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());

        let custom_tool = Arc::new(MockTool::new("mock_tool", "Mock Tool", "Mock tool"));
        custom_tool.then_return(ToolResult {
            title: "Bash".to_string(),
            content: "ok".to_string(),
            metadata: Some(serde_json::json!({
                "checklist": {
                    "items": [{
                        "id": "task-1",
                        "content": "Should be ignored",
                        "status": "pending",
                        "priority": "high"
                    }]
                }
            })),
            attachments: vec![],
        });
        tools.register(custom_tool);

        provider.set_stream_events(vec![
            StreamingEvent::ToolCallStart { id: "call_1".to_string(), name: "mock_tool".to_string() },
            StreamingEvent::ToolCallArg { id: "call_1".to_string(), name: "input".to_string(), value: r#"{"input":"pwd"}"#.to_string() },
            StreamingEvent::ToolCallEnd { id: "call_1".to_string() },
            StreamingEvent::Finish {
                stop_reason: CoreStopReason::EndTurn,
                usage: TokenUsage { input_tokens: 1, output_tokens: 1, total_tokens: Some(2) },
            },
        ]);

        let executor = AgentExecutor::new(agent, provider, tools);
        let mut ctx = create_test_agent_context();

        let _result = executor.run(&mut ctx).await.unwrap();

        let tool_result_message = ctx
            .messages
            .iter()
            .rev()
            .find(|message| matches!(message.parts.first(), Some(Part::ToolResult { .. })))
            .expect("expected tool result message");
        assert_eq!(tool_result_message.parts.len(), 1);
        assert!(matches!(tool_result_message.parts[0], Part::ToolResult { .. }));
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

    #[tokio::test]
    async fn test_executor_capability_check_with_support_succeeds() {
        use rcode_core::provider::ProviderCapabilities;
        
        let agent = Arc::new(MockTestAgent::new("test"));
        let provider = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(ToolRegistryService::new());
        
        // Set provider to support tool calling (default, but explicit for clarity)
        provider.set_capabilities(ProviderCapabilities::all());
        
        // Configure provider to return a tool call for bash (which is registered)
        provider.set_stream_events(vec![
            StreamingEvent::ToolCallStart { id: "call_123".to_string(), name: "bash".to_string() },
            StreamingEvent::ToolCallArg { id: "call_123".to_string(), name: "command".to_string(), value: "\"echo hello\"".to_string() },
            StreamingEvent::ToolCallEnd { id: "call_123".to_string() },
            StreamingEvent::Finish { 
                stop_reason: CoreStopReason::EndTurn, 
                usage: TokenUsage { 
                    input_tokens: 10, 
                    output_tokens: 5, 
                    total_tokens: Some(15) 
                },
            },
        ]);
        
        let executor = AgentExecutor::new(agent, provider.clone(), tools);
        let mut ctx = create_test_agent_context();
        
        let result = executor.run(&mut ctx).await;
        assert!(result.is_ok());
        // The test passes if the executor handles the tool call without error
        // (it will try to continue since there's a tool call, but max_steps may be hit)
    }
}
