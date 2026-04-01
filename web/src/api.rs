//! API Integration for OpenCode Backend

use crate::components::Session;
use crate::components::PaginatedMessages;
use serde::{Deserialize, Serialize};

pub const API_BASE: &str = "http://localhost:8080";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub project_path: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptResponse {
    pub message_id: String,
    pub status: String,
}

/// List all sessions
pub async fn list_sessions() -> Result<Vec<Session>, String> {
    let url = format!("{}/session", API_BASE);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }
    
    response
        .json::<Vec<Session>>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Create a new session
pub async fn create_session(project_path: &str) -> Result<Session, String> {
    let url = format!("{}/session", API_BASE);
    let body = CreateSessionRequest {
        project_path: project_path.to_string(),
        agent_id: Some("build".to_string()),
        model_id: Some("claude-sonnet-4-5".to_string()),
    };
    
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }
    
    response
        .json::<Session>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Get a single session by ID
pub async fn get_session(session_id: &str) -> Result<Session, String> {
    let url = format!("{}/session/{}", API_BASE, session_id);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }
    
    response
        .json::<Session>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Get messages for a session with pagination
pub async fn get_messages(
    session_id: &str,
    offset: usize,
    limit: usize,
) -> Result<PaginatedMessages, String> {
    let url = format!(
        "{}/session/{}/messages?offset={}&limit={}",
        API_BASE, session_id, offset, limit
    );
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }
    
    response
        .json::<PaginatedMessages>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Submit a prompt to a session
pub async fn submit_prompt(session_id: &str, prompt: &str) -> Result<PromptResponse, String> {
    let url = format!("{}/session/{}/prompt", API_BASE, session_id);
    let body = PromptRequest {
        prompt: prompt.to_string(),
    };
    
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }
    
    response
        .json::<PromptResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Abort a running session
pub async fn abort_session(session_id: &str) -> Result<(), String> {
    let url = format!("{}/session/{}/abort", API_BASE, session_id);
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }
    
    Ok(())
}

/// Delete a session
pub async fn delete_session(session_id: &str) -> Result<(), String> {
    let url = format!("{}/session/{}", API_BASE, session_id);
    let client = reqwest::Client::new();
    let response = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }
    
    Ok(())
}

/// SSE Event types for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum SseEvent {
    #[serde(rename = "session_created")]
    SessionCreated { session_id: String },
    #[serde(rename = "message_added")]
    MessageAdded { session_id: String, message_id: String },
    #[serde(rename = "session_updated")]
    SessionUpdated { session_id: String },
    #[serde(rename = "session_deleted")]
    SessionDeleted { session_id: String },
    #[serde(rename = "error")]
    Error { message: String },
}

/// Create an SSE connection for session events
pub fn create_sse_connection(session_id: &str) -> String {
    format!("{}/session/{}/events", API_BASE, session_id)
}

/// Create an SSE connection for global events
pub fn create_global_sse_connection() -> String {
    format!("{}/event", API_BASE)
}
