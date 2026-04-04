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

use crate::state::AppState;
use crate::error::ServerError;
use rcode_core::{
    Session, SessionId, SessionStatus, Message, Part, MessageId, 
    PaginationParams, PaginatedMessages, AgentContext, save_config, ProviderConfig,
};
use rcode_agent::{AgentExecutor, DefaultAgent};
use rcode_providers::{ProviderFactory, ModelInfo};

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
    let model = state.config.lock().unwrap().effective_model().unwrap_or("claude-sonnet-4-5").to_string();
    let session = Session::new(
        req.project_path.into(),
        req.agent_id.unwrap_or_else(|| "build".to_string()),
        req.model_id.unwrap_or_else(|| model),
    );
    let session = state.session_service.create(session);
    Ok(Json(session.as_ref().clone()))
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub project_path: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Session>, ServerError> {
    let session = state.session_service.get(&SessionId(id))
        .ok_or_else(|| ServerError::not_found())?;
    Ok(Json(session.as_ref().clone()))
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
    // D5: Check session exists first
    let session = state.session_service.get(&SessionId(id.clone()))
        .ok_or_else(|| ServerError::not_found())?;
    
    // D5: Build provider FIRST, before setting Running status
    let model_id = session.model_id.clone();
    let config = state.config.lock().map_err(|e| ServerError::internal(e.to_string()))?;
    let (provider, effective_model) = match ProviderFactory::build(&model_id, Some(&*config)) {
        Ok((p, m)) => (p, m),
        Err(e) => {
            return Err(ServerError::internal(e.to_string()));
        }
    };
    drop(config); // Release lock before spawning
    
    // D5: Gate concurrent access - only allow if session is Idle, AFTER provider build succeeds
    let was_set = state.session_service.update_status(&id, SessionStatus::Running);
    if !was_set {
        return Err(ServerError::conflict("Session is already running or in an invalid state"));
    }
    
    // D5: Track pre-existing message count for deduplication (captured after provider built)
    let pre_existing_count = state.session_service.get_messages(&id).len();
    let message = Message::user(id.clone(), vec![Part::Text {
        content: req.prompt.clone(),
    }]);
    state.session_service.add_message(&id, message.clone());
    
    // C9: Get FULL session history (includes the new message we just added)
    let messages = state.session_service.get_messages(&id);
    
    // Create the agent
    let agent: Arc<dyn rcode_core::Agent> = Arc::new(DefaultAgent::new());
    
    // Build the executor
    let executor = AgentExecutor::new(
        agent,
        provider,
        Arc::clone(&state.tools),
    )
    .with_event_bus(Arc::clone(&state.event_bus));
    
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
    
    tokio::spawn(async move {
        let result = executor.run(&mut ctx).await;
        
        // D2: Persist only NEW assistant messages (not previously persisted)
        let new_messages = ctx.messages.iter().skip(pre_existing_count);
        for msg in new_messages {
            if msg.role == rcode_core::Role::Assistant {
                session_service.add_message(&id, msg.clone());
            }
        }
        
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
                if let Some(session) = session_service.get(&rcode_core::SessionId(id.clone())) {
                    let mut session_mut = (*session).clone();
                    session_mut.add_usage(prompt_toks, completion_toks, cost);
                    // Persist to storage
                    if let Some(repo) = session_service.session_repo() {
                        let _ = repo.update_usage(&id, session_mut.prompt_tokens, 
                            session_mut.completion_tokens, session_mut.total_cost_usd);
                    }
                }
            }
        }
        
        match result {
            Ok(_) => {
                // G5: Set to Idle so session can accept new prompts
                let _ = session_service.update_status(&id, SessionStatus::Idle);
            }
            Err(e) => {
                tracing::error!("Agent execution failed: {}", e);
                let _ = session_service.update_status(&id, SessionStatus::Aborted);
            }
        }
        
        // Publish agent finished event
        event_bus.publish(rcode_event::Event::AgentFinished {
            session_id: id,
        });
    });
    
    Ok(Json(PromptResponse {
        message_id: MessageId::new().0,
        status: "processing".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct PromptRequest {
    pub prompt: String,
}

#[derive(Debug, Serialize)]
pub struct PromptResponse {
    pub message_id: String,
    pub status: String,
}

pub async fn abort_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<()>, ServerError> {
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
    
    Ok(Sse::new(stream))
}

/// GET /models - List all available models
pub async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Json<ListModelsResponse> {
    let config = state.config.lock().unwrap();
    let models = ProviderFactory::list_models(&config);
    Json(ListModelsResponse { models })
}

#[derive(Debug, Serialize)]
pub struct ListModelsResponse {
    pub models: Vec<ModelInfo>,
}

/// POST /connect - Switch the active model for a session
pub async fn connect_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConnectRequest>,
) -> Result<Json<ConnectResponse>, ServerError> {
    // Validate the model_id exists in available models
    let config = state.config.lock().unwrap();
    let available_models = ProviderFactory::list_models(&config);
    let model_exists = available_models.iter().any(|m| m.id == req.model_id);
    
    if !model_exists {
        return Err(ServerError::bad_request(&format!(
            "Model '{}' is not available. Use GET /models to see available models.",
            req.model_id
        )));
    }
    
    // Verify session exists
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
    if let Some(model) = req.model {
        state.config.lock().unwrap().model = Some(model);
    }
    // Persist to disk
    let config = state.config.lock().unwrap();
    let _ = save_config(&config);
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /config/providers - Returns provider status
pub async fn get_providers(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let config = state.config.lock().unwrap();

    // Returns true if the provider has an API key set via env var OR in the
    // in-memory config (which may have been saved via PUT /config/providers/:id).
    let check_has_key = |provider_id: &str| -> bool {
        let env_key = format!("{}_API_KEY", provider_id.to_uppercase().replace('-', "_"));
        if std::env::var(&env_key).is_ok() {
            return true;
        }
        config
            .providers
            .get(provider_id)
            .and_then(|p| p.api_key.as_deref())
            .map(|k| !k.is_empty())
            .unwrap_or(false)
    };

    let get_base_url = |provider_id: &str| -> serde_json::Value {
        config
            .providers
            .get(provider_id)
            .and_then(|p| p.base_url.clone())
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null)
    };

    let known: &[(&str, &str)] = &[
        ("anthropic",  "Anthropic"),
        ("openai",     "OpenAI"),
        ("google",     "Google"),
        ("openrouter", "OpenRouter"),
        ("minimax",    "MiniMax"),
        ("zai",        "ZAI"),
    ];

    let list: Vec<serde_json::Value> = known
        .iter()
        .map(|(id, name)| serde_json::json!({
            "id":       id,
            "name":     name,
            "has_key":  check_has_key(id),
            "base_url": get_base_url(id),
            "enabled":  true,
        }))
        .collect();

    Json(serde_json::json!({ "providers": list }))
}

/// PUT /config/providers/:id - Set provider config
#[derive(Debug, Deserialize)]
pub struct UpdateProviderRequest {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

pub async fn update_provider(
    Path(provider_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let mut config = state.config.lock().unwrap();
    let provider_config = config
        .providers
        .entry(provider_id.clone())
        .or_insert_with(ProviderConfig::default);
    
    if let Some(api_key) = req.api_key {
        provider_config.api_key = Some(api_key);
    }
    if let Some(base_url) = req.base_url {
        provider_config.base_url = Some(base_url);
    }
    
    // Persist to disk
    let _ = save_config(&config);
    Ok(Json(serde_json::json!({ "ok": true })))
}
