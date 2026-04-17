//! Serve command - HTTP server mode

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use std::sync::Arc;
use rcode_server::{start_server, AppState};
use rcode_providers::{parse_model_id, ProviderFactory};

#[derive(Args)]
pub struct Serve {
    #[arg(short, long, default_value = "4096")]
    port: u16,
    
    #[arg(long, default_value = "127.0.0.1")]
    hostname: String,
}

impl Serve {
    #[allow(clippy::await_holding_lock)]
    pub async fn execute(&self, config_path: Option<&PathBuf>, no_config: bool) -> Result<()> {
        let work_dir = std::env::current_dir().unwrap_or_default();
        let config = rcode_core::load_config(config_path.cloned(), no_config, Some(work_dir)).await?;
        let state = Arc::new(AppState::with_config(config));

        let config_ref = state.config.lock().unwrap();
        let model_string = rcode_core::resolve_model_from_config(&config_ref, None, None)
            .unwrap_or_else(|| "anthropic/claude-sonnet-4-5".to_string());
        
        let (provider_id, _) = parse_model_id(&model_string);
        let (provider, _) = ProviderFactory::build(&model_string, Some(&config_ref))?;
        drop(config_ref); // Release lock before registering provider
        state.providers.lock().unwrap().register(provider_id, provider);
        
        tracing::info!("Starting server on {}:{}", self.hostname, self.port);
        
        let result = start_server(state, self.port).await;
        
        match &result {
            Ok(()) => tracing::info!("Server exited cleanly"),
            Err(e) => tracing::error!("Server error: {}", e),
        }
        
        result
    }
}
