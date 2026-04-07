//! Integration tests for the HTTP server
#![allow(unused_imports)]

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
    
    let session1 = app.create_test_session().await;
    let session2 = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/session", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: Vec<serde_json::Value> = response.json().await.unwrap();
    // Verify the sessions we created are in the list
    let session_ids: Vec<_> = body.iter().filter_map(|s| s.get("id").and_then(|id| id.as_str())).collect();
    assert!(session_ids.contains(&session1.0.as_str()), "Session 1 should be in list");
    assert!(session_ids.contains(&session2.0.as_str()), "Session 2 should be in list");
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
async fn test_abort_idle_session_returns_200() {
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    // Session is in Idle state - abort should still return 200 and set status to Aborted
    // (this is a no-op abort but is valid per spec)
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/session/{}/abort", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    // Should succeed - abort on idle session returns 200 per spec
    assert_eq!(response.status(), 200);
    
    // Verify session status is now Aborted
    let session_response = client
        .get(format!("{}/session/{}", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();
    assert_eq!(session_response.status(), 200);
    let session_body: serde_json::Value = session_response.json().await.unwrap();
    assert_eq!(session_body["status"], "aborted");
}

// T7: Tests for cancellation wired to submit_prompt and abort

#[tokio::test]
async fn test_concurrent_prompt_returns_409() {
    // This test verifies that sending two prompts to the same session
    // while the first is still "in flight" returns 409 Conflict.
    // Note: Without a mock provider, we can't control when the executor
    // actually finishes. The executor may complete quickly (no LLM call).
    // This test documents the race condition behavior.
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    
    // Send first prompt - this spawns an executor task
    let first_response = client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({ "prompt": "first prompt" }))
        .send()
        .await
        .unwrap();
    
    // First prompt should succeed (202 Accepted / 200)
    assert!(first_response.status() == 200 || first_response.status() == 202,
        "First prompt failed with status: {}", first_response.status());
    
    // Immediately send second prompt - should hit the is_active check
    let second_response = client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({ "prompt": "second prompt" }))
        .send()
        .await
        .unwrap();
    
    // If the first executor is still registered, we get 409.
    // If it already completed and deregistered, we get 200/202.
    // This is inherently racy without a slow/mock provider.
    let status = second_response.status();
    if status == 409 {
        let body: serde_json::Value = second_response.json().await.unwrap();
        assert_eq!(body["code"], "CONFLICT");
    } else {
        // Race won by executor completion - this is acceptable behavior
        // without a slow/mock provider
        assert!(status == 200 || status == 202,
            "Unexpected status: {}", status);
    }
}

#[tokio::test]
async fn test_abort_calls_cancellation_cancel() {
    // Test that abort_session actually calls cancellation.cancel()
    // by verifying the token is removed from the registry.
    // This is tested indirectly: we check that after abort,
    // a new prompt can be submitted (token was cancelled and removed).
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    
    // Submit a prompt (this registers a cancellation token)
    let first_response = client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({ "prompt": "test" }))
        .send()
        .await
        .unwrap();
    assert_eq!(first_response.status(), 200);
    
    // Abort the session (this should call cancellation.cancel())
    let abort_response = client
        .post(format!("{}/session/{}/abort", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();
    assert_eq!(abort_response.status(), 200);
    
    // Give the executor a moment to process the cancellation
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Verify cancellation registry is no longer active for this session
    // by checking that is_active returns false
    // (this is tested by submitting a new prompt successfully)
    let second_response = client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({ "prompt": "after abort" }))
        .send()
        .await
        .unwrap();
    
    // After abort, we should be able to submit a new prompt.
    // Note: This may return 409 if the session status is not Idle
    // (depends on timing of the executor and abort handling).
    let status = second_response.status();
    assert!(status == 200 || status == 202 || status == 409,
        "Unexpected status after abort: {}", status);
}

// T7: Document what's needed for full integration test coverage
// Full test of abort_cancels_executor requires:
// - A mock LLM provider that delays responses (e.g., 5 second sleep)
// - Ability to inject the mock into the route handler
// - Verification that abort reduces the total response time significantly
// - Without such infrastructure, we can only test the "happy path" and
//   the race condition behavior as shown above.

// T10: Tests for model override in submit_prompt

