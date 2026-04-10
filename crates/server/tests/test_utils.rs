//! Test utilities for integration testing
#![allow(unused_imports, dead_code)]

use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use rcode_server::{create_app, AppState};
use rcode_core::{Session, SessionId, RcodeConfig, Part, Message};
use rcode_providers::LlmProvider;

pub struct TestApp {
    pub state: Arc<AppState>,
    pub port: u16,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl TestApp {
    pub async fn new() -> Self {
        let config = RcodeConfig::default();
        let state = Arc::new(AppState::with_config(config));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // Bind to a random available port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Spawn the server
        let app = create_app(state.clone()).await;
        let server = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
            });

        // Spawn server in background
        tokio::spawn(async move {
            server.await.ok();
        });

        Self {
            state,
            port,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Create a TestApp with a custom config (e.g., for testing disabled providers)
    pub async fn with_config(config: RcodeConfig) -> Self {
        let state = Arc::new(AppState::with_config(config));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // Bind to a random available port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Spawn the server
        let app = create_app(state.clone()).await;
        let server = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
            });

        // Spawn server in background
        tokio::spawn(async move {
            server.await.ok();
        });

        Self {
            state,
            port,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Create a TestApp with a mock provider injected
    pub async fn with_mock_provider(provider: Arc<dyn LlmProvider>) -> Self {
        let config = RcodeConfig::default();
        let state = Arc::new(AppState::with_config(config));
        
        // Inject the mock provider
        {
            let mut mock_guard = state.mock_provider.lock().unwrap();
            *mock_guard = Some(provider);
        }
        
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // Bind to a random available port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Spawn the server
        let app = create_app(state.clone()).await;
        let server = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
            });

        // Spawn server in background
        tokio::spawn(async move {
            server.await.ok();
        });

        Self {
            state,
            port,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub async fn create_test_session(&self) -> SessionId {
        let session = Session::new(
            std::path::PathBuf::from("/test/project"),
            "test-agent".to_string(),
            "claude-sonnet-4-5".to_string(),
        );
        let session_id = session.id.0.clone();
        self.state.session_service.create(session);
        SessionId(session_id)
    }

    /// Create a test session with a real temporary directory path (for explorer tests)
    pub async fn create_test_session_with_real_path(&self) -> (SessionId, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let session = Session::new(
            temp_dir.path().to_path_buf(),
            "test-agent".to_string(),
            "claude-sonnet-4-5".to_string(),
        );
        let session_id = session.id.0.clone();
        self.state.session_service.create(session);
        (SessionId(session_id), temp_dir)
    }

    pub fn add_test_message(&self, session_id: &str, content: &str) {
        let message = Message::user(
            session_id.to_string(),
            vec![Part::Text {
                content: content.to_string(),
            }],
        );
        self.state.session_service.add_message(session_id, message);
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Shutdown the test server
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.shutdown();
    }
}