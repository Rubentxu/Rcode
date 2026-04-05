//! ACP Server - stdio-based JSON-RPC 2.0 server

use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::RwLock;
use rcode_core::{AgentContext, Message, Part, SessionStatus};

use crate::protocol::{JsonRpcRequest, JsonRpcResponse, JsonRpcError, JsonRpcErrorCode};
use crate::session::ACPSessionManager;

pub struct AcpServer {
    session_manager: ACPSessionManager,
    session_service: Option<Arc<rcode_session::SessionService>>,
    event_bus: Option<Arc<rcode_event::EventBus>>,
    tool_registry: Option<Arc<rcode_tools::ToolRegistryService>>,
    agent_executor: Option<Arc<rcode_agent::AgentExecutor>>,
    cancellation_tokens: Arc<RwLock<std::collections::HashMap<String, rcode_agent::CancellationToken>>>,
}

impl AcpServer {
    pub fn new() -> Self {
        Self {
            session_manager: ACPSessionManager::new(),
            session_service: None,
            event_bus: None,
            tool_registry: None,
            agent_executor: None,
            cancellation_tokens: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub fn with_session_service(mut self, service: Arc<rcode_session::SessionService>) -> Self {
        self.session_service = Some(Arc::clone(&service));
        self.session_manager = self.session_manager.with_session_service(service);
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<rcode_event::EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_tool_registry(mut self, registry: Arc<rcode_tools::ToolRegistryService>) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    pub fn with_agent_executor(mut self, executor: Arc<rcode_agent::AgentExecutor>) -> Self {
        self.agent_executor = Some(executor);
        self
    }

    pub async fn run(&self) -> Result<()> {
        let stdin = BufReader::new(std::io::stdin());
        let mut lines = stdin.lines();
        let mut stdout = std::io::stdout().lock();

        while let Some(line) = lines.next() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Failed to read stdin: {}", e);
                    break;
                }
            };

            if line.trim().is_empty() {
                continue;
            }

            let response = self.handle_line(&line).await;
            if let Some(response) = response {
                let json = serde_json::to_string(&response)?;
                writeln!(stdout, "{}", json)?;
                stdout.flush()?;
            }
        }

        Ok(())
    }

