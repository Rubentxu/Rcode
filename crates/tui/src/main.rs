//! TUI binary entry point

use opencode_event::EventBus;
use opencode_session::SessionService;
use opencode_tui::run;
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
    let session_service = Arc::new(SessionService::new(event_bus.clone()));

    // Run the TUI
    run(session_service, event_bus).await
}
