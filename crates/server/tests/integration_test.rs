//! Integration tests for the HTTP server

mod test_utils;

use rcode_core::SessionId;
use serde_json::json;
use test_utils::TestApp;

#[tokio::test]
async fn test_health_check() {
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/health", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn test_create_session() {
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/session", app.base_url()))
        .json(&json!({
            "project_path": "/test/project",
            "agent_id": "build",
            "model_id": "claude-sonnet-4-5"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["id"].is_string());
    assert_eq!(body["project_path"], "/test/project");
    assert_eq!(body["agent_id"], "build");
    assert_eq!(body["model_id"], "claude-sonnet-4-5");
    assert_eq!(body["status"], "idle");
}

#[tokio::test]
async fn test_get_session() {
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/session/{}", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["id"], session_id.0);
}

#[tokio::test]
async fn test_get_session_not_found() {
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/session/nonexistent", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "SESSION_NOT_FOUND");
}

#[tokio::test]
async fn test_list_sessions() {
    let app = TestApp::new().await;
    let _session1 = app.create_test_session().await;
    let _session2 = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/session", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: Vec<serde_json::Value> = response.json().await.unwrap();
    assert_eq!(body.len(), 2);
}

#[tokio::test]
async fn test_delete_session() {
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    
    // Delete the session
    let response = client
        .delete(format!("{}/session/{}", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    // Verify session is gone
    let response = client
        .get(format!("{}/session/{}", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_delete_session_not_found() {
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .delete(format!("{}/session/nonexistent", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_get_messages() {
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    // Add some messages
    app.add_test_message(&session_id.0, "Hello");
    app.add_test_message(&session_id.0, "World");
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/session/{}/messages", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(body["total"], 2);
    assert_eq!(body["offset"], 0);
    assert_eq!(body["limit"], 50);
}

#[tokio::test]
async fn test_get_messages_with_pagination() {
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    // Add 5 messages
    for i in 0..5 {
        app.add_test_message(&session_id.0, &format!("Message {}", i));
    }
    
    let client = reqwest::Client::new();
    
    // Get first page
    let response = client
        .get(format!("{}/session/{}/messages?offset=0&limit=2", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(body["total"], 5);
    assert_eq!(body["offset"], 0);
    assert_eq!(body["limit"], 2);
}

#[tokio::test]
async fn test_get_messages_not_found() {
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/session/nonexistent/messages", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_submit_prompt() {
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({
            "prompt": "Hello, world!"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["message_id"].is_string());
    assert_eq!(body["status"], "processing");
}

#[tokio::test]
async fn test_abort_session() {
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    // First, set session to Running state via submit_prompt
    let client = reqwest::Client::new();
    client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({ "prompt": "test" }))
        .send()
        .await
        .unwrap();
    
    // Now abort
    let response = client
        .post(format!("{}/session/{}/abort", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_abort_invalid_transition() {
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    // Session is in Idle state, cannot abort directly
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/session/{}/abort", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    // Should fail with invalid transition error
    assert_eq!(response.status(), 409);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "INVALID_TRANSITION");
}
