//! Serve command - HTTP server mode

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use std::sync::Arc;
use opencode_server::{start_server, AppState};
use opencode_providers::anthropic::AnthropicProvider;

#[derive(Args)]
pub struct Serve {
    #[arg(short, long, default_value = "4096")]
    port: u16,
    
    #[arg(short, long, default_value = "127.0.0.1")]
    hostname: String,
}

impl Serve {
    pub async fn execute(&self, config_path: Option<&PathBuf>, no_config: bool) -> Result<()> {
        let config = opencode_core::load_config(config_path.map(|p| p.clone()), no_config).await?;
        let state = Arc::new(AppState::with_config(config));
        
        let anthropic = Arc::new(AnthropicProvider::new(
            std::env::var("ANTHROPIC_API_KEY")
                .expect("ANTHROPIC_API_KEY must be set"),
        ));
        state.providers.lock().unwrap().register(anthropic);
        
        tracing::info!("Starting server on {}:{}", self.hostname, self.port);
        
        let result = start_server(state, self.port).await;
        
        match &result {
            Ok(()) => tracing::info!("Server exited cleanly"),
            Err(e) => tracing::error!("Server error: {}", e),
        }
        
        result
    }
}
