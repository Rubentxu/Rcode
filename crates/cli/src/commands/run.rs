//! Run command - non-interactive execution

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::sync::Arc;
use opencode_core::{
    AgentContext, Message, Part, Session, SessionStatus,
};
use opencode_server::AppState;
use opencode_providers::anthropic::AnthropicProvider;
use opencode_agent::{AgentExecutor, DefaultAgent};

#[derive(Args)]
pub struct Run {
    /// Direct message input
    #[arg(short, long)]
    pub message: Option<String>,
    
    /// Read prompt from file
    #[arg(short, long)]
    pub file: Option<String>,
    
    /// Read from stdin
    #[arg(long)]
    pub stdin: bool,
    
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
    
    /// Suppress stdout
    #[arg(long)]
    pub silent: bool,
    
    /// Persist session
    #[arg(long)]
    pub save_session: Option<bool>,
    
    #[arg(long, default_value = "claude-sonnet-4-5")]
    pub model: String,
    
    #[arg(short, long)]
    pub agent: Option<String>,
}

impl Run {
    pub async fn execute(&self, config_path: Option<&PathBuf>, no_config: bool) -> Result<()> {
        // Load config
        let config = opencode_core::load_config(config_path.map(|p| p.clone()), no_config).await?;
        
        // Get prompt content
        let prompt = self.get_prompt_content()?;
        
        // Create AppState with providers and tools
        let state = Arc::new(AppState::with_config(config.clone()));
        
        // Setup provider
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY must be set")?;
        let anthropic = Arc::new(AnthropicProvider::new(api_key));
        state.providers.lock().unwrap().register(anthropic);
        
        // Create session
        let session = Session::new(
            std::env::current_dir().unwrap_or_default(),
            self.agent.clone().unwrap_or_else(|| "default".to_string()),
            self.model.clone(),
        );
        let session = state.session_service.create(session);
        
        // Update status to running
        state.session_service.update_status(&session.id.0, SessionStatus::Running);
        
        // Publish agent started event
        state.event_bus.publish(opencode_event::Event::AgentStarted {
            session_id: session.id.0.clone(),
        });
        
        // Add user message
        let user_message = Message::user(
            session.id.0.clone(),
            vec![Part::Text { content: prompt.clone() }],
        );
        state.session_service.add_message(&session.id.0, user_message.clone());
        
        // Create AgentContext
        let mut ctx = AgentContext {
            session_id: session.id.0.clone(),
            messages: vec![user_message],
            project_path: session.project_path.clone(),
            cwd: std::env::current_dir().unwrap_or_default(),
            user_id: None,
        };
        
        // Create AgentExecutor with event bus
        let provider = state.providers.lock().unwrap().get("anthropic")
            .context("Anthropic provider not found")?
            .clone();
        let executor = AgentExecutor::new(
            Arc::new(DefaultAgent::new()),
            provider,
            state.tools.clone(),
        ).with_event_bus(state.event_bus.clone());
        
        tracing::info!("Starting agent execution for session {}", session.id.0);
        
        // Execute agent with streaming
        let result = executor.run(&mut ctx).await;
        
        let stop_reason = match &result {
            Ok(r) => format!("{:?}", r.stop_reason),
            Err(e) => format!("Error: {}", e),
        };
        
        // Display streaming output
        if !self.silent {
            if self.json {
                let output = serde_json::json!({
                    "session_id": session.id.0,
                    "message": ctx.messages.last(),
                    "stop_reason": stop_reason,
                    "status": if result.is_ok() { "completed" } else { "error" }
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                // Print the assistant's final message
                if let Some(msg) = ctx.messages.last() {
                    for part in &msg.parts {
                        match part {
                            Part::Text { content } => print!("{}", content),
                            Part::ToolCall { name, .. } => print!("[Calling tool: {}]", name),
                            Part::ToolResult { content, .. } => print!("{}", content),
                            _ => {}
                        }
                    }
                    println!();
                }
            }
        }
        
        // Update status to completed or aborted based on result
        let final_status = if result.is_ok() {
            SessionStatus::Completed
        } else {
            tracing::warn!("Agent execution failed: {:?}", result.as_ref().err());
            SessionStatus::Aborted
        };
        state.session_service.update_status(&session.id.0, final_status);
        
        // Publish agent finished event
        state.event_bus.publish(opencode_event::Event::AgentFinished {
            session_id: session.id.0.clone(),
        });
        
        // Optionally persist session
        if self.save_session.unwrap_or(true) {
            tracing::info!("Session {} saved", session.id.0);
        }
        
        if result.is_err() {
            anyhow::bail!("Agent execution failed: {:?}", result.err());
        }
        
        Ok(())
    }
    
    fn get_prompt_content(&self) -> Result<String> {
        // Priority: message > file > stdin
        if let Some(ref msg) = self.message {
            return Ok(msg.clone());
        }
        
        if let Some(ref path) = self.file {
            let content = std::fs::read_to_string(path)
                .context(format!("Failed to read file: {}", path))?;
            return Ok(content);
        }
        
        if self.stdin {
            let mut buffer = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)
                .context("Failed to read from stdin")?;
            return Ok(buffer);
        }
        
        anyhow::bail!("No input provided. Use --message, --file, or --stdin");
    }
}
