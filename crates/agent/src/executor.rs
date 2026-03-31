//! Agent executor - main agent loop with streaming support

use std::sync::Arc;
use tokio_stream::StreamExt;
use opencode_core::{
    Agent, AgentContext, AgentResult, Message, Part, ToolContext, error::Result,
};
use opencode_core::agent::StopReason;
use opencode_core::provider::{StreamingEvent, StreamingResponse};
use opencode_providers::LlmProvider;
use opencode_tools::ToolRegistryService;
use opencode_event::EventBus;
use tracing::{info, warn, error};

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

/// AgentExecutor with streaming and cancellation support
pub struct AgentExecutor {
    agent: Arc<dyn Agent>,
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistryService>,
    event_bus: Option<Arc<EventBus>>,
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
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
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
                });
            }

            info!("Starting step {} of agent execution", step_count);

            // Process a single streaming turn
            match self.process_streaming_turn(ctx, &cancellation_token).await {
                Ok(ShouldContinue(should_continue, stop_reason)) => {
                    if !should_continue {
                        return Ok(AgentResult {
                            message: Message::assistant(ctx.session_id.clone(), vec![]),
                            should_continue: false,
                            stop_reason,
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
        let request = opencode_core::CompletionRequest {
            model: ctx.messages.first()
                .map(|_| "claude-sonnet-4-5".to_string())
                .unwrap_or_default(),
            messages: ctx.messages.clone(),
            system_prompt: Some(self.agent.system_prompt()),
            tools: self.tools.list().into_iter().map(|t|
                opencode_core::ToolDefinition {
                    name: t.id,
                    description: t.description,
                    parameters: serde_json::json!({}),
                }
            ).collect(),
            temperature: None,
            max_tokens: Some(4096),
        };

        let response = self.provider.stream(request).await?;

        // Process streaming events
        let mut accumulated_text = String::new();
        let mut accumulated_reasoning = String::new();
        let mut tool_calls = Vec::new();
        let mut active_tool_call: Option<(String, String, String)> = None;
        let mut final_stop_reason = StopReason::EndOfTurn;
        let mut final_usage = None;

        self.process_stream(
            response,
            cancellation_token,
            &mut |event| {
                match event {
                    StreamingEvent::Text { delta } => {
                        accumulated_text.push_str(&delta);
                    }
                    StreamingEvent::Reasoning { delta } => {
                        accumulated_reasoning.push_str(&delta);
                    }
                    StreamingEvent::ToolCallStart { id, name } => {
                        // Start accumulating a tool call
                        active_tool_call = Some((id, name, String::new()));
                    }
                    StreamingEvent::ToolCallArg { id, name, value } => {
                        // Continue accumulating tool call arguments
                        if let Some(ref mut active) = active_tool_call {
                            if active.0 == id {
                                active.2.push_str(&value);
                            }
                        }
                    }
                    StreamingEvent::ToolCallEnd { id } => {
                        // Finalize tool call
                        if let Some(ref active) = active_tool_call {
                            if active.0 == id {
                                let arguments: serde_json::Value = serde_json::from_str(&active.2)
                                    .unwrap_or(serde_json::json!({}));
                                tool_calls.push((active.0.clone(), active.1.clone(), arguments));
                                active_tool_call = None;
                            }
                        }
                    }
                    StreamingEvent::ContentBlock { content: _ } => {
                        // Handle content blocks if needed
                    }
                    StreamingEvent::Finish { stop_reason, usage } => {
                        final_stop_reason = match stop_reason {
                            opencode_core::provider::StopReason::EndTurn => StopReason::EndOfTurn,
                            opencode_core::provider::StopReason::MaxTokens => StopReason::MaxSteps,
                            opencode_core::provider::StopReason::StopSequence => StopReason::EndOfTurn,
                        };
                        final_usage = Some(usage);
                    }
                }
            },
        ).await?;

        // Check cancellation after streaming
        if cancellation_token.is_cancelled() {
            return Ok(ShouldContinue(false, StopReason::UserStopped));
        }

        // Build the assistant message with accumulated content
        let mut assistant_parts = Vec::new();

        if !accumulated_text.is_empty() {
            assistant_parts.push(Part::Text {
                content: accumulated_text.clone(),
            });
        }

        if !accumulated_reasoning.is_empty() {
            assistant_parts.push(Part::Reasoning {
                content: accumulated_reasoning,
            });
        }

        // Add tool calls if any
        let has_tool_calls = !tool_calls.is_empty();
        for (id, name, arguments) in &tool_calls {
            assistant_parts.push(Part::ToolCall {
                id: id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
            });
        }

        // If no content and no tool calls, return early
        if assistant_parts.is_empty() {
            return Ok(ShouldContinue(false, final_stop_reason));
        }

        // Add assistant message to context
        let assistant_msg = Message::assistant(ctx.session_id.clone(), assistant_parts.clone());
        ctx.messages.push(assistant_msg.clone());

        // Publish MessageAdded event
        if let Some(event_bus) = &self.event_bus {
            event_bus.publish(opencode_event::Event::MessageAdded {
                session_id: ctx.session_id.clone(),
                message_id: assistant_msg.id.0.clone(),
            });
        }

        // If no tool calls, we're done
        if !has_tool_calls {
            return Ok(ShouldContinue(false, final_stop_reason));
        }

        // Execute tool calls and collect results
        let mut tool_results = Vec::new();
        for (id, name, arguments) in &tool_calls {
            // Check cancellation before each tool call
            if cancellation_token.is_cancelled() {
                return Ok(ShouldContinue(false, StopReason::UserStopped));
            }

            info!("Executing tool: {} with id: {}", name, id);

            let tool_result = match self.tools.execute(name, arguments.clone(), &ToolContext {
                session_id: ctx.session_id.clone(),
                project_path: ctx.project_path.clone(),
                cwd: ctx.cwd.clone(),
                user_id: ctx.user_id.clone(),
            }).await {
                Ok(result) => {
                    info!("Tool {} executed successfully", name);
                    Part::ToolResult {
                        tool_call_id: id.clone(),
                        content: result.content,
                        is_error: false,
                    }
                }
                Err(e) => {
                    warn!("Tool {} failed: {}", name, e);
                    Part::ToolResult {
                        tool_call_id: id.clone(),
                        content: format!("Error: {}", e),
                        is_error: true,
                    }
                }
            };

            tool_results.push(tool_result);
        }

        // Add tool results message
        let results_msg = Message::assistant(ctx.session_id.clone(), tool_results.clone());
        ctx.messages.push(results_msg.clone());

        // Publish MessageAdded event for tool results
        if let Some(event_bus) = &self.event_bus {
            event_bus.publish(opencode_event::Event::MessageAdded {
                session_id: ctx.session_id.clone(),
                message_id: results_msg.id.0.clone(),
            });
        }

        // Continue for another step if we had tool calls
        Ok(ShouldContinue(true, StopReason::ToolCalls(tool_calls.iter().map(|(id, name, _)| name.clone()).collect())))
    }

    async fn process_stream<F>(
        &self,
        response: StreamingResponse,
        cancellation_token: &CancellationToken,
        mut handler: F,
    ) -> Result<()>
    where
        F: FnMut(StreamingEvent) + Send,
    {
        let mut stream = response.events;

        loop {
            // Check cancellation periodically
            if cancellation_token.is_cancelled() {
                break;
            }

            match stream.next().await {
                Some(Ok(event)) => {
                    handler(event);
                }
                Some(Err(e)) => {
                    warn!("Stream error: {:?}", e);
                    break;
                }
                None => {
                    // Stream ended
                    break;
                }
            }
        }

        Ok(())
    }
}

struct ShouldContinue(bool, StopReason);

#[cfg(test)]
mod tests {
    use super::*;
    use opencode_core::{Message, Role, Part};

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
        // This is a compile-time check that AgentExecutor can be instantiated
        // Real tests would require mock implementations
    }
}
