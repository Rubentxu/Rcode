//! TUI binary entry point

use rcode_core::RcodeConfig;
use rcode_event::EventBus;
use rcode_session::create_default_session_service;
use rcode_tools::ToolRegistryService;
use rcode_tui::run;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Create shared services
    let event_bus = Arc::new(EventBus::new(100));
    let session_service = Arc::new(create_default_session_service(event_bus.clone()));
    let tools = Arc::new(ToolRegistryService::with_session_service(session_service.clone()));
    let config = RcodeConfig::default();

    // Run the TUI
    run(session_service, event_bus, tools, config).await
}
