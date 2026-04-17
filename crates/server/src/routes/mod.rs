//! Server routes

pub mod terminal;
pub mod diff;
pub mod privacy;

use axum::{
    extract::{Path, State, Query},
    response::sse::{Event, Sse},
    Json,
};
use chrono::{DateTime, Utc};
use rcode_session::ProjectService;
use std::sync::Arc;
use std::path::{Path as FsPath, PathBuf};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;
use crate::error::ServerError;
use crate::explorer::{ExplorerBootstrap, TreeResponse, TreeFilter};
use rcode_core::{
    Project, ProjectId, Session, SessionId, SessionStatus, Message, Part, MessageId,
    PaginationParams, PaginatedMessages, AgentContext, save_config, ProviderConfig, AgentDefinition, DynamicAgent,
};
use rcode_agent::{AgentExecutor, DefaultAgent};
use rcode_providers::ProviderFactory;
use rcode_lsp::LanguageServerRegistry;
use tracing::{debug, error, info, warn, Instrument};

/// Adapter to wrap rcode_providers::LlmProvider and expose it as rcode_core::LlmProvider
/// This allows using production providers (via ProviderFactory) with TitleGenerator
/// which expects rcode_core::LlmProvider
struct ProviderAdapter {
    inner: Arc<dyn rcode_providers::LlmProvider>,
}

