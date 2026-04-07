//! TUI command - terminal UI

use anyhow::Result;
use clap::Args;
use rcode_event::EventBus;
use rcode_session::create_default_session_service;
use rcode_tools::ToolRegistryService;
use rcode_tui::run;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Args, Default)]
pub struct Tui {
    #[arg(short, long)]
    project: Option<String>,
    
    #[arg(short, long)]
    session: Option<String>,
}

impl Tui {
    pub async fn execute(&self, config_path: Option<&PathBuf>, no_config: bool) -> Result<()> {
        let work_dir = std::env::current_dir().unwrap_or_default();
        let config = rcode_core::load_config(config_path.cloned(), no_config, Some(work_dir)).await?;
        
        tracing::info!("TUI mode starting...");
        tracing::info!("Project: {:?}", self.project);
        tracing::info!("Session: {:?}", self.session);
        
        // Create shared services
        let event_bus = Arc::new(EventBus::new(100));
        let session_service = Arc::new(create_default_session_service(event_bus.clone()));
        let tools = Arc::new(ToolRegistryService::with_session_service(session_service.clone()));
        
        // Run the TUI
        run(session_service, event_bus, tools, config).await
    }
}