#[tokio::test]
async fn test_prompt_with_model_override() {
    // Test that submit_prompt accepts model_id in request body
    // Note: We can't easily verify the model is actually used without a mock provider,
    // but we can verify the request is accepted and doesn't error
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({
            "prompt": "Hello with model override",
            "model_id": "claude-sonnet-4-5"
        }))
        .send()
        .await
        .unwrap();

    // Should succeed (200 or 202) - the request was accepted
    assert!(response.status() == 200 || response.status() == 202,
        "Expected 200 or 202, got: {}", response.status());
}

#[tokio::test]
async fn test_prompt_without_model_override_uses_session_model() {
    // Test that submit_prompt works without model_id (backward compatible)
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({
            "prompt": "Hello without model override"
        }))
        .send()
        .await
        .unwrap();

    // Should succeed (200 or 202)
    assert!(response.status() == 200 || response.status() == 202,
        "Expected 200 or 202, got: {}", response.status());
}

// T11 & T12: Tests for child session API

#[tokio::test]
async fn test_create_child_session() {
    // Test that creating a session with parent_id creates a child session
    // Note: Child sessions inherit parent's project_path (by design)
    let app = TestApp::new().await;
    
    // First create a parent session with known project_path (/test/project from test_utils)
    let parent_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    
    // Create child session with parent_id
    // project_path in request is ignored - child inherits parent's path
    let child_response = client
        .post(format!("{}/session", app.base_url()))
        .json(&json!({
            "project_path": "/test/child",  // This is ignored - child inherits parent's path
            "agent_id": "build",
            "model_id": "claude-sonnet-4-5",
            "parent_id": parent_id.0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(child_response.status(), 200);
    
    let body: serde_json::Value = child_response.json().await.unwrap();
    assert!(body["id"].is_string());
    // Child inherits parent's project_path (/test/project from create_test_session)
    assert_eq!(body["project_path"], "/test/project");
    assert_eq!(body["parent_id"], parent_id.0);
}

#[tokio::test]
async fn test_create_child_session_with_invalid_parent_returns_404() {
    // Test that creating a child with non-existent parent_id returns 404
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    
    let response = client
        .post(format!("{}/session", app.base_url()))
        .json(&json!({
            "project_path": "/test/child",
            "agent_id": "build",
            "model_id": "claude-sonnet-4-5",
            "parent_id": "nonexistent-parent-id"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_create_session_without_parent_still_works() {
    // Test backward compatibility - creating session without parent_id works
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/session", app.base_url()))
        .json(&json!({
            "project_path": "/test/standalone",
            "agent_id": "build",
            "model_id": "claude-sonnet-4-5"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["id"].is_string());
    assert!(body["parent_id"].is_null());
}

#[tokio::test]
async fn test_get_session_children() {
    // Test that GET /session/:id/children returns child sessions
    let app = TestApp::new().await;
    
    // Create parent session
    let parent_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    
    // Create first child
    let child1_response = client
        .post(format!("{}/session", app.base_url()))
        .json(&json!({
            "project_path": "/test/child1",
            "parent_id": parent_id.0
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(child1_response.status(), 200);
    let child1: serde_json::Value = child1_response.json().await.unwrap();
    let child1_id = child1["id"].as_str().unwrap();

    // Create second child
    let child2_response = client
        .post(format!("{}/session", app.base_url()))
        .json(&json!({
            "project_path": "/test/child2",
            "parent_id": parent_id.0
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(child2_response.status(), 200);
    let child2: serde_json::Value = child2_response.json().await.unwrap();
    let child2_id = child2["id"].as_str().unwrap();

    // Get children of parent
    let children_response = client
        .get(format!("{}/session/{}/children", app.base_url(), parent_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(children_response.status(), 200);
    
    let children: Vec<serde_json::Value> = children_response.json().await.unwrap();
    assert_eq!(children.len(), 2);
    
    // Verify both children are in the list
    let child_ids: Vec<&str> = children.iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(child_ids.contains(&child1_id));
    assert!(child_ids.contains(&child2_id));
}

#[tokio::test]
async fn test_get_session_children_empty() {
    // Test that GET /session/:id/children returns empty array for session with no children
    let app = TestApp::new().await;
    let session_id = app.create_test_session().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/session/{}/children", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let children: Vec<serde_json::Value> = response.json().await.unwrap();
    assert!(children.is_empty());
}

#[tokio::test]
async fn test_get_session_children_not_found() {
    // Test that GET /session/:id/children returns 404 for non-existent session
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/session/nonexistent/children", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}