/// Helper async fn to sanitize prompt without capturing config in the async context.
///
/// This avoids the issue where holding a std::sync::MutexGuard (non-Send) across
/// an .await point makes the future non-Send.
async fn sanitize_prompt_async(
    privacy: &rcode_privacy::service::PrivacyService,
    session_id: &str,
    prompt: &str,
) -> String {
    privacy.sanitize_prompt(session_id, prompt).await
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

fn project_service(state: &AppState) -> Result<Arc<ProjectService>, ServerError> {
    state
        .project_service
        .clone()
        .ok_or_else(|| ServerError::internal("project service unavailable"))
}

fn resolve_project(state: &AppState, project_id: &str) -> Result<Project, ServerError> {
    project_service(state)?
        .get(&ProjectId(project_id.to_string()))
        .map_err(ServerError::internal)?
        .ok_or_else(|| ServerError::bad_request(format!("Unknown project_id: {project_id}")))
}

fn maybe_resolve_project_by_path(
    state: &AppState,
    project_path: &FsPath,
) -> Result<Option<Project>, ServerError> {
    let Some(project_service) = state.project_service.clone() else {
        return Ok(None);
    };

    if !project_path.exists() {
        return Ok(None);
    }

    project_service
        .resolve_by_path(project_path)
        .map_err(ServerError::internal)
}

async fn resolve_workspace_root(
    state: &AppState,
    project_id: Option<&str>,
    session_id: Option<&str>,
) -> Result<PathBuf, ServerError> {
    if let Some(project_id) = project_id {
        return Ok(resolve_project(state, project_id)?.canonical_path);
    }

    let session_id = session_id
        .ok_or_else(|| ServerError::bad_request("project_id or session_id is required"))?;

    state
        .session_service
        .get(&SessionId(session_id.to_string()))
        .map(|session| session.project_path.clone())
        .ok_or_else(ServerError::not_found)
}

fn to_project_summary(state: &AppState, project: Project) -> ProjectSummary {
    let session_count = state.session_service.list_by_project(&project.id).len();
    ProjectSummary {
        id: project.id.0,
        name: project.name,
        canonical_path: project.canonical_path.to_string_lossy().to_string(),
        session_count,
        created_at: project.created_at,
        updated_at: project.updated_at,
    }
}

pub async fn list_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ProjectSummary>>, ServerError> {
    let projects = project_service(state.as_ref())?
        .list()
        .map_err(ServerError::internal)?;

    Ok(Json(
        projects
            .into_iter()
            .map(|project| to_project_summary(state.as_ref(), project))
            .collect(),
    ))
}

pub async fn create_project(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<Json<ProjectSummary>, ServerError> {
    let project = project_service(state.as_ref())?
        .create(FsPath::new(&req.path), req.name)
        .map_err(|error| {
            if error.contains("already exists") {
                ServerError::conflict(error)
            } else {
                ServerError::bad_request(error)
            }
        })?;

    Ok(Json(to_project_summary(state.as_ref(), project)))
}

pub async fn list_project_sessions(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<Session>>, ServerError> {
    let project = resolve_project(state.as_ref(), &project_id)?;
    let sessions = state.session_service.list_by_project(&project.id);
    Ok(Json(
        sessions
            .into_iter()
            .map(|session| (*session).clone())
            .collect(),
    ))
}

pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let deleted = project_service(state.as_ref())?
        .delete(&ProjectId(project_id))
        .map_err(ServerError::internal)?;

    if !deleted {
        return Err(ServerError::not_found());
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<Session>, ServerError> {
    let model = state.config.lock()
        .map(|g| g.effective_model())
        .unwrap_or(None)
        .unwrap_or_else(|| "claude-sonnet-4-5".to_string());
    
    let session = if let Some(ref parent_id) = req.parent_id {
        // T11: Create child session inheriting parent's project_path
        // project_path is optional for child sessions - inherited from parent
        let agent_id = req.agent_id.unwrap_or_else(|| "build".to_string());
        let model_id = req.model_id.unwrap_or_else(|| model.clone());
        state.session_service
            .create_child(parent_id, agent_id, model_id)
            .map_err(|_e| ServerError::not_found())?
    } else if let Some(project_id) = req.project_id.as_deref() {
        let project = resolve_project(state.as_ref(), project_id)?;
        let mut session = Session::new(
            project.canonical_path.clone(),
            req.agent_id.unwrap_or_else(|| "build".to_string()),
            req.model_id.unwrap_or_else(|| model),
        );
        session.project_id = Some(project.id);
        state.session_service.create(session)
    } else {
        // Original behavior: create top-level session
        // project_path is required for top-level sessions
        let project_path = req.project_path
            .ok_or_else(|| ServerError::bad_request("project_path is required for top-level sessions"))?;
        let mut session = Session::new(
            project_path.clone().into(),
            req.agent_id.unwrap_or_else(|| "build".to_string()),
            req.model_id.unwrap_or_else(|| model),
        );

        if let Some(project) = maybe_resolve_project_by_path(state.as_ref(), FsPath::new(&project_path))? {
            session.project_id = Some(project.id);
            session.project_path = project.canonical_path;
        }

        state.session_service.create(session)
    };
    
    Ok(Json(session.as_ref().clone()))
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub canonical_path: String,
    pub session_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Session>, ServerError> {
    let session = state.session_service.get(&SessionId(id))
        .ok_or_else(|| ServerError::not_found())?;
    Ok(Json(session.as_ref().clone()))
}

/// PATCH /session/:id - Rename a session (manual rename, bypasses auto-title idempotent guard)
#[derive(Debug, Deserialize)]
pub struct RenameSessionRequest {
    pub title: String,
}

pub async fn rename_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<RenameSessionRequest>,
) -> Result<Json<Session>, ServerError> {
    state.session_service
        .force_set_title(&id, req.title)
        .map_err(|e| ServerError::internal(e.to_string()))?;

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

#[axum::debug_handler]
pub async fn submit_prompt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PromptRequest>,
) -> Result<Json<PromptResponse>, ServerError> {
    let request_id = Uuid::new_v4().to_string();
    info!(session_id = %id, prompt_len = req.prompt.len(), requested_model = ?req.model_id, "submit prompt received");
    info!(session_id = %id, request_id = %request_id, "assigned prompt request id");

    // T4: Check for concurrent prompt — reject if session already has active executor run
    debug!(session_id = %id, request_id = %request_id, "checking cancellation active");
    if state.cancellation.is_active(&id) {
        warn!(session_id = %id, "rejecting prompt because session is already running");
        // T-02: Return structured details for SESSION_ALREADY_RUNNING conflict
        return Err(ServerError::conflict_with_details(
            "session already running",
            serde_json::json!({
                "code": "SESSION_ALREADY_RUNNING",
                "session_id": id
            }),
        ));
    }

    // D5: Check session exists first
    debug!(session_id = %id, request_id = %request_id, "looking up session");
    let session = state.session_service.get(&SessionId(id.clone()))
        .ok_or_else(|| ServerError::not_found())?;
    
    // Get agent name from session
    let agent_name = &session.agent_id;
    debug!(session_id = %id, request_id = %request_id, agent_name = %agent_name, session_model = %session.model_id, "session found, about to sanitize prompt");

    // Privacy: sanitize prompt BEFORE acquiring config lock to avoid
    // the non-Send MutexGuard crossing any await boundary
    let safe_prompt = sanitize_prompt_async(&state.privacy, &id, &req.prompt).await;
    debug!(session_id = %id, request_id = %request_id, "prompt sanitized, acquiring config lock");
    
    // D5: Build provider FIRST, before setting Running status
    // Resolve model using hierarchy: req.model_id > agent_config.model > session.model_id > config.model
    let config = state.config.lock().map_err(|e| {
        error!(session_id = %id, request_id = %request_id, error = %e, "failed to acquire config lock in submit_prompt");
        ServerError::internal(e.to_string())
    })?;
    
    // Extract all needed values from config as owned data before releasing the lock.
    // This is required because agent_config borrows from config, and we must drop
    // the lock BEFORE re-acquiring it below (Mutex is NOT re-entrant — double-locking
    // on the same thread is a guaranteed deadlock).
    let model_id = req.model_id
        .clone()
        .or_else(|| config.model_for_agent(agent_name).map(|s| s.to_string()))
        .unwrap_or_else(|| session.model_id.clone());
    debug!(session_id = %id, request_id = %request_id, model_id = %model_id, "resolved model_id, about to check fast-path");

    let max_tokens_override: Option<u32> = config.agent.as_ref()
        .and_then(|agents| agents.get(agent_name))
        .and_then(|ac| ac.max_tokens);
    let reasoning_effort_override: Option<String> = config.agent.as_ref()
        .and_then(|agents| agents.get(agent_name))
        .and_then(|ac| ac.reasoning_effort.clone());
    let allowed_tools = config.tools_for_agent(agent_name);
    let auto_compact = config.auto_compact;
    let compact_threshold_messages = config.compact_threshold_messages;
    let compact_keep_messages = config.compact_keep_messages;

    // Release the config lock — must happen before we reach the re-acquire below.
    drop(config);

    if let Some(command) = parse_fast_path_shell_command(&req.prompt, allowed_tools.as_deref()) {
        // agent_config borrow is no longer live (config was dropped above)
        let _ = agent_name;

        let was_set = state.session_service.update_status(&id, SessionStatus::Running);
        if !was_set {
            warn!(session_id = %id, "failed to transition session to running for fast-path command");
            return Err(ServerError::conflict("session already running for fast-path command"));
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
            content: safe_prompt.clone(),
        }]);
        state.session_service.add_message(&id, message.clone());

        let title_gen_provider = if pre_existing_count == 0 {
            // Compute title_model_str AFTER the await using a fresh config lock
            let config_guard = state.config.lock().ok();
            let title_model_str = config_guard
                .as_ref()
                .and_then(|g| g.effective_small_model().map(|s| s.to_string()))
                .or_else(|| config_guard.as_ref().and_then(|g| g.model_for_agent("title").map(|s| s.to_string())))
                .or_else(|| config_guard.as_ref().and_then(|g| g.effective_model()));
            if let Some(ref model_str) = title_model_str {
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
            let prompt_content = safe_prompt.clone();

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
        let privacy = state.privacy.clone();

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
                // add_message already publishes MessageAdded
                session_service.add_message(&session_id_clone, tool_call_message.clone());

                let tool_result = tools
                    .execute("bash", serde_json::json!({ "command": command }), &tool_ctx)
                    .await;

                let (content, is_error) = match tool_result {
                    Ok(result) => (result.content, false),
                    Err(error) => (format!("Error: {}", error), true),
                };

                // Privacy: sanitize tool result content before persisting
                // (fast-path bypasses AgentExecutor, so Hook 3 is not called automatically)
                let safe_content = privacy.sanitize_response(&session_id_clone, &content).await;

                let result_message = Message::assistant(session_id_clone.clone(), vec![Part::ToolResult {
                    tool_call_id,
                    content: safe_content,
                    is_error,
                }]);
                // add_message already publishes MessageAdded
                session_service.add_message(&session_id_clone, result_message.clone());

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

    // Build provider — config lock was already released above (before fast-path check).
    // Re-acquire it here for both the normal path and any re-entry after fast-path returns.
    debug!(session_id = %id, request_id = %request_id, model_id = %model_id, "acquiring config lock to build provider");
    let config = state.config.lock().map_err(|e| {
        error!(session_id = %id, request_id = %request_id, error = %e, "failed to acquire config lock before provider build");
        ServerError::internal(e.to_string())
    })?;
    debug!(session_id = %id, request_id = %request_id, model_id = %model_id, "building provider");
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

    // Resolve title model: compute owned String from config (after config is dropped)
    // This avoids the non-Send MutexGuard from crossing the async boundary
    let title_model_str = {
        let config_guard = state.config.lock().ok();
        config_guard
            .as_ref()
            .and_then(|g| g.effective_small_model().map(|s| s.to_string()))
            .or_else(|| config_guard.as_ref().and_then(|g| g.model_for_agent("title").map(|s| s.to_string())))
            .or_else(|| config_guard.as_ref().and_then(|g| g.effective_model()))
    };
    
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

    // safe_prompt was already computed before config was locked
    let message = Message::user(id.clone(), vec![Part::Text {
        content: safe_prompt.clone(),
    }]);
    state.session_service.add_message(&id, message.clone());
    
    // T3: If this is the first message in the session, spawn async title generation
    // We build the provider BEFORE spawning to avoid holding MutexGuard across await
    let title_gen_provider = if pre_existing_count == 0 {
        if let Some(ref model_str) = title_model_str {
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
        let prompt_content = safe_prompt.clone();
        
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
    .with_event_bus(Arc::clone(&state.event_bus))
    .with_privacy_service(state.privacy.clone());

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
                    // add_message already publishes MessageAdded — no manual publish needed
                    session_service.add_message(&session_id_clone, agent_result.message.clone());
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
                
                // T-01: Emit AgentError event on executor failure
                let error_msg = e.to_string();
                event_bus.publish(rcode_event::Event::AgentError {
                    session_id: session_id_clone.clone(),
                    agent_id: agent_name_clone.clone(),
                    error: error_msg.clone(),
                });
                
                // Persist error message to session
                let error_message = Message::assistant(
                    session_id_clone.clone(),
                    vec![Part::Text { content: format!("⚠ Agent error: {}", error_msg) }],
                );
                let msg_id = error_message.id.0.clone();
                session_service.add_message(&session_id_clone, error_message);
                event_bus.publish(rcode_event::Event::MessageAdded {
                    session_id: session_id_clone.clone(),
                    message_id: msg_id,
                });
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
    /// Wire protocol: "openai_compat" | "anthropic_compat" | "google"
    pub protocol: String,
    /// True for non-native providers (anything other than openai, anthropic, google)
    pub is_compatible: bool,
}

/// Convert ProviderProtocol to a serializable string
fn protocol_to_string(protocol: rcode_core::ProviderProtocol) -> &'static str {
    match protocol {
        rcode_core::ProviderProtocol::OpenAiCompat => "openai_compat",
        rcode_core::ProviderProtocol::AnthropicCompat => "anthropic_compat",
        rcode_core::ProviderProtocol::Google => "google",
    }
}

/// GET /models - List all available models
pub async fn list_models(
    State(state): State<Arc<AppState>>,
) -> axum::Json<ListModelsResponse> {
    let config = (*state.config.lock().unwrap()).clone();
    let models = state.catalog.list_models(&config).await;
    
    // Native (non-compatible) providers: these use their own native protocol
    const NATIVE_PROVIDERS: &[&str] = &["openai", "anthropic", "google"];
    
    let dto_models: Vec<CatalogModelDto> = models.into_iter().map(|m| {
        // Look up protocol from registry
        let protocol_str = rcode_providers::lookup_provider(&m.provider)
            .map(|def| protocol_to_string(def.protocol))
            .unwrap_or("openai_compat");
        
        // is_compatible is false for native providers, true for everything else
        let is_compatible = !NATIVE_PROVIDERS.contains(&m.provider.as_str());
        
        CatalogModelDto {
            id: m.id,
            provider: m.provider,
            display_name: m.display_name,
            has_credentials: m.has_credentials,
            source: format!("{:?}", m.source).to_lowercase(),
            enabled: m.enabled,
            protocol: protocol_str.to_string(),
            is_compatible,
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

/// GET /config/providers - Returns provider status with registry-enriched data
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

    // Compute whether a provider is enabled based on disabled_providers config
    let is_provider_enabled = |provider_id: &str| -> bool {
        config
            .disabled_providers
            .as_ref()
            .map(|disabled| !disabled.contains(&provider_id.to_string()))
            .unwrap_or(true)
    };

    // Native (non-compatible) providers - these use their own native protocol
    const NATIVE_PROVIDERS: &[&str] = &["openai", "anthropic", "google"];

    // Build ordered list: registry providers first, then any additional ones
    // that exist in the config (e.g. custom or third-party providers).
    let mut seen = std::collections::HashSet::new();
    let mut list: Vec<serde_json::Value> = Vec::new();

    // First add all built-in providers from the registry (maintains order: anthropic, openai, google, openrouter, minimax, zai, github-copilot)
    for def in rcode_providers::registry().values() {
        seen.insert(def.id);
        let is_native = NATIVE_PROVIDERS.contains(&def.id);
        let protocol_str = protocol_to_string(def.protocol);
        // supports_custom_base_url: true for openai_compat and anthropic_compat protocols
        let supports_custom_base_url = matches!(
            def.protocol,
            rcode_core::ProviderProtocol::OpenAiCompat | rcode_core::ProviderProtocol::AnthropicCompat
        );

        list.push(serde_json::json!({
            "id":                           def.id,
            "name":                         def.display_name,
            "display_name":                 def.display_name,
            "protocol":                     protocol_str,
            "native":                       is_native,
            "supports_custom_base_url":     supports_custom_base_url,
            "has_key":                      check_has_key(def.id),
            "key_source":                   get_key_source(def.id),
            "base_url":                     get_base_url(def.id),
            "enabled":                      is_provider_enabled(def.id),
            "models_count":                 def.fallback_models.len(),
        }));
    }

    // Add providers from config that are not in the registry (custom providers)
    for (id, _provider_cfg) in config.providers.iter() {
        if seen.contains(id.as_str()) {
            continue;
        }
        // Unknown provider: assume openai_compat, non-native, supports custom base_url
        list.push(serde_json::json!({
            "id":                           id,
            "name":                         id,
            "display_name":                 id,
            "protocol":                     "openai_compat",
            "native":                       false,
            "supports_custom_base_url":     true,
            "has_key":                      check_has_key(id),
            "key_source":                   get_key_source(id),
            "base_url":                     get_base_url(id),
            "enabled":                      is_provider_enabled(id),
            "models_count":                 0,
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

// ========== Explorer Endpoints ==========

/// Query parameters for explorer endpoints
#[derive(Debug, Deserialize)]
pub struct ExplorerQuery {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Query parameters for tree/children endpoint
#[derive(Debug, Deserialize)]
pub struct TreeQuery {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    /// Path to get children for (absolute or relative to workspace root)
    #[serde(default)]
    pub path: String,
    /// Depth to load (default 1 for immediate children only)
    #[serde(default = "default_depth")]
    pub depth: usize,
    /// Filter mode: all, changed, staged, untracked, conflicted
    #[serde(default = "default_filter")]
    pub filter: String,
    /// Include ignored files (default false)
    #[serde(default)]
    pub include_ignored: bool,
    /// Include outside repo files (default false)
    #[serde(default)]
    pub include_outside_repo: bool,
}

fn default_depth() -> usize { 1 }
fn default_filter() -> String { "all".to_string() }

/// GET /explorer/bootstrap?session_id=<id>
/// Returns workspace metadata for the explorer
pub async fn explorer_bootstrap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExplorerQuery>,
) -> Result<Json<ExplorerBootstrap>, ServerError> {
    let workspace_root = resolve_workspace_root(
        state.as_ref(),
        query.project_id.as_deref(),
        query.session_id.as_deref(),
    )
    .await?;
    let bootstrap = state.explorer_service.get_bootstrap(&workspace_root)
        .await
        .map_err(|e| ServerError::internal(e.to_string()))?;
    
    Ok(Json(bootstrap))
}

/// GET /explorer/tree?session_id=<id>&path=<path>&depth=1&filter=all
/// Returns children for a directory path (lazy loading)
pub async fn explorer_tree(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<TreeResponse>, ServerError> {
    let workspace_root = resolve_workspace_root(
        state.as_ref(),
        query.project_id.as_deref(),
        query.session_id.as_deref(),
    )
    .await?;
    let path = if query.path.is_empty() || query.path == "." {
        PathBuf::from(".")
    } else {
        PathBuf::from(&query.path)
    };
    
    let response = state.explorer_service.get_children(
        &workspace_root, 
        &path, 
        query.depth,
        TreeFilter::parse(&query.filter),
        query.include_ignored,
        query.include_outside_repo,
    )
        .await
        .map_err(|e| ServerError::internal(e.to_string()))?;
    
    Ok(Json(response))
}

// ========== Outline Endpoint ==========

/// Timeout for LSP document_symbols requests in seconds
const OUTLINE_TIMEOUT_SECS: u64 = 5;

/// Query parameters for outline endpoint
#[derive(Debug, Deserialize)]
pub struct OutlineQuery {
    pub session_id: String,
    pub path: String,
}

/// Capabilities supported by the outline endpoint
#[derive(Debug, Clone, Serialize)]
pub struct OutlineCapabilities {
    pub document_symbols: bool,
    pub hierarchical: bool,
}

/// A symbol in the outline tree with frontend-compatible types
/// This DTO converts LSP types to the wire format expected by the frontend:
/// - `kind` is converted from LSP integer to string name
/// - `selection_range` uses snake_case (not camelCase)
#[derive(Debug, Clone, Serialize)]
pub struct OutlineSymbolDto {
    pub name: String,
    #[serde(rename = "kind")]
    pub kind_string: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub range: rcode_lsp::Range,
    #[serde(rename = "selectionRange")]
    pub selection_range: rcode_lsp::Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<OutlineSymbolDto>>,
}

/// Response for outline endpoint
#[derive(Debug, Serialize)]
pub struct OutlineResponse {
    pub path: String,
    pub absolute_path: String,
    pub language: String,
    pub source: OutlineSource,
    pub capabilities: OutlineCapabilities,
    pub symbols: Vec<OutlineSymbolDto>,
    // T4.5: Session metadata
    pub message_count: usize,
    pub token_estimate: Option<usize>,
}

/// Source of outline data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutlineSource {
    Lsp,
    Unavailable,
}

/// Convert LSP DocumentSymbol to frontend-compatible OutlineSymbolDto
/// This handles:
/// - Converting SymbolKind integer to string name (e.g., 12 -> "Function")
/// - Recursively converting children
fn document_symbol_to_dto(symbol: rcode_lsp::DocumentSymbol) -> OutlineSymbolDto {
    fn kind_to_string(kind: rcode_lsp::SymbolKind) -> &'static str {
        match kind {
            rcode_lsp::SymbolKind::File => "File",
            rcode_lsp::SymbolKind::Module => "Module",
            rcode_lsp::SymbolKind::Namespace => "Namespace",
            rcode_lsp::SymbolKind::Package => "Package",
            rcode_lsp::SymbolKind::Class => "Class",
            rcode_lsp::SymbolKind::Method => "Method",
            rcode_lsp::SymbolKind::Property => "Property",
            rcode_lsp::SymbolKind::Field => "Field",
            rcode_lsp::SymbolKind::Constructor => "Constructor",
            rcode_lsp::SymbolKind::Enum => "Enum",
            rcode_lsp::SymbolKind::Interface => "Interface",
            rcode_lsp::SymbolKind::Function => "Function",
            rcode_lsp::SymbolKind::Variable => "Variable",
            rcode_lsp::SymbolKind::Constant => "Constant",
            rcode_lsp::SymbolKind::String => "String",
            rcode_lsp::SymbolKind::Number => "Number",
            rcode_lsp::SymbolKind::Boolean => "Boolean",
            rcode_lsp::SymbolKind::Array => "Array",
            rcode_lsp::SymbolKind::Object => "Object",
            rcode_lsp::SymbolKind::Key => "Key",
            rcode_lsp::SymbolKind::Null => "Null",
            rcode_lsp::SymbolKind::EnumMember => "EnumMember",
            rcode_lsp::SymbolKind::Struct => "Struct",
            rcode_lsp::SymbolKind::Event => "Event",
            rcode_lsp::SymbolKind::Operator => "Operator",
            rcode_lsp::SymbolKind::TypeParameter => "TypeParameter",
        }
    }

    fn convert(symbol: rcode_lsp::DocumentSymbol) -> OutlineSymbolDto {
        OutlineSymbolDto {
            name: symbol.name,
            kind_string: kind_to_string(symbol.kind).to_string(),
            detail: symbol.detail,
            range: symbol.range,
            selection_range: symbol.selection_range,
            children: symbol.children.map(|children| {
                children.into_iter().map(convert).collect()
            }),
        }
    }

    convert(symbol)
}

/// Build unavailable response with capabilities
fn build_unavailable_response(
    path: String, 
    absolute_path: String, 
    language: String,
    message_count: usize,
    token_estimate: Option<usize>,
) -> Json<OutlineResponse> {
    Json(OutlineResponse {
        path,
        absolute_path,
        language,
        source: OutlineSource::Unavailable,
        capabilities: OutlineCapabilities {
            document_symbols: false,
            hierarchical: false,
        },
        symbols: vec![],
        message_count,
        token_estimate,
    })
}

/// GET /outline?session_id=<id>&path=<workspace-relative-path>
/// Returns document symbols (outline) for a file using LSP
pub async fn get_outline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OutlineQuery>,
) -> Result<Json<OutlineResponse>, ServerError> {
    // Validate session_id is present (already required by Query deserialization)
    // Validate path is present (already required by Query deserialization)
    
    // Get session and verify it exists
    let session = state.session_service.get(&SessionId(query.session_id.clone()))
        .ok_or_else(|| ServerError::not_found())?;
    
    let project_path = session.project_path.clone();
    
    // T4.5: Get message count and estimate tokens for session metadata
    let messages = state.session_service.get_messages(&query.session_id);
    let message_count = messages.len();
    // Rough token estimate: average 4 chars per token
    let token_estimate = messages.iter()
        .map(|m| {
            let rcode_core::Message { parts, .. } = m;
            parts.iter().map(|p| match p {
                rcode_core::Part::Text { content } => content.len(),
                rcode_core::Part::Reasoning { content } => content.len(),
                rcode_core::Part::ToolCall { name, arguments, .. } => {
                    name.len() + arguments.to_string().len()
                }
                rcode_core::Part::ToolResult { content, .. } => content.len(),
                rcode_core::Part::Attachment { .. } => 0,
                rcode_core::Part::TaskChecklist { items } => items
                    .iter()
                    .map(|item| item.content.len() + item.status.len() + item.priority.len())
                    .sum::<usize>(),
            }).sum::<usize>()
        })
        .sum::<usize>() / 4; // Rough: 4 chars per token
    
    // Resolve the path - it should be relative to project root
    let requested_path = std::path::Path::new(&query.path);
    
    // Security check: ensure path doesn't traverse outside project_path
    let absolute_requested = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        project_path.join(requested_path)
    };
    
    // Normalize and check for path traversal
    let canonical = absolute_requested.canonicalize()
        .map_err(|e| ServerError::bad_request(format!("Invalid path: {}", e)))?;
    let canonical_project = project_path.canonicalize()
        .map_err(|e| ServerError::internal(format!("Invalid project path: {}", e)))?;
    
    // Check that the canonical path is within the project
    if !canonical.starts_with(&canonical_project) {
        return Err(ServerError::forbidden("Path outside project directory"));
    }
    
    // Check that path is a file, not a directory
    if canonical.is_dir() {
        return Err(ServerError::bad_request("Path is a directory, expected a file"));
    }
    
    // Detect language from file extension
    let language = LanguageServerRegistry::detect_language(&canonical)
        .unwrap_or_else(|| "unknown".to_string());
    
    // Get or start LSP server for this language
    let lsp_client = state.lsp_registry
        .get_or_start_for_language(&language, &project_path)
        .await;
    
    // Convert path to file URI
    let file_uri = format!("file://{}", canonical.display());
    
    let symbols = if let Some(client) = lsp_client {
        // CRITICAL 1: Read file content and call did_open BEFORE document_symbols
        // Many LSP servers require the document to be registered before they return symbols
        let file_content = tokio::fs::read_to_string(&canonical)
            .await
            .map_err(|e| ServerError::internal(format!("Failed to read file: {}", e)))?;
        
        // Call did_open to notify the LSP server about the opened file
        // Version is 1 for new files (LSP spec: version should increment on each change)
        if let Err(e) = client.did_open(&file_uri, &language, 1, &file_content).await {
            tracing::warn!(uri = %file_uri, error = %e, "Failed to send did_open notification");
            // Continue anyway - some servers handle this gracefully
        }
        
        // Apply timeout to LSP request
        let symbols_result = tokio::time::timeout(
            std::time::Duration::from_secs(OUTLINE_TIMEOUT_SECS),
            client.document_symbols(&file_uri)
        ).await;
        
        match symbols_result {
            Ok(Ok(symbols)) => symbols,
            Ok(Err(_)) | Err(_) => {
                // LSP error or timeout - return unavailable with empty symbols
                return Ok(build_unavailable_response(
                    query.path,
                    canonical.to_string_lossy().to_string(),
                    language,
                    message_count,
                    Some(token_estimate),
                ));
            }
        }
    } else {
        // No LSP server available for this language
        return Ok(build_unavailable_response(
            query.path,
            canonical.to_string_lossy().to_string(),
            language,
            message_count,
            Some(token_estimate),
        ));
    };
    
    // Convert symbols to frontend-compatible DTO format
    let symbol_dtos: Vec<OutlineSymbolDto> = symbols
        .into_iter()
        .map(document_symbol_to_dto)
        .collect();
    
    Ok(Json(OutlineResponse {
        path: query.path,
        absolute_path: canonical.to_string_lossy().to_string(),
        language,
        source: OutlineSource::Lsp,
        capabilities: OutlineCapabilities {
            document_symbols: true,
            hierarchical: true,
        },
        symbols: symbol_dtos,
        message_count,
        token_estimate: Some(token_estimate),
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        create_project, create_session, delete_project, document_symbol_to_dto, explorer_bootstrap,
        list_project_sessions, parse_fast_path_shell_command, rename_session, CreateProjectRequest,
        CreateSessionRequest, ExplorerQuery, RenameSessionRequest,
    };
    use crate::{cancellation::CancellationRegistry, explorer::ExplorerService, state::AppState};
    use axum::{extract::{Path as AxumPath, Query, State}, Json};
    use rcode_core::{ProjectId, RcodeConfig, Session};
    use rcode_event::EventBus;
    use rcode_lsp::{DocumentSymbol, Range, Position, SymbolKind};
    use rcode_lsp::LanguageServerRegistry;
    use rcode_providers::{catalog::ModelCatalogService, ProviderRegistry};
    use rcode_session::{ProjectService, SessionService};
    use rcode_storage::{schema, ProjectRepository};
    use rcode_tools::ToolRegistryService;
    use rusqlite::Connection;
    use std::{collections::HashMap, sync::{Arc, Mutex}};
    use tempfile::tempdir;
    use tokio::sync::Mutex as TokioMutex;

    fn create_test_state() -> (Arc<AppState>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("projects.db");
        let conn = Connection::open(&db_path).unwrap();
        schema::init_schema(&conn).unwrap();

        let event_bus = Arc::new(EventBus::new(32));
        let session_service = Arc::new(SessionService::new(event_bus.clone()));
        let tools = Arc::new(ToolRegistryService::with_session_service(session_service.clone()));
        let project_service = Arc::new(ProjectService::new(Arc::new(ProjectRepository::new(conn))));

        let state = Arc::new(AppState {
            project_service: Some(project_service),
            session_service,
            event_bus,
            providers: Arc::new(Mutex::new(ProviderRegistry::new())),
            tools,
            config: Arc::new(Mutex::new(RcodeConfig::default())),
            catalog: Arc::new(ModelCatalogService::new()),
            cancellation: Arc::new(CancellationRegistry::new()),
            permission_services: Arc::new(TokioMutex::new(HashMap::new())),
            lsp_registry: Arc::new(LanguageServerRegistry::new()),
            mock_provider: Arc::new(Mutex::new(None)),
            explorer_service: Arc::new(ExplorerService::new()),
            privacy: rcode_privacy::service::PrivacyService::new(
                rcode_privacy::service::PrivacyConfig::default()
            ),
        });

        (state, dir)
    }

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

    // ========== Outline DTO Tests ==========

    #[test]
    fn test_document_symbol_to_dto_converts_kind_to_string() {
        let symbol = DocumentSymbol {
            name: "my_function".to_string(),
            kind: SymbolKind::Function,
            detail: Some("fn() -> ()".to_string()),
            range: Range::new(Position::new(0, 0), Position::new(10, 0)),
            selection_range: Range::new(Position::new(0, 0), Position::new(10, 0)),
            children: None,
            tags: None,
            deprecated: None,
        };

        let dto = document_symbol_to_dto(symbol);

        assert_eq!(dto.name, "my_function");
        assert_eq!(dto.kind_string, "Function");
        assert_eq!(dto.detail, Some("fn() -> ()".to_string()));
    }

    #[test]
    fn test_document_symbol_to_dto_converts_all_symbol_kinds() {
        // Test that all major SymbolKind values convert to the correct string
        let cases = vec![
            (SymbolKind::File, "File"),
            (SymbolKind::Module, "Module"),
            (SymbolKind::Namespace, "Namespace"),
            (SymbolKind::Package, "Package"),
            (SymbolKind::Class, "Class"),
            (SymbolKind::Method, "Method"),
            (SymbolKind::Property, "Property"),
            (SymbolKind::Field, "Field"),
            (SymbolKind::Constructor, "Constructor"),
            (SymbolKind::Enum, "Enum"),
            (SymbolKind::Interface, "Interface"),
            (SymbolKind::Function, "Function"),
            (SymbolKind::Variable, "Variable"),
            (SymbolKind::Constant, "Constant"),
            (SymbolKind::Struct, "Struct"),
            (SymbolKind::EnumMember, "EnumMember"),
            (SymbolKind::TypeParameter, "TypeParameter"),
        ];

        for (kind, expected_str) in cases {
            let symbol = DocumentSymbol {
                name: "test".to_string(),
                kind,
                detail: None,
                range: Range::new(Position::new(0, 0), Position::new(1, 0)),
                selection_range: Range::new(Position::new(0, 0), Position::new(1, 0)),
                children: None,
                tags: None,
                deprecated: None,
            };
            let dto = document_symbol_to_dto(symbol);
            assert_eq!(dto.kind_string, expected_str, "Kind {:?} should convert to {}", kind, expected_str);
        }
    }

    #[test]
    fn test_document_symbol_to_dto_preserves_range() {
        let symbol = DocumentSymbol {
            name: "test".to_string(),
            kind: SymbolKind::Function,
            detail: None,
            range: Range::new(Position::new(5, 10), Position::new(15, 20)),
            selection_range: Range::new(Position::new(5, 10), Position::new(5, 14)),
            children: None,
            tags: None,
            deprecated: None,
        };

        let dto = document_symbol_to_dto(symbol);

        assert_eq!(dto.range.start.line, 5);
        assert_eq!(dto.range.start.character, 10);
        assert_eq!(dto.range.end.line, 15);
        assert_eq!(dto.range.end.character, 20);
        assert_eq!(dto.selection_range.start.line, 5);
        assert_eq!(dto.selection_range.start.character, 10);
        assert_eq!(dto.selection_range.end.line, 5);
        assert_eq!(dto.selection_range.end.character, 14);
    }

    #[test]
    fn test_document_symbol_to_dto_with_children() {
        let child = DocumentSymbol {
            name: "child_method".to_string(),
            kind: SymbolKind::Method,
            detail: None,
            range: Range::new(Position::new(2, 0), Position::new(5, 0)),
            selection_range: Range::new(Position::new(2, 0), Position::new(2, 12)),
            children: None,
            tags: None,
            deprecated: None,
        };

        let parent = DocumentSymbol {
            name: "MyClass".to_string(),
            kind: SymbolKind::Class,
            detail: Some("struct MyClass".to_string()),
            range: Range::new(Position::new(0, 0), Position::new(10, 0)),
            selection_range: Range::new(Position::new(0, 0), Position::new(0, 7)),
            children: Some(vec![child]),
            tags: None,
            deprecated: None,
        };

        let dto = document_symbol_to_dto(parent);

        assert_eq!(dto.name, "MyClass");
        assert_eq!(dto.kind_string, "Class");
        assert!(dto.children.is_some());
        let children = dto.children.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "child_method");
        assert_eq!(children[0].kind_string, "Method");
    }

    #[test]
    fn test_document_symbol_to_dto_serialization_format() {
        // Verify that the DTO serializes with the correct field names for the frontend
        let symbol = DocumentSymbol {
            name: "test_func".to_string(),
            kind: SymbolKind::Function,
            detail: None,
            range: Range::new(Position::new(0, 0), Position::new(5, 0)),
            selection_range: Range::new(Position::new(0, 0), Position::new(0, 9)),
            children: None,
            tags: None,
            deprecated: None,
        };

        let dto = document_symbol_to_dto(symbol);
        let json = serde_json::to_string(&dto).unwrap();

        // Should use "kind" as the field name (via #[serde(rename = "kind")])
        // Should use "selectionRange" for selection_range (via #[serde(rename = "selectionRange")])
        assert!(json.contains(r#""name":"test_func""#));
        assert!(json.contains(r#""kind":"Function""#));
        assert!(json.contains(r#""selectionRange""#));
        // Should NOT contain snake_case "selection_range"
        assert!(!json.contains(r#""selection_range""#));
    }

    #[test]
    fn test_outline_timeout_constant_is_5_seconds() {
        // Verify the timeout constant is correctly set to 5 seconds
        // This proves the timeout is configured as specified in the requirements
        assert_eq!(super::OUTLINE_TIMEOUT_SECS, 5);
        // Also verify it's used correctly in Duration creation
        let duration = std::time::Duration::from_secs(super::OUTLINE_TIMEOUT_SECS);
        assert_eq!(duration.as_secs(), 5);
    }

    #[test]
    fn test_outline_source_lsp_serialization() {
        // Verify OutlineSource::Lsp serializes correctly
        use super::OutlineSource;
        let source = OutlineSource::Lsp;
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, "\"lsp\"");
    }

    #[test]
    fn test_outline_source_unavailable_serialization() {
        // Verify OutlineSource::Unavailable serializes correctly
        use super::OutlineSource;
        let source = OutlineSource::Unavailable;
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, "\"unavailable\"");
    }

    #[test]
    fn test_outline_response_with_lsp_source() {
        // Verify OutlineResponse with OutlineSource::Lsp serializes with correct source field
        use super::{OutlineResponse, OutlineSource, OutlineCapabilities, OutlineSymbolDto};
        
        let response = OutlineResponse {
            path: "main.rs".to_string(),
            absolute_path: "/project/main.rs".to_string(),
            language: "rust".to_string(),
            source: OutlineSource::Lsp,
            capabilities: OutlineCapabilities {
                document_symbols: true,
                hierarchical: true,
            },
            symbols: vec![OutlineSymbolDto {
                name: "main".to_string(),
                kind_string: "Function".to_string(),
                detail: None,
                range: Range::new(Position::new(0, 0), Position::new(5, 0)),
                selection_range: Range::new(Position::new(0, 0), Position::new(0, 4)),
                children: None,
            }],
            message_count: 5,
            token_estimate: Some(100),
        };
        
        let json = serde_json::to_string(&response).unwrap();
        // Verify source is "lsp" not "Lsp" (lowercase due to #[serde(rename_all = "lowercase")])
        assert!(json.contains(r#""source":"lsp""#));
        assert!(json.contains(r#""language":"rust""#));
        assert!(json.contains(r#""name":"main""#));
        assert!(json.contains(r#""kind":"Function""#));
    }

    #[tokio::test]
    async fn create_session_with_project_id_uses_project_root() {
        let (state, dir) = create_test_state();
        let project_root = dir.path().join("project-root");
        std::fs::create_dir_all(&project_root).unwrap();

        let Json(project) = create_project(
            State(state.clone()),
            Json(CreateProjectRequest {
                path: project_root.to_string_lossy().to_string(),
                name: Some("demo".to_string()),
            }),
        )
        .await
        .unwrap();

        let Json(session) = create_session(
            State(state.clone()),
            Json(CreateSessionRequest {
                project_id: Some(project.id.clone()),
                project_path: None,
                agent_id: Some("build".to_string()),
                model_id: Some("model-x".to_string()),
                parent_id: None,
            }),
        )
        .await
        .unwrap();

        assert_eq!(session.project_id, Some(ProjectId(project.id)));
        assert_eq!(session.project_path, project_root.canonicalize().unwrap());
    }

    #[tokio::test]
    async fn create_session_legacy_project_path_still_works() {
        let (state, dir) = create_test_state();
        let project_root = dir.path().join("legacy-root");
        std::fs::create_dir_all(&project_root).unwrap();

        let Json(session) = create_session(
            State(state),
            Json(CreateSessionRequest {
                project_id: None,
                project_path: Some(project_root.to_string_lossy().to_string()),
                agent_id: Some("build".to_string()),
                model_id: Some("model-x".to_string()),
                parent_id: None,
            }),
        )
        .await
        .unwrap();

        assert_eq!(session.project_path, project_root);
    }

    #[tokio::test]
    async fn list_project_sessions_returns_only_matching_project() {
        let (state, dir) = create_test_state();
        let project_root = dir.path().join("project-a");
        std::fs::create_dir_all(&project_root).unwrap();
        let other_root = dir.path().join("project-b");
        std::fs::create_dir_all(&other_root).unwrap();

        let Json(project) = create_project(
            State(state.clone()),
            Json(CreateProjectRequest {
                path: project_root.to_string_lossy().to_string(),
                name: Some("project-a".to_string()),
            }),
        )
        .await
        .unwrap();
        let project_id = project.id.clone();

        let mut session = Session::new(project_root.canonicalize().unwrap(), "build".into(), "model".into());
        session.project_id = Some(ProjectId(project.id.clone()));
        state.session_service.create(session);
        state.session_service.create(Session::new(other_root, "build".into(), "model".into()));

        let Json(sessions) = list_project_sessions(State(state), AxumPath(project_id.clone()))
            .await
            .unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project_id.as_ref().map(|id| id.0.as_str()), Some(project_id.as_str()));
    }

    #[tokio::test]
    async fn explorer_bootstrap_prefers_project_id_over_session_id() {
        let (state, dir) = create_test_state();
        let project_root = dir.path().join("project-root");
        std::fs::create_dir_all(project_root.join("src")).unwrap();
        let session_root = dir.path().join("session-root");
        std::fs::create_dir_all(&session_root).unwrap();

        let Json(project) = create_project(
            State(state.clone()),
            Json(CreateProjectRequest {
                path: project_root.to_string_lossy().to_string(),
                name: Some("project-root".to_string()),
            }),
        )
        .await
        .unwrap();

        let session = state.session_service.create(Session::new(
            session_root,
            "build".into(),
            "model".into(),
        ));

        let Json(bootstrap) = explorer_bootstrap(
            State(state),
            Query(ExplorerQuery {
                project_id: Some(project.id),
                session_id: Some(session.id.0.clone()),
            }),
        )
        .await
        .unwrap();

        assert_eq!(bootstrap.workspace_root, project_root.canonicalize().unwrap().to_string_lossy());
    }

    #[tokio::test]
    async fn delete_project_removes_existing_project() {
        let (state, dir) = create_test_state();
        let project_root = dir.path().join("delete-project");
        std::fs::create_dir_all(&project_root).unwrap();

        let Json(project) = create_project(
            State(state.clone()),
            Json(CreateProjectRequest {
                path: project_root.to_string_lossy().to_string(),
                name: Some("delete-project".to_string()),
            }),
        )
        .await
        .unwrap();

        let Json(response) = delete_project(State(state.clone()), AxumPath(project.id.clone()))
            .await
            .unwrap();

        assert_eq!(response.get("ok").and_then(|value| value.as_bool()), Some(true));
        assert!(state
            .project_service
            .as_ref()
            .unwrap()
            .get(&ProjectId(project.id))
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn rename_session_updates_title() {
        let (state, _dir) = create_test_state();
        let project_root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_root.path()).unwrap();

        let session = state.session_service.create(Session::new(
            project_root.path().to_path_buf(),
            "build".into(),
            "model".into(),
        ));
        let session_id = session.id.0.clone();

        // Verify initial title is None
        let session_before = state.session_service.get(&rcode_core::SessionId(session_id.clone())).unwrap();
        assert!(session_before.title.is_none(), "Initial title should be None");

        // Rename the session
        let Json(updated) = rename_session(
            State(state.clone()),
            AxumPath(session_id.clone()),
            Json(RenameSessionRequest {
                title: "My Custom Title".to_string(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(updated.title, Some("My Custom Title".to_string()));

        // Verify persisted
        let session_after = state.session_service.get(&rcode_core::SessionId(session_id.clone())).unwrap();
        assert_eq!(session_after.title, Some("My Custom Title".to_string()));
    }

    #[tokio::test]
    async fn rename_session_overwrites_existing_title() {
        let (state, _dir) = create_test_state();
        let project_root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_root.path()).unwrap();

        let session = state.session_service.create(Session::new(
            project_root.path().to_path_buf(),
            "build".into(),
            "model".into(),
        ));
        let session_id = session.id.0.clone();

        // First rename
        let Json(first) = rename_session(
            State(state.clone()),
            AxumPath(session_id.clone()),
            Json(RenameSessionRequest {
                title: "First Title".to_string(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(first.title, Some("First Title".to_string()));

        // Second rename (overwrite)
        let Json(second) = rename_session(
            State(state.clone()),
            AxumPath(session_id.clone()),
            Json(RenameSessionRequest {
                title: "Second Title".to_string(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(second.title, Some("Second Title".to_string()));
    }

    #[tokio::test]
    async fn rename_session_returns_404_for_nonexistent() {
        let (state, _dir) = create_test_state();

        let result = rename_session(
            State(state.clone()),
            AxumPath("nonexistent-id".to_string()),
            Json(RenameSessionRequest {
                title: "Test".to_string(),
            }),
        )
        .await;

        assert!(result.is_err(), "Should return error for nonexistent session");
    }
}
