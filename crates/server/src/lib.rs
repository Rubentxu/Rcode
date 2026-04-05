//! HTTP Server using Axum

pub mod routes;
pub mod state;
pub mod error;

pub use state::AppState;
pub use error::ServerError;

use axum::{
    Router,
    routing::{get, post, delete, put},
};
use std::sync::Arc;
use tokio::signal;
use tower_http::cors::{CorsLayer, Any};

/// Build a CORS layer from server configuration
pub fn build_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}

pub async fn create_app(state: Arc<AppState>) -> Router {
    let cors = build_cors_layer();
    
    Router::new()
        .route("/health", get(routes::health))
        .route("/session", get(routes::list_sessions))
        .route("/session", post(routes::create_session))
        .route("/session/:id", get(routes::get_session))
        .route("/session/:id", delete(routes::delete_session))
        .route("/session/:id/messages", get(routes::get_messages))
        .route("/session/:id/prompt", post(routes::submit_prompt))
        .route("/session/:id/abort", post(routes::abort_session))
        .route("/session/:id/events", get(routes::sse_session_events))
        .route("/event", get(routes::sse_events))
        .route("/models", get(routes::list_models))
        .route("/connect", post(routes::connect_session))
        .route("/config", get(routes::get_config))
        .route("/config", put(routes::update_config))
        .route("/config/providers", get(routes::get_providers))
        .route("/config/providers/:id", put(routes::update_provider))
        .route("/terminal/exec", post(routes::terminal::exec_terminal_command))
        .route("/session/:id/diffs", get(routes::diff::list_diffs))
        .route("/session/:id/diff/:file", get(routes::diff::get_diff))
        .with_state(state)
        .layer(cors)
}

/// Run the server with graceful shutdown support
pub async fn start_server(state: Arc<AppState>, port: u16) -> anyhow::Result<()> {
    let app = create_app(state).await;
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    tracing::info!("Server listening on {}", addr);
    
    // Create a shutdown signal future
    let shutdown = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        tracing::info!("Shutdown signal received");
    };

    // Run with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    tracing::info!("Shutdown complete");
    Ok(())
}

/// Run the server with a custom shutdown future
pub async fn start_server_with_shutdown<F>(
    state: Arc<AppState>,
    port: u16,
    shutdown: F,
) -> anyhow::Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let app = create_app(state).await;
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    tracing::info!("Server listening on {}", addr);
    
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    tracing::info!("Shutdown complete");
    Ok(())
}

/// Run the server on a pre-bound listener with a oneshot shutdown signal.
/// Does NOT install ctrl_c handler - caller manages shutdown.
/// Returns the local socket address of the listener.
pub async fn start_server_on_listener(
    state: Arc<AppState>,
    listener: tokio::net::TcpListener,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<std::net::SocketAddr> {
    let app = create_app(state).await;
    let addr = listener.local_addr()?;
    
    tracing::info!("Server listening on {}", addr);
    
    // Convert oneshot receiver to a future that completes on shutdown signal
    let shutdown_future = async {
        match shutdown.await {
            Ok(()) => tracing::info!("Shutdown signal received via oneshot"),
            Err(_) => tracing::info!("Shutdown sender dropped without sending signal"),
        }
    };
    
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_future)
        .await?;

    tracing::info!("Shutdown complete");
    Ok(addr)
}
