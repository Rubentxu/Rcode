//! TUI command - terminal UI

use anyhow::Result;
use clap::Args;
use opencode_event::EventBus;
use opencode_session::SessionService;
use opencode_tui::run;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Args)]
pub struct Tui {
    #[arg(short, long)]
    project: Option<String>,
    
    #[arg(short, long)]
    session: Option<String>,
}

impl Default for Tui {
    fn default() -> Self {
        Self {
            project: None,
            session: None,
        }
    }
}

impl Tui {
    pub async fn execute(&self, config_path: Option<&PathBuf>, no_config: bool) -> Result<()> {
        let _config = opencode_core::load_config(config_path.map(|p| p.clone()), no_config).await?;
        
        tracing::info!("TUI mode starting...");
        tracing::info!("Project: {:?}", self.project);
        tracing::info!("Session: {:?}", self.session);
        
        // Create shared services
        let event_bus = Arc::new(EventBus::new(100));
        let session_service = Arc::new(SessionService::new(event_bus.clone()));
        
        // Run the TUI
        run(session_service, event_bus).await
    }
}