    async fn handle_line(&self, line: &str) -> Option<JsonRpcResponse> {
        let request: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to parse JSON-RPC request: {}", e);
                return Some(JsonRpcResponse::error(
                    None,
                    JsonRpcError::new(JsonRpcErrorCode::ParseError, format!("Invalid JSON: {}", e)),
                ));
            }
        };

        Some(self.handle_request(request).await)
    }

    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone();

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params).await,
            "session/create" => self.handle_session_create(request.params).await,
            "session/destroy" => self.handle_session_destroy(request.params).await,
            "session/load" => self.handle_session_load(request.params).await,
            "execute" => self.handle_execute(request.params).await,
            "cancel" => self.handle_cancel(request.params).await,
            "tools/list" => self.handle_tools_list().await,
            "tools/call" => self.handle_tools_call(request.params).await,
            _ => Err(anyhow::anyhow!("Method not found: {}", request.method)),
        };

        match result {
            Ok(result) => JsonRpcResponse::success(id, result),
            Err(e) => {
                tracing::error!("Handler error: {}", e);
                JsonRpcResponse::error(
                    id,
                    JsonRpcError::new(JsonRpcErrorCode::ServerError, e.to_string()),
                )
            }
        }
    }

    async fn handle_initialize(&self, _params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "protocolVersion": "1.0",
            "capabilities": {
                "streaming": true,
                "tools": true,
                "sessions": true,
                "cancel": true,
            },
            "serverInfo": {
                "name": "rcode-acp",
                "version": "0.1.0"
            }
        }))
    }

    async fn handle_session_create(&self, _params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let session_id = self.session_manager.create_session().await?;
        
        self.cancellation_tokens.write().await.insert(
            session_id.clone(),
            rcode_agent::CancellationToken::new(),
        );
        
        Ok(serde_json::json!({ "sessionId": session_id }))
    }

    async fn handle_session_destroy(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let session_id = params
            .and_then(|p| p.get("sessionId").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| anyhow::anyhow!("sessionId required"))?;

        self.cancellation_tokens.write().await.remove(&session_id);
        self.session_manager.destroy_session(&session_id).await?;
        Ok(serde_json::json!({ "ok": true }))
    }

    async fn handle_session_load(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let session_id = params
            .and_then(|p| p.get("sessionId").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| anyhow::anyhow!("sessionId required"))?;

        let session = self.session_manager.load_session(&session_id).await?;
        let messages = self.session_manager.get_messages(&session_id).await;

        Ok(serde_json::json!({
            "session": {
                "id": session.id.0,
                "projectPath": session.project_path,
                "agentId": session.agent_id,
                "modelId": session.model_id,
                "status": format!("{:?}", session.status).to_lowercase(),
            },
            "messages": messages.iter().map(|m| serde_json::json!({
                "id": m.id.0,
                "role": format!("{:?}", m.role).to_lowercase(),
                "parts": m.parts,
            })).collect::<Vec<_>>(),
        }))
    }

    async fn handle_execute(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let (session_id, prompt) = self.extract_session_and_prompt(params).await?;

        if !self.session_manager.has_session(&session_id).await {
            return Err(anyhow::anyhow!("Session not found: {}", session_id));
        }

        let cancellation_token = self.cancellation_tokens.read().await
            .get(&session_id)
            .cloned()
            .unwrap_or_else(rcode_agent::CancellationToken::new);

        let mut messages = self.session_manager.get_messages(&session_id).await;
        
        let mut user_message = Message::user(session_id.clone(), vec![Part::Text {
            content: prompt.clone(),
        }]);
        
        if let Some(svc) = &self.session_service {
            svc.add_message(&session_id, user_message.clone());
        }
        
        user_message.id.0 = uuid::Uuid::new_v4().to_string();
        messages.push(user_message);

        let mut ctx = AgentContext {
            session_id: session_id.clone(),
            project_path: std::env::current_dir().unwrap_or_default(),
            cwd: std::env::current_dir().unwrap_or_default(),
            user_id: None,
            model_id: "claude-sonnet-4-5".to_string(),
            messages,
        };

        let result = if let Some(executor) = &self.agent_executor {
            executor.run_with_cancellation(&mut ctx, cancellation_token).await
        } else {
            return Err(anyhow::anyhow!("Agent executor not configured"));
        };

        match result {
            Ok(agent_result) => {
                if let Some(svc) = &self.session_service {
                    svc.add_message(&session_id, agent_result.message.clone());
                    let new_status = match agent_result.stop_reason {
                        rcode_core::agent::StopReason::EndOfTurn => SessionStatus::Completed,
                        rcode_core::agent::StopReason::MaxSteps => SessionStatus::Completed,
                        rcode_core::agent::StopReason::UserStopped => SessionStatus::Aborted,
                        rcode_core::agent::StopReason::Error => SessionStatus::Aborted,
                        rcode_core::agent::StopReason::ToolCalls(_) => SessionStatus::Running,
                    };
                    svc.update_status(&session_id, new_status);
                }

                let text_content = agent_result.message.parts.iter()
                    .filter_map(|p| match p {
                        Part::Text { content } => Some(content.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(serde_json::json!({
                    "type": "execute_result",
                    "sessionId": session_id,
                    "result": text_content,
                    "shouldContinue": agent_result.should_continue,
                    "stopReason": format!("{:?}", agent_result.stop_reason).to_lowercase(),
                }))
            }
            Err(e) => Err(anyhow::anyhow!("Execution failed: {}", e)),
        }
    }

    async fn handle_cancel(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let session_id = params
            .and_then(|p| p.get("sessionId").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| anyhow::anyhow!("sessionId required"))?;

        if let Some(token) = self.cancellation_tokens.read().await.get(&session_id) {
            token.cancel();
            tracing::info!("Cancelled session: {}", session_id);
        }

        Ok(serde_json::json!({
            "type": "cancel_ack",
            "sessionId": session_id,
        }))
    }

    async fn handle_tools_list(&self) -> Result<serde_json::Value> {
        if let Some(registry) = &self.tool_registry {
            let tools = registry.list();
            Ok(serde_json::json!({
                "tools": tools
            }))
        } else {
            Ok(serde_json::json!({
                "tools": []
            }))
        }
    }

    async fn handle_tools_call(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("params required"))?;
        let tool_id = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("tool name required"))?
            .to_string();
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        if let Some(registry) = &self.tool_registry {
            let context = rcode_core::ToolContext {
                session_id: "acp-session".to_string(),
                project_path: std::env::current_dir().unwrap_or_default(),
                cwd: std::env::current_dir().unwrap_or_default(),
                user_id: None,
                agent: "acp".to_string(),
            };

            match registry.execute(&tool_id, arguments, &context).await {
                Ok(result) => Ok(serde_json::json!({
                    "ok": true,
                    "result": result.content
                })),
                Err(e) => Ok(serde_json::json!({
                    "ok": false,
                    "error": e.to_string()
                })),
            }
        } else {
            Ok(serde_json::json!({
                "ok": false,
                "error": "Tool registry not available"
            }))
        }
    }

    async fn extract_session_and_prompt(&self, params: Option<serde_json::Value>) -> Result<(String, String)> {
        let params = params.ok_or_else(|| anyhow::anyhow!("params required"))?;
        let session_id = params
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("sessionId required"))?
            .to_string();
        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("prompt required"))?
            .to_string();
        Ok((session_id, prompt))
    }
}

impl Default for AcpServer {
    fn default() -> Self {
        Self::new()
    }
}
