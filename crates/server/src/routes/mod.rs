//! Server routes

pub mod terminal;
pub mod diff;

use axum::{
    extract::{Path, State, Query},
    response::sse::{Event, Sse},
    Json,
};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;
use crate::error::ServerError;
use rcode_core::{
    Session, SessionId, SessionStatus, Message, Part, MessageId, 
    PaginationParams, PaginatedMessages, AgentContext, save_config, ProviderConfig, AgentDefinition, DynamicAgent,
};
use rcode_agent::{AgentExecutor, DefaultAgent};
use rcode_providers::ProviderFactory;
use tracing::{debug, error, info, warn, Instrument};

/// Adapter to wrap rcode_providers::LlmProvider and expose it as rcode_core::LlmProvider
/// This allows using production providers (via ProviderFactory) with TitleGenerator
/// which expects rcode_core::LlmProvider
struct ProviderAdapter {
    inner: Arc<dyn rcode_providers::LlmProvider>,
}

impl ProviderAdapter {
    fn new(inner: Arc<dyn rcode_providers::LlmProvider>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl rcode_core::LlmProvider for ProviderAdapter {
    async fn complete(&self, req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::CompletionResponse> {
        self.inner.complete(req).await
    }

    async fn stream(&self, req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::StreamingResponse> {
        self.inner.stream(req).await
    }

    fn model_info(&self, model_id: &str) -> Option<rcode_core::ModelInfo> {
        self.inner.model_info(model_id)
    }
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Session>>, ServerError> {
    let sessions = state.session_service.list_all();
    Ok(Json(sessions.into_iter().map(|s| (*s).clone()).collect()))
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<Session>, ServerError> {
    let model = state.config.lock().unwrap().effective_model().unwrap_or_else(|| "claude-sonnet-4-5".to_string());
    
    let session = if let Some(ref parent_id) = req.parent_id {
        // T11: Create child session inheriting parent's project_path
        // project_path is optional for child sessions - inherited from parent
        let agent_id = req.agent_id.unwrap_or_else(|| "build".to_string());
        let model_id = req.model_id.unwrap_or_else(|| model.clone());
        state.session_service
            .create_child(parent_id, agent_id, model_id)
            .map_err(|_e| ServerError::not_found())?
    } else {
        // Original behavior: create top-level session
        // project_path is required for top-level sessions
        let project_path = req.project_path
            .ok_or_else(|| ServerError::bad_request("project_path is required for top-level sessions"))?;
        let session = Session::new(
            project_path.into(),
            req.agent_id.unwrap_or_else(|| "build".to_string()),
            req.model_id.unwrap_or_else(|| model),
        );
        state.session_service.create(session)
    };
    
    Ok(Json(session.as_ref().clone()))
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Session>, ServerError> {
    let session = state.session_service.get(&SessionId(id))
        .ok_or_else(|| ServerError::not_found())?;
    Ok(Json(session.as_ref().clone()))
}

/// T12: Get child sessions for a parent session
pub async fn get_session_children(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Session>>, ServerError> {
    // First verify the parent session exists
    let _session = state.session_service.get(&SessionId(id.clone()))
        .ok_or_else(|| ServerError::not_found())?;
    
    let children = state.session_service.get_children(&id);
    Ok(Json(children.into_iter().map(|s| (*s).clone()).collect()))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<()>, ServerError> {
    let deleted = state.session_service.delete(&id);
    if !deleted {
        return Err(ServerError::not_found());
    }
    Ok(Json(()))
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_offset")]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_offset() -> usize { 0 }
fn default_limit() -> usize { 50 }

const FAST_PATH_SHELL_COMMANDS: &[&str] = &[
    "pwd",
    "ls",
    "whoami",
    "date",
    "uname",
    "id",
    "env",
    "printenv",
    "which",
];

const FAST_PATH_SHELL_OPERATORS: &[char] = &['|', ';', '&', '>', '<', '`'];

fn is_safe_shell_arg(arg: &str) -> bool {
    !arg.is_empty()
        && arg.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(ch, '/' | '.' | '_' | '-' | ':' | '=' | '+' | ',' | '@')
        })
}

fn parse_fast_path_shell_command(
    prompt: &str,
    allowed_tools: Option<&[String]>,
) -> Option<String> {
    if let Some(tools) = allowed_tools {
        if !tools.iter().any(|tool| tool == "bash") {
            return None;
        }
    }

    let command = prompt.trim();
    if command.is_empty() || command.contains('\n') || command.contains('\r') {
        return None;
    }

    if FAST_PATH_SHELL_OPERATORS.iter().any(|op| command.contains(*op)) {
        return None;
    }

    let mut parts = command.split_whitespace();
    let binary = parts.next()?;
    if !FAST_PATH_SHELL_COMMANDS.contains(&binary) {
        return None;
    }

    if !parts.all(is_safe_shell_arg) {
        return None;
    }

    Some(command.to_string())
}

impl Default for PaginationQuery {
    fn default() -> Self {
        Self { offset: 0, limit: 50 }
    }
}

pub async fn get_messages(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<PaginationQuery>,
) -> Result<Json<PaginatedMessages>, ServerError> {
    // First check if session exists
    let _session = state.session_service.get(&SessionId(id.clone()))
        .ok_or_else(|| ServerError::not_found())?;

    let pagination = PaginationParams::new(params.offset, params.limit);
    let result = state.session_service
        .get_messages_paginated(&id, &pagination)
        .map_err(|e| ServerError::internal(e))?;

    Ok(Json(result))
}

pub async fn submit_prompt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PromptRequest>,
) -> Result<Json<PromptResponse>, ServerError> {
    let request_id = Uuid::new_v4().to_string();
    info!(session_id = %id, prompt_len = req.prompt.len(), requested_model = ?req.model_id, "submit prompt received");
    info!(session_id = %id, request_id = %request_id, "assigned prompt request id");

    // T4: Check for concurrent prompt — reject if session already has active executor run
    if state.cancellation.is_active(&id) {
        warn!(session_id = %id, "rejecting prompt because session is already running");
        return Err(ServerError::conflict("session already running"));
    }

    // D5: Check session exists first
    let session = state.session_service.get(&SessionId(id.clone()))
        .ok_or_else(|| ServerError::not_found())?;
    
    // Get agent name from session
    let agent_name = &session.agent_id;
    
    // D5: Build provider FIRST, before setting Running status
    // Resolve model using hierarchy: req.model_id > agent_config.model > session.model_id > config.model
    let config = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
    
    // Get agent config for this agent (if any)
    let agent_config = config.agent.as_ref()
        .and_then(|agents| agents.get(agent_name));
    
    // Resolve model_id using the full hierarchy:
    // req.model_id > config.model_for_agent(agent_name) > session.model_id
    let model_id = req.model_id
        .clone()
        .or_else(|| config.model_for_agent(agent_name).map(|s| s.to_string()))
        .unwrap_or_else(|| session.model_id.clone());
    
    // Get max_tokens and reasoning_effort from agent config
    let max_tokens_override = agent_config.and_then(|ac| ac.max_tokens);
    let reasoning_effort_override = agent_config.as_ref()
        .and_then(|ac| ac.reasoning_effort.clone());
    let allowed_tools = config.tools_for_agent(agent_name);
    
    // Get auto_compact setting for executor
    let auto_compact = config.auto_compact;
    let compact_threshold_messages = config.compact_threshold_messages;
    let compact_keep_messages = config.compact_keep_messages;
    
    // Resolve title model: small_model > model_for_agent("title") > effective_model
    // Note: effective_model() returns Option<String>, so we convert others to String for consistency
    let title_model = config.effective_small_model()
        .map(|s| s.to_string())
        .or_else(|| config.model_for_agent("title").map(|s| s.to_string()))
        .or_else(|| config.effective_model());

    if let Some(command) = parse_fast_path_shell_command(&req.prompt, allowed_tools.as_deref()) {
        drop(config);

        let was_set = state.session_service.update_status(&id, SessionStatus::Running);
        if !was_set {
            warn!(session_id = %id, "failed to transition session to running for fast-path command");
            return Err(ServerError::conflict("Session is already running or in an invalid state"));
        }

        let token = state.cancellation.register(&id);
        let pre_existing_count = state.session_service.get_messages(&id).len();
        info!(
            session_id = %id,
            request_id = %request_id,
            command = %command,
            pre_existing_count,
            "handling prompt with direct bash fast-path"
        );

        let message = Message::user(id.clone(), vec![Part::Text {
            content: req.prompt.clone(),
        }]);
        state.session_service.add_message(&id, message.clone());

        let title_gen_provider = if pre_existing_count == 0 {
            if let Some(ref model_str) = title_model {
                let config_guard = state.config.lock().ok();
                config_guard.and_then(|g| {
                    ProviderFactory::build(model_str, Some(&*g))
                        .ok()
                        .map(|(p, m)| (p, m))
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some((provider, model_name)) = title_gen_provider {
            let session_service = Arc::clone(&state.session_service);
            let session_id = id.clone();
            let prompt_content = req.prompt.clone();

            tokio::spawn(async move {
                let title_gen = rcode_session::TitleGenerator::new(
                    Arc::new(ProviderAdapter::new(provider)),
                    model_name,
                );
                match title_gen.generate_title(&prompt_content).await {
                    Ok(title) => {
                        if let Err(e) = session_service.update_title(&session_id, title) {
                            tracing::warn!("Failed to update session title: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Title generation failed: {}", e);
                    }
                }
            });
        }

        let session_service = Arc::clone(&state.session_service);
        let event_bus = Arc::clone(&state.event_bus);
        let tools = Arc::clone(&state.tools);
        let cancellation = Arc::clone(&state.cancellation);
        let session_id_clone = id.clone();
        let project_path = session.project_path.clone();
        let request_id_clone = request_id.clone();
        let agent_name_clone = agent_name.to_string();
        let command_clone = command.clone();

        tokio::spawn(async move {
            let result = async {
                if token.is_cancelled() {
                    let cancellation_message = Message::assistant(session_id_clone.clone(), vec![Part::Text {
                        content: "Execution cancelled by user".to_string(),
                    }]);
                    session_service.add_message(&session_id_clone, cancellation_message.clone());
                    event_bus.publish(rcode_event::Event::MessageAdded {
                        session_id: session_id_clone.clone(),
                        message_id: cancellation_message.id.0.clone(),
                    });
                    let _ = session_service.update_status(&session_id_clone, SessionStatus::Aborted);
                    return;
                }

                let cwd = std::env::current_dir().unwrap_or_else(|_| project_path.clone());
                let tool_ctx = rcode_core::ToolContext {
                    session_id: session_id_clone.clone(),
                    project_path: project_path.clone(),
                    cwd,
                    user_id: None,
                    agent: agent_name_clone.clone(),
                };

                let tool_call_id = MessageId::new().0;
                let tool_call_message = Message::assistant(session_id_clone.clone(), vec![Part::ToolCall {
                    id: tool_call_id.clone(),
                    name: "bash".to_string(),
                    arguments: Box::new(serde_json::json!({ "command": command_clone })),
                }]);
                session_service.add_message(&session_id_clone, tool_call_message.clone());
                event_bus.publish(rcode_event::Event::MessageAdded {
                    session_id: session_id_clone.clone(),
                    message_id: tool_call_message.id.0.clone(),
                });

                let tool_result = tools
                    .execute("bash", serde_json::json!({ "command": command }), &tool_ctx)
                    .await;

                let (content, is_error) = match tool_result {
                    Ok(result) => (result.content, false),
                    Err(error) => (format!("Error: {}", error), true),
                };

                let result_message = Message::assistant(session_id_clone.clone(), vec![Part::ToolResult {
                    tool_call_id,
                    content,
                    is_error,
                }]);
                session_service.add_message(&session_id_clone, result_message.clone());
                event_bus.publish(rcode_event::Event::MessageAdded {
                    session_id: session_id_clone.clone(),
                    message_id: result_message.id.0.clone(),
                });

                if is_error {
                    event_bus.publish(rcode_event::Event::AgentError {
                        session_id: session_id_clone.clone(),
                        agent_id: agent_name_clone.clone(),
                        error: result_message.parts.iter().find_map(|part| match part {
                            Part::ToolResult { content, .. } => Some(content.clone()),
                            _ => None,
                        }).unwrap_or_else(|| "Tool execution failed".to_string()),
                    });
                }

                let _ = session_service.update_status(&session_id_clone, SessionStatus::Idle);
            }.await;

            cancellation.remove(&session_id_clone);
            event_bus.publish(rcode_event::Event::AgentFinished {
                session_id: session_id_clone.clone(),
            });
            info!(request_id = %request_id_clone, session_id = %session_id_clone, "fast-path command finished");
            result
        });

        return Ok(Json(PromptResponse {
            message_id: MessageId::new().0,
            request_id,
            status: "processing".to_string(),
        }));
    }

    // Check for mock provider first (used by integration tests)
    let provider_build_result = if let Ok(mock_guard) = state.mock_provider.lock() {
        if let Some(ref mock) = *mock_guard {
            Ok((Arc::clone(mock) as Arc<dyn rcode_providers::LlmProvider>, model_id.clone()))
        } else {
            drop(mock_guard);
            ProviderFactory::build(&model_id, Some(&*config))
        }
    } else {
        ProviderFactory::build(&model_id, Some(&*config))
    };

    // If provider build fails, try to reset session to Idle so user can retry
    let (provider, effective_model) = match provider_build_result {
        Ok((p, m)) => (p, m),
        Err(e) => {
            // Reset session to Idle on error so user can try again
            let _ = state.session_service.update_status(&id, SessionStatus::Idle);
            error!(session_id = %id, request_id = %request_id, model_id = %model_id, error = %e, "failed to build provider for prompt");
            return Err(ServerError::internal(e.to_string()));
        }
    };
    drop(config); // Release lock before spawning
    
    // D5: Gate concurrent access - only allow if session is Idle, AFTER provider build succeeds
    let was_set = state.session_service.update_status(&id, SessionStatus::Running);
    if !was_set {
        warn!(session_id = %id, "failed to transition session to running");
        return Err(ServerError::conflict("Session is already running or in an invalid state"));
    }
    
    // T4: Register cancellation token BEFORE spawning
    let token = state.cancellation.register(&id);
    
    // D5: Track pre-existing message count for deduplication (captured after provider built)
    let pre_existing_count = state.session_service.get_messages(&id).len();
    info!(session_id = %id, request_id = %request_id, effective_model = %effective_model, pre_existing_count, "prompt accepted and provider configured");
    let message = Message::user(id.clone(), vec![Part::Text {
        content: req.prompt.clone(),
    }]);
    state.session_service.add_message(&id, message.clone());
    
    // T3: If this is the first message in the session, spawn async title generation
    // We build the provider BEFORE spawning to avoid holding MutexGuard across await
    let title_gen_provider = if pre_existing_count == 0 {
        if let Some(ref model_str) = title_model {
            let config_guard = state.config.lock().ok();
            config_guard.and_then(|g| {
                ProviderFactory::build(model_str, Some(&*g))
                    .ok()
                    .map(|(p, m)| (p, m))
            })
        } else {
            None
        }
    } else {
        None
    };

    if let Some((provider, model_name)) = title_gen_provider {
        let session_service = Arc::clone(&state.session_service);
        let session_id = id.clone();
        let prompt_content = req.prompt.clone();
        
        tokio::spawn(async move {
            let title_gen = rcode_session::TitleGenerator::new(
                Arc::new(ProviderAdapter::new(provider)),
                model_name
            );
            match title_gen.generate_title(&prompt_content).await {
                Ok(title) => {
                    if let Err(e) = session_service.update_title(&session_id, title) {
                        tracing::warn!("Failed to update session title: {}", e);
                    }
                }
                Err(e) => {
                    tracing::warn!("Title generation failed: {}", e);
                }
            }
        });
    }
    
    // C9: Get FULL session history (includes the new message we just added)
    let messages = state.session_service.get_messages(&id);
    
    let config_snapshot = {
        let guard = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
        (*guard).clone()
    };

    // Create the agent from OpenCode-compatible agent config when available.
    let agent: Arc<dyn rcode_core::Agent> = if let Some(agent_cfg) = config_snapshot
        .agent
        .as_ref()
        .and_then(|agents| agents.get(agent_name.as_str()))
    {
        let definition = AgentDefinition {
            identifier: agent_name.clone(),
            name: agent_name.clone(),
            description: agent_cfg.description.clone().unwrap_or_else(|| format!("{} agent", agent_name)),
            when_to_use: String::new(),
            system_prompt: agent_cfg.prompt.clone().unwrap_or_else(DefaultAgent::system_prompt_text),
            mode: match agent_cfg.mode.clone().unwrap_or_default() {
                rcode_core::AgentMode::Primary => rcode_core::AgentDefinitionMode::Primary,
                rcode_core::AgentMode::Subagent => rcode_core::AgentDefinitionMode::Subagent,
                rcode_core::AgentMode::All => rcode_core::AgentDefinitionMode::All,
            },
            hidden: agent_cfg.hidden.unwrap_or(false),
            permission: Default::default(),
            tools: config_snapshot.tools_for_agent(&agent_name).unwrap_or_default(),
            model: agent_cfg.model.clone(),
            max_tokens: agent_cfg.max_tokens,
            reasoning_effort: agent_cfg.reasoning_effort.clone(),
        };
        DynamicAgent::from_definition(definition)
    } else {
        Arc::new(DefaultAgent::new())
    };
    
    // Build the executor with all overrides
    // Note: InteractivePermissionService wiring is pending due to type inference issues
    // with axum Handler trait. Using AutoPermissionService for now.
    let mut executor = AgentExecutor::new(
        agent,
        provider,
        Arc::clone(&state.tools),
    )
    .with_event_bus(Arc::clone(&state.event_bus));

    if let Some(tools) = allowed_tools.clone() {
        executor = executor.with_allowed_tools(tools);
    }
    
    // T9: Apply model override if provided in request (explicit request override wins)
    if let Some(ref model_override) = req.model_id {
        executor = executor.with_model_override(model_override.clone());
    }
    
    // Apply max_tokens override from agent config
    if let Some(max_tokens) = max_tokens_override {
        executor = executor.with_max_tokens_override(max_tokens);
    }
    
    // Apply reasoning_effort override from agent config
    if let Some(reasoning_effort) = reasoning_effort_override {
        executor = executor.with_reasoning_effort(reasoning_effort);
    }
    
    // Apply auto_compact setting if enabled
    if auto_compact {
        executor = executor.with_auto_compact(true);
        
        // Apply custom compaction thresholds if specified
        if let (Some(threshold), Some(keep)) = (compact_threshold_messages, compact_keep_messages) {
            executor = executor.with_compaction_thresholds(threshold, keep);
        }
    }
    
    // Create agent context
    let cwd = std::env::current_dir()
        .unwrap_or_else(|_| session.project_path.clone());
    
    let mut ctx = AgentContext {
        session_id: id.clone(),
        project_path: session.project_path.clone(),
        cwd,
        user_id: None,
        model_id: effective_model,
        messages,
    };
    
    // Run the executor in a spawned task to avoid blocking the server
    let session_service = Arc::clone(&state.session_service);
    let event_bus = Arc::clone(&state.event_bus);
    let cancellation = Arc::clone(&state.cancellation);
    let session_id_clone = id.clone();
    let agent_name_clone = agent_name.to_string();
    let request_id_clone = request_id.clone();
    let prompt_span = tracing::info_span!("executor_run", session_id = %session_id_clone, request_id = %request_id_clone, model = %ctx.model_id);

    tokio::spawn(async move {
        info!(request_id = %request_id_clone, message_count = ctx.messages.len(), "executor task started");
        // T4: Run with cancellation token
        let result = executor.run_with_cancellation(&mut ctx, token).await;
        
        // T4: Deregister token when executor finishes (finally block equivalent)
        cancellation.remove(&session_id_clone);
        
        // G4: Extract and persist token usage from executor result
        if let Ok(ref agent_result) = result {
            if let Some(ref usage) = agent_result.usage {
                // Update session metadata with usage
                let prompt_toks = usage.input_tokens as u64;
                let completion_toks = usage.output_tokens as u64;
                // Approximate cost calculation (Claude 3.5 Sonnet rates: ~$3/M input, ~$15/M output)
                let cost = (prompt_toks as f64 * 3.0 / 1_000_000.0) 
                    + (completion_toks as f64 * 15.0 / 1_000_000.0);
                
                // Update session in memory
                if let Some(session) = session_service.get(&rcode_core::SessionId(session_id_clone.clone())) {
                    let mut session_mut = (*session).clone();
                    session_mut.add_usage(prompt_toks, completion_toks, cost);
                    // Persist to storage
                    if let Some(repo) = session_service.session_repo() {
                        let _ = repo.update_usage(&session_id_clone, session_mut.prompt_tokens, 
                            session_mut.completion_tokens, session_mut.total_cost_usd);
                    }
                }
            }
        }
        
        match result {
            Ok(agent_result) => {
                let persisted_new_messages = ctx.messages.iter().skip(pre_existing_count)
                    .filter(|msg| msg.role == rcode_core::Role::Assistant)
                    .count();

                // Persist assistant result message if the executor returned one but did not append it to ctx.messages.
                if agent_result.message.role == rcode_core::Role::Assistant
                    && !agent_result.message.parts.is_empty()
                    && persisted_new_messages == 0
                {
                    debug!(request_id = %request_id_clone, message_id = %agent_result.message.id.0, part_count = agent_result.message.parts.len(), "persisting terminal assistant result message");
                    session_service.add_message(&session_id_clone, agent_result.message.clone());
                    event_bus.publish(rcode_event::Event::MessageAdded {
                        session_id: session_id_clone.clone(),
                        message_id: agent_result.message.id.0.clone(),
                    });
                } else {
                    let new_messages = ctx.messages.iter().skip(pre_existing_count);
                    for msg in new_messages {
                        if msg.role == rcode_core::Role::Assistant {
                            debug!(request_id = %request_id_clone, message_id = %msg.id.0, part_count = msg.parts.len(), "persisting new assistant message");
                            session_service.add_message(&session_id_clone, msg.clone());
                        }
                    }
                }

                info!(request_id = %request_id_clone, stop_reason = ?agent_result.stop_reason, has_usage = agent_result.usage.is_some(), "executor finished successfully");
                // G5: If user cancelled (via abort), set to Aborted; otherwise set to Idle
                if matches!(agent_result.stop_reason, rcode_core::agent::StopReason::UserStopped) {
                    let _ = session_service.update_status(&session_id_clone, SessionStatus::Aborted);
                } else {
                    let _ = session_service.update_status(&session_id_clone, SessionStatus::Idle);
                }

                if matches!(agent_result.stop_reason, rcode_core::agent::StopReason::Error) {
                    let error_message = agent_result.message.parts.iter().find_map(|part| match part {
                        Part::Text { content } => Some(content.clone()),
                        _ => None,
                    }).unwrap_or_else(|| "Agent execution failed".to_string());

                    event_bus.publish(rcode_event::Event::AgentError {
                        session_id: session_id_clone.clone(),
                        agent_id: agent_name_clone.clone(),
                        error: error_message,
                    });
                }
            }
            Err(e) => {
                error!(request_id = %request_id_clone, error = %e, "agent execution failed");
                let _ = session_service.update_status(&session_id_clone, SessionStatus::Aborted);
            }
        }
        
        // Publish agent finished event
        event_bus.publish(rcode_event::Event::AgentFinished {
            session_id: session_id_clone,
        });
    }.instrument(prompt_span));
    
    Ok(Json(PromptResponse {
        message_id: MessageId::new().0,
        request_id,
        status: "processing".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct PromptRequest {
    pub prompt: String,
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PromptResponse {
    pub message_id: String,
    pub request_id: String,
    pub status: String,
}

pub async fn abort_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<()>, ServerError> {
    // T5: Cancel the executor if there's an active run
    state.cancellation.cancel(&id);
    
    let updated = state.session_service.update_status(&id, SessionStatus::Aborted);
    if !updated {
        return Err(ServerError::invalid_transition("Cannot abort session in current state"));
    }
    Ok(Json(()))
}

pub async fn sse_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, axum::Error>>> {
    let mut subscriber = state.event_bus.subscribe();
    
    let stream = async_stream::stream! {
        loop {
            match subscriber.recv().await {
                Ok(event) => {
                    let event_name = event.event_type();
                    let data = serde_json::to_string(&event).unwrap_or_else(|e| {
                        format!("{{\"error\":\"serialization failed: {}\"}}", e)
                    });
                    yield Ok(Event::default()
                        .event(event_name)
                        .data(data));
                }
                Err(_) => break,
            }
        }
    };
    
    Sse::new(stream)
}

/// Per-session SSE events - streams events filtered by session_id
pub async fn sse_session_events(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, axum::Error>>>, ServerError> {
    // Verify session exists
    state.session_service.get(&SessionId(session_id.clone()))
        .ok_or_else(|| ServerError::not_found())?;
    
    let mut subscriber = state.event_bus.subscribe_for_session(&session_id);
    info!(session_id = %session_id, "opening session SSE stream");
    
    let stream = async_stream::stream! {
        loop {
            match subscriber.recv().await {
                Ok(event) => {
                    let event_name = event.event_type();
                    debug!(session_id = %session_id, event_type = event_name, "sending SSE event");
                    let data = serde_json::to_string(&event).unwrap_or_else(|e| {
                        format!("{{\"error\":\"serialization failed: {}\"}}", e)
                    });
                    yield Ok(Event::default()
                        .event(event_name)
                        .data(data));
                }
                Err(e) => {
                    warn!(session_id = %session_id, error = %e, "session SSE subscriber closed");
                    break;
                }
            }
        }
    };
    
    Ok(Sse::new(stream))
}

/// Response for GET /models
#[derive(serde::Serialize)]
pub struct ListModelsResponse {
    pub models: Vec<CatalogModelDto>,
}

#[derive(serde::Serialize)]
pub struct CatalogModelDto {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub has_credentials: bool,
    pub source: String,
    pub enabled: bool,
}

/// GET /models - List all available models
pub async fn list_models(
    State(state): State<Arc<AppState>>,
) -> axum::Json<ListModelsResponse> {
    let config = (*state.config.lock().unwrap()).clone();
    let models = state.catalog.list_models(&config).await;
    
    let dto_models: Vec<CatalogModelDto> = models.into_iter().map(|m| {
        CatalogModelDto {
            id: m.id,
            provider: m.provider,
            display_name: m.display_name,
            has_credentials: m.has_credentials,
            source: format!("{:?}", m.source).to_lowercase(),
            enabled: m.enabled,
        }
    }).collect();
    
    axum::Json(ListModelsResponse { models: dto_models })
}

/// POST /connect - Switch the active model for a session
pub async fn connect_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConnectRequest>,
) -> Result<Json<ConnectResponse>, ServerError> {
    // Don't validate model existence against list — just verify session exists
    // and update the model. Let inference fail if model is invalid.
    let _session = state.session_service.get(&SessionId(req.session_id.clone()))
        .ok_or_else(|| ServerError::not_found())?;
    
    // Update the session's model
    state.session_service
        .update_model(&req.session_id, req.model_id.clone())
        .map_err(|e| ServerError::internal(e))?;
    
    Ok(Json(ConnectResponse {
        ok: true,
        session_id: req.session_id,
        model_id: req.model_id,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub session_id: String,
    pub model_id: String,
}

#[derive(Debug, Serialize)]
pub struct ConnectResponse {
    pub ok: bool,
    pub session_id: String,
    pub model_id: String,
}

/// GET /config - Returns safe config (no API keys exposed)
pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let config = state.config.lock().unwrap();
    let mut safe_config = serde_json::to_value(&*config).unwrap_or(serde_json::Value::Object(Default::default()));
    
    // Remove API keys from typed providers
    if let Some(providers) = safe_config.get_mut("providers").and_then(|p| p.as_object_mut()) {
        for (_provider_id, provider_config) in providers.iter_mut() {
            if let Some(obj) = provider_config.as_object_mut() {
                obj.remove("api_key");
                obj.remove("key");
            }
        }
    }
    
    Json(safe_config)
}

/// PUT /config - Updates config
#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub providers: Option<serde_json::Value>,
    pub model: Option<String>,
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateConfigRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    // Clone the current config FIRST to avoid mutating state before save succeeds.
    let mut config = {
        let guard = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
        (*guard).clone()
    };

    // Apply changes to the clone (not the live state)
    if let Some(model) = req.model {
        config.model = Some(model);
    }

    // Persist the clone to disk. If this fails, live state is untouched.
    save_config(&config).map_err(|e| ServerError::internal(e))?;

    // Only update live state AFTER successful disk write
    {
        let mut guard = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
        *guard = config;
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /config/providers - Returns provider status
pub async fn get_providers(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let config = state.config.lock().unwrap();

    // Check for credentials in auth.json (OpenCode's primary credential store)
    // Note: In OpenCode, the user picks a provider ID via /connect which may
    // differ from the canonical provider name (e.g. "zai-coding-plan" vs "zai").
    // We check the provider_id first, then known model names as fallback.
    let check_has_key = |provider_id: &str| -> bool {
        // First check auth.json for the provider_id
        if rcode_core::auth::has_credential(provider_id) {
            return true;
        }
        // Also check known model names (e.g. "zai-coding-plan" for "zai")
        let model_names: &[&str] = match provider_id {
            "zai" => &["zai-coding-plan", "zai-coding-standard", "zai-coding-premium"],
            _ => &[],
        };
        for name in model_names {
            if rcode_core::auth::has_credential(name) {
                return true;
            }
        }
        // Then check env vars
        let env_key = format!("{}_API_KEY", provider_id.to_uppercase().replace('-', "_"));
        if std::env::var(&env_key).is_ok() {
            return true;
        }
        let auth_key = format!("{}_AUTH_TOKEN", provider_id.to_uppercase().replace('-', "_"));
        if std::env::var(&auth_key).is_ok() {
            return true;
        }
        // Then check config (deprecated - api_key should be in auth.json)
        config
            .providers
            .get(provider_id)
            .and_then(|p| p.api_key.as_deref())
            .map(|k| !k.is_empty())
            .unwrap_or(false)
    };

    // Determine the source of the API key: "auth", "env", or "config"
    let get_key_source = |provider_id: &str| -> &'static str {
        if rcode_core::auth::has_credential(provider_id) {
            return "auth";
        }
        // Also check model names for auth source
        let model_names: &[&str] = match provider_id {
            "zai" => &["zai-coding-plan", "zai-coding-standard", "zai-coding-premium"],
            _ => &[],
        };
        for name in model_names {
            if rcode_core::auth::has_credential(name) {
                return "auth";
            }
        }
        let env_key = format!("{}_API_KEY", provider_id.to_uppercase().replace('-', "_"));
        if std::env::var(&env_key).is_ok() {
            return "env";
        }
        let auth_key = format!("{}_AUTH_TOKEN", provider_id.to_uppercase().replace('-', "_"));
        if std::env::var(&auth_key).is_ok() {
            return "env";
        }
        if config
            .providers
            .get(provider_id)
            .and_then(|p| p.api_key.as_deref())
            .map(|k| !k.is_empty())
            .unwrap_or(false)
        {
            return "config";
        }
        "none"
    };

    let get_base_url = |provider_id: &str| -> serde_json::Value {
        if let Some(url) = config
            .providers
            .get(provider_id)
            .and_then(|p| p.base_url.clone())
        {
            return serde_json::Value::String(url);
        }

        let env_key = format!("{}_BASE_URL", provider_id.to_uppercase().replace('-', "_"));
        std::env::var(&env_key)
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null)
    };

    // Well-known providers shown by default (id → display name)
    let known: &[(&str, &str)] = &[
        ("anthropic",  "Anthropic"),
        ("openai",     "OpenAI"),
        ("google",     "Google"),
        ("openrouter", "OpenRouter"),
        ("minimax",    "MiniMax"),
        ("zai",        "ZAI"),
    ];

    // Build ordered list: known providers first, then any additional ones
    // that exist in the config (e.g. custom or third-party providers).
    let mut seen = std::collections::HashSet::new();
    let mut list: Vec<serde_json::Value> = Vec::new();

    for (id, name) in known {
        seen.insert(*id);
        list.push(serde_json::json!({
            "id":        id,
            "name":      name,
            "has_key":   check_has_key(id),
            "key_source": get_key_source(id),
            "base_url":  get_base_url(id),
            "enabled":   true,
        }));
    }

    // Add providers from config that are not in the well-known list
    for (id, _provider_cfg) in config.providers.iter() {
        if seen.contains(id.as_str()) {
            continue;
        }
        // Derive a display name: title-case the id, replace dashes/underscores with spaces
        let name = id
            .replace('-', " ")
            .replace('_', " ")
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        list.push(serde_json::json!({
            "id":        id,
            "name":      name,
            "has_key":   check_has_key(id),
            "key_source": get_key_source(id),
            "base_url":  get_base_url(id),
            "enabled":   true,
        }));
    }

    Json(serde_json::json!({ "providers": list }))
}

/// PUT /config/providers/:id - Set provider config
#[derive(Debug, Deserialize)]
pub struct UpdateProviderRequest {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEnabledStateRequest {
    pub enabled: bool,
}

pub async fn update_provider(
    Path(provider_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    // Clone the current config FIRST to avoid mutating state before save succeeds.
    let mut config = {
        let guard = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
        (*guard).clone()
    };

    // Apply changes to the clone (not the live state)
    let provider_config = config
        .providers
        .entry(provider_id.clone())
        .or_insert_with(ProviderConfig::default);

    // API key goes to auth.json (OpenCode-compatible), NOT the config file
    if let Some(api_key) = req.api_key {
        // Save to auth.json - this is the OpenCode-compatible way
        use rcode_core::auth::{Credential, save_credential};
        save_credential(&provider_id, Credential::Api { key: api_key })
            .map_err(|e| ServerError::internal(format!("Failed to save credential: {}", e)))?;
    }
    
    // base_url stays in config file (not a secret)
    if let Some(base_url) = req.base_url {
        provider_config.base_url = Some(base_url);
    }

    // Persist config changes (base_url only, not api_key) to disk.
    // Use strip_secrets_from_config to ensure api_key is never written to config.
    let config_to_save = rcode_core::auth::strip_secrets_from_config(&config);
    save_config(&config_to_save).map_err(|e| ServerError::internal(e))?;

    // Only update live state AFTER successful disk write
    {
        let mut guard = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
        *guard = config;
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn update_provider_state(
    Path(provider_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateEnabledStateRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let mut config = {
        let guard = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
        (*guard).clone()
    };

    let mut disabled = config.disabled_providers.clone().unwrap_or_default();
    disabled.retain(|id| id != &provider_id);
    if !req.enabled {
        disabled.push(provider_id.clone());
    }
    config.disabled_providers = if disabled.is_empty() { None } else { Some(disabled) };

    let config_to_save = rcode_core::auth::strip_secrets_from_config(&config);
    save_config(&config_to_save).map_err(ServerError::internal)?;

    {
        let mut guard = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
        *guard = config;
    }

    Ok(Json(serde_json::json!({ "ok": true, "enabled": req.enabled })))
}

pub async fn update_model_state(
    Path(model_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateEnabledStateRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let mut config = {
        let guard = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
        (*guard).clone()
    };

    let mut disabled_models = config
        .extra
        .get("disabled_models")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect::<Vec<_>>();

    disabled_models.retain(|id| id != &model_id);
    if !req.enabled {
        disabled_models.push(model_id.clone());
    }

    if !config.extra.is_object() {
        config.extra = serde_json::json!({});
    }

    if let Some(extra) = config.extra.as_object_mut() {
        extra.insert(
            "disabled_models".to_string(),
            serde_json::Value::Array(disabled_models.into_iter().map(serde_json::Value::String).collect()),
        );
    }

    let config_to_save = rcode_core::auth::strip_secrets_from_config(&config);
    save_config(&config_to_save).map_err(ServerError::internal)?;

    {
        let mut guard = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
        *guard = config;
    }

    Ok(Json(serde_json::json!({ "ok": true, "enabled": req.enabled })))
}

// ========== Permission Grant/Deny Endpoints ==========

/// POST /permission/:request_id/grant - Grant permission for a pending request
pub async fn permission_grant(
    State(state): State<Arc<AppState>>,
    Path(request_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ServerError> {
    // Search for the permission service that has this pending request
    let perm_services = state.permission_services.lock().await;
    
    for (_session_id, perm_service) in perm_services.iter() {
        if perm_service.grant(request_id).await.is_ok() {
            return Ok(Json(serde_json::json!({
                "ok": true,
                "request_id": request_id.to_string(),
                "granted": true,
            })));
        }
    }
    
    Err(ServerError::not_found())
}

/// POST /permission/:request_id/deny - Deny permission for a pending request
pub async fn permission_deny(
    State(state): State<Arc<AppState>>,
    Path(request_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ServerError> {
    // Search for the permission service that has this pending request
    let perm_services = state.permission_services.lock().await;
    
    for (_session_id, perm_service) in perm_services.iter() {
        if perm_service.deny(request_id).await.is_ok() {
            return Ok(Json(serde_json::json!({
                "ok": true,
                "request_id": request_id.to_string(),
                "granted": false,
            })));
        }
    }
    
    Err(ServerError::not_found())
}

#[cfg(test)]
mod tests {
    use super::parse_fast_path_shell_command;

    #[test]
    fn fast_path_accepts_simple_allowed_command() {
        assert_eq!(
            parse_fast_path_shell_command("pwd", None),
            Some("pwd".to_string())
        );
        assert_eq!(
            parse_fast_path_shell_command("ls -la", None),
            Some("ls -la".to_string())
        );
    }

    #[test]
    fn fast_path_rejects_shell_operators() {
        assert_eq!(parse_fast_path_shell_command("pwd | cat", None), None);
        assert_eq!(parse_fast_path_shell_command("date; whoami", None), None);
    }

    #[test]
    fn fast_path_respects_allowed_tools() {
        let allowed = vec!["read".to_string(), "glob".to_string()];
        assert_eq!(parse_fast_path_shell_command("pwd", Some(&allowed)), None);

        let allowed = vec!["bash".to_string()];
        assert_eq!(
            parse_fast_path_shell_command("pwd", Some(&allowed)),
            Some("pwd".to_string())
        );
    }
}
