//! Run command - non-interactive execution

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::sync::Arc;
use rcode_core::{
    AgentContext, Message, Part, Session, SessionStatus,
};
use rcode_server::AppState;
use rcode_providers::{parse_model_id, ProviderFactory};
use rcode_agent::{AgentExecutor, DefaultAgent};

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
    
    /// Model to use (format: provider/model or just model name)
    #[arg(long)]
    pub model: Option<String>,
    
    #[arg(short, long)]
    pub agent: Option<String>,
}

impl Run {
    pub async fn execute(&self, config_path: Option<&PathBuf>, no_config: bool) -> Result<()> {
        let work_dir = std::env::current_dir().unwrap_or_default();
        let config = rcode_core::load_config(config_path.map(|p| p.clone()), no_config, Some(work_dir.clone())).await?;
        
        let prompt = self.get_prompt_content()?;
        
        let state = Arc::new(AppState::with_config(config.clone()));

        let model_string = rcode_core::resolve_model_from_config(&config, self.model.as_deref(), self.agent.as_deref())
            .unwrap_or_else(|| "anthropic/claude-sonnet-4-5".to_string());
        
        let (provider_id, model_name) = parse_model_id(&model_string);
        let (provider, _) = ProviderFactory::build(&model_string, Some(&config))
            .context(format!("Failed to configure provider '{}'", provider_id))?;
        state.providers.lock().unwrap().register(provider.clone());
        
        let session = Session::new(
            work_dir,
            self.agent.clone().unwrap_or_else(|| "default".to_string()),
            model_name.clone(),
        );
        let session = state.session_service.create(session);
        
        state.session_service.update_status(&session.id.0, SessionStatus::Running);
        
        state.event_bus.publish(rcode_event::Event::AgentStarted {
            session_id: session.id.0.clone(),
        });
        
        let user_message = Message::user(
            session.id.0.clone(),
            vec![Part::Text { content: prompt.clone() }],
        );
        state.session_service.add_message(&session.id.0, user_message.clone());
        
        let mut ctx = AgentContext {
            session_id: session.id.0.clone(),
            messages: vec![user_message],
            project_path: session.project_path.clone(),
            cwd: std::env::current_dir().unwrap_or_default(),
            user_id: None,
            model_id: model_name.clone(),
        };
        
        let provider = state.providers.lock().unwrap().get(&provider_id)
            .with_context(|| format!("Provider '{}' not found", provider_id))?
            .clone();
        let executor = AgentExecutor::new(
            Arc::new(DefaultAgent::new()),
            provider,
            state.tools.clone(),
        ).with_event_bus(state.event_bus.clone());
        
        tracing::info!("Starting agent execution for session {}", session.id.0);
        
        let result = executor.run(&mut ctx).await;
        
        let stop_reason = match &result {
            Ok(r) => format!("{:?}", r.stop_reason),
            Err(e) => format!("Error: {}", e),
        };
        
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
        
        let final_status = if result.is_ok() {
            SessionStatus::Completed
        } else {
            tracing::warn!("Agent execution failed: {:?}", result.as_ref().err());
            SessionStatus::Aborted
        };
        state.session_service.update_status(&session.id.0, final_status);
        
        state.event_bus.publish(rcode_event::Event::AgentFinished {
            session_id: session.id.0.clone(),
        });
        
        if self.save_session.unwrap_or(true) {
            tracing::info!("Session {} saved", session.id.0);
        }
        
        if result.is_err() {
            anyhow::bail!("Agent execution failed: {:?}", result.err());
        }
        
        Ok(())
    }
    
    fn get_prompt_content(&self) -> Result<String> {
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
