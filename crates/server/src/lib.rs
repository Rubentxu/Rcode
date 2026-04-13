//! HTTP Server using Axum
#![allow(
    clippy::collapsible_if,
    clippy::redundant_closure,
    clippy::manual_contains,
    clippy::needless_borrows_for_generic_args,
    clippy::unnecessary_lazy_evaluations,
    clippy::map_identity,
    clippy::needless_borrow,
    clippy::while_let_loop,
    clippy::collapsible_str_replace,
    clippy::nonminimal_bool,
    clippy::bool_comparison
)]

pub mod routes;
pub mod state;
pub mod error;
pub mod cancellation;
pub mod subagent_runner_impl;
pub mod cache_store_impl;
pub mod explorer;

pub use state::AppState;
pub use error::ServerError;

use axum::{
    Router,
    http::HeaderValue,
    routing::{get, post, delete, put, patch},
};
use std::sync::Arc;
use tokio::signal;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;

/// Build a CORS layer from server configuration
pub fn build_cors_layer(cors_origins: Option<Vec<String>>) -> CorsLayer {
    let layer = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any);
    
    match cors_origins {
        Some(origins) if !origins.is_empty() => {
            // Use the configured origins
            let allowed: Vec<HeaderValue> = origins
                .iter()
                .filter_map(|origin| {
                    origin.parse::<HeaderValue>().ok()
                })
                .collect();
            
            if allowed.is_empty() {
                // Fallback to dev defaults if no valid origins could be parsed
                layer.allow_origin(default_dev_origins())
            } else {
                layer.allow_origin(allowed)
            }
        }
        _ => {
            // Default: allow common development origins
            // Covers: Tauri webview (tauri.localhost), Vite dev (localhost:1420),
            // direct browser (localhost:4098), and localhost variants
            layer.allow_origin(default_dev_origins())
        }
    }
}

/// Default development CORS origins for Tauri + Vite + direct browser access
fn default_dev_origins() -> Vec<HeaderValue> {
    vec![
        HeaderValue::from_static("http://localhost"),
        HeaderValue::from_static("http://localhost:1420"),
        HeaderValue::from_static("http://localhost:4096"),
        HeaderValue::from_static("http://localhost:4098"),
        HeaderValue::from_static("http://127.0.0.1"),
        HeaderValue::from_static("http://127.0.0.1:1420"),
        HeaderValue::from_static("http://127.0.0.1:4098"),
        HeaderValue::from_static("tauri://localhost"),
        HeaderValue::from_static("tauri://127.0.0.1"),
        HeaderValue::from_static("https://tauri.localhost"),
        HeaderValue::from_static("http://tauri.localhost"),
        HeaderValue::from_static("http://localhost:5173"), // Vite default
    ]
}

pub async fn create_app(state: Arc<AppState>) -> Router {
    // Extract CORS origins from config
    let cors_origins = state.config.lock()
        .ok()
        .and_then(|config| config.server.clone())
        .and_then(|server| server.cors);
    
    let cors = build_cors_layer(cors_origins);
    
    Router::new()
        .route("/health", get(routes::health))
        .route("/projects", get(routes::list_projects))
        .route("/projects", post(routes::create_project))
        .route("/projects/:id/sessions", get(routes::list_project_sessions))
        .route("/projects/:id", delete(routes::delete_project))
        .route("/session", get(routes::list_sessions))
        .route("/session", post(routes::create_session))
        .route("/session/:id", get(routes::get_session))
        .route("/session/:id", patch(routes::rename_session))
        .route("/session/:id", delete(routes::delete_session))
        .route("/session/:id/messages", get(routes::get_messages))
        .route("/session/:id/prompt", post(routes::submit_prompt))
        .route("/session/:id/abort", post(routes::abort_session))
        .route("/session/:id/events", get(routes::sse_session_events))
        .route("/session/:id/children", get(routes::get_session_children))
        .route("/event", get(routes::sse_events))
        .route("/models", get(routes::list_models))
        .route("/connect", post(routes::connect_session))
        .route("/config", get(routes::get_config))
        .route("/config", put(routes::update_config))
        .route("/config/providers", get(routes::get_providers))
        .route("/config/providers/:id", put(routes::update_provider))
        .route("/config/providers/:id/state", put(routes::update_provider_state))
        .route("/config/models/:id/state", put(routes::update_model_state))
        .route("/terminal/exec", post(routes::terminal::exec_terminal_command))
        .route("/session/:id/diffs", get(routes::diff::list_diffs))
        .route("/session/:id/diff/:file", get(routes::diff::get_diff))
        .route("/permission/:request_id/grant", post(routes::permission_grant))
        .route("/permission/:request_id/deny", post(routes::permission_deny))
        // Explorer routes
        .route("/explorer/bootstrap", get(routes::explorer_bootstrap))
        .route("/explorer/tree", get(routes::explorer_tree))
        // Outline route
        .route("/outline", get(routes::get_outline))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                    )
                })
                .on_response(|response: &axum::http::Response<_>, latency: std::time::Duration, _span: &tracing::Span| {
                    tracing::info!(status = %response.status(), latency_ms = latency.as_millis(), "request completed");
                })
                .on_failure(|error: tower_http::classify::ServerErrorsFailureClass, latency: std::time::Duration, _span: &tracing::Span| {
                    tracing::error!(%error, latency_ms = latency.as_millis(), "request failed");
                })
                .on_request(|request: &axum::http::Request<_>, _span: &tracing::Span| {
                    tracing::debug!(method = %request.method(), uri = %request.uri(), "request started");
                }),
        )
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

#[cfg(test)]
mod tests {
    use super::default_dev_origins;

    #[test]
    fn cors_defaults_include_tauri_origins() {
        let origins = default_dev_origins();
        let as_strings: Vec<&str> = origins
            .iter()
            .map(|value| value.to_str().expect("default origin should be valid ASCII"))
            .collect();

        assert!(as_strings.contains(&"tauri://localhost"));
        assert!(as_strings.contains(&"tauri://127.0.0.1"));
    }
}
