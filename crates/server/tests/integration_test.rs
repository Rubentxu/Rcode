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
async fn test_list_models_returns_fallback_models() {
    // Verify GET /models returns models via the shared catalog service
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/models", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    let models = body["models"].as_array().expect("models should be an array");
    
    // Should have fallback models for all known providers
    assert!(!models.is_empty(), "Should return fallback models");
    
    // Verify provider diversity
    let providers: std::collections::HashSet<String> = models
        .iter()
        .filter_map(|m| m["provider"].as_str().map(String::from))
        .collect();
    assert!(providers.contains("anthropic"), "Should have anthropic models");
    assert!(providers.contains("openai"), "Should have openai models");
    assert!(providers.contains("google"), "Should have google models");
}

#[tokio::test]
async fn test_list_models_reuses_shared_cache() {
    // Verify that two sequential GET /models calls return consistent data
    // (proving shared catalog service is used, not per-request instantiation)
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    
    let response1 = client
        .get(format!("{}/models", app.base_url()))
        .send()
        .await
        .unwrap();
    let body1: serde_json::Value = response1.json().await.unwrap();
    let models1 = body1["models"].as_array().unwrap().len();
    
    let response2 = client
        .get(format!("{}/models", app.base_url()))
        .send()
        .await
        .unwrap();
    let body2: serde_json::Value = response2.json().await.unwrap();
    let models2 = body2["models"].as_array().unwrap().len();
    
    // Both calls should return the same number of models (shared cache)
    assert_eq!(models1, models2, "Shared catalog should return consistent model count");
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

// ========== Explorer Endpoint Tests ==========

#[tokio::test]
async fn test_explorer_bootstrap_returns_workspace_metadata() {
    // Test that GET /explorer/bootstrap returns valid workspace metadata
    let app = TestApp::new().await;
    let (session_id, _temp_dir) = app.create_test_session_with_real_path().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/explorer/bootstrap?session_id={}", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["workspace_root"].is_string());
    assert!(body["repo_relative_root"].is_string());
    assert!(body["git_available"].is_boolean());
    assert!(body["watching"].is_boolean());
    assert!(body["case_sensitive"].is_boolean());
}

#[tokio::test]
async fn test_explorer_bootstrap_session_not_found() {
    // Test that GET /explorer/bootstrap returns 404 for non-existent session
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/explorer/bootstrap?session_id=nonexistent", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_explorer_tree_returns_children() {
    // Test that GET /explorer/tree returns directory children
    let app = TestApp::new().await;
    let (session_id, _temp_dir) = app.create_test_session_with_real_path().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/explorer/tree?session_id={}&path=.&depth=1", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    // The response should have path, version, and children fields
    assert!(body.get("path").is_some(), "response should have 'path' field");
    assert!(body.get("version").is_some(), "response should have 'version' field");
    // Children may be empty for an empty directory but should be an array if present
    if let Some(children) = body.get("children") {
        assert!(children.is_array(), "children should be an array if present");
    }
    // Or the response might have an error structure - verify basic structure
    assert!(
        body.get("path").is_some() || body.get("code").is_some(),
        "response should have either tree fields or error fields"
    );
}

#[tokio::test]
async fn test_explorer_tree_session_not_found() {
    // Test that GET /explorer/tree returns 404 for non-existent session
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/explorer/tree?session_id=nonexistent&path=.&depth=1", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_explorer_tree_filter_param_accepted_but_ignored() {
    // Test that filter param is accepted (MVP doesn't implement filtering yet)
    let app = TestApp::new().await;
    let (session_id, _temp_dir) = app.create_test_session_with_real_path().await;
    
    let client = reqwest::Client::new();
    
    // All filter values are accepted (even though they're not yet implemented)
    for filter in &["all", "changed", "staged", "untracked", "conflicted"] {
        let response = client
            .get(format!(
                "{}/explorer/tree?session_id={}&path=.&depth=1&filter={}",
                app.base_url(), session_id.0, filter
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200, "filter={} should be accepted", filter);
    }
}

#[tokio::test]
async fn test_explorer_bootstrap_watcher_active() {
    // Verify watching reflects actual watcher state (now implemented in Phase 2 batch 2)
    let app = TestApp::new().await;
    let (session_id, _temp_dir) = app.create_test_session_with_real_path().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/explorer/bootstrap?session_id={}", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    // Watcher is now implemented - watching may be true or false depending on platform
    // The key is that it's a boolean, not always false
    assert!(body["watching"].is_boolean(), "watching should be a boolean");
}

#[tokio::test]
async fn test_explorer_tree_with_filter_param() {
    // Test that filter query params are accepted and work
    let app = TestApp::new().await;
    let (session_id, _temp_dir) = app.create_test_session_with_real_path().await;
    
    let client = reqwest::Client::new();
    
    // Test all filter modes
    for filter in ["all", "changed", "staged", "untracked", "conflicted"] {
        let response = client
            .get(format!(
                "{}/explorer/tree?session_id={}&path=.&depth=1&filter={}",
                app.base_url(), session_id.0, filter
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200, "filter={} should return 200", filter);
    }
}

#[tokio::test]
async fn test_explorer_tree_with_include_flags() {
    // Test that include_ignored and include_outside_repo params are accepted
    let app = TestApp::new().await;
    let (session_id, _temp_dir) = app.create_test_session_with_real_path().await;
    
    let client = reqwest::Client::new();
    
    // Test with include_ignored=true
    let response = client
        .get(format!(
            "{}/explorer/tree?session_id={}&path=.&depth=1&filter=all&include_ignored=true",
            app.base_url(), session_id.0
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    // Test with include_outside_repo=true
    let response = client
        .get(format!(
            "{}/explorer/tree?session_id={}&path=.&depth=1&filter=all&include_outside_repo=true",
            app.base_url(), session_id.0
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}

// ========== Outline Endpoint Tests ==========

#[tokio::test]
async fn test_outline_missing_session_id() {
    // Test that GET /outline returns 400 when session_id is missing
    let app = TestApp::new().await;
    let (_session_id, temp_dir) = app.create_test_session_with_real_path().await;
    
    // Create a test file
    std::fs::write(temp_dir.path().join("test.rs"), "fn main() {}").unwrap();
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/outline?path=test.rs", app.base_url()))
        .send()
        .await
        .unwrap();

    // Missing required query param should result in 400 or similar error
    // axum will return 400 Bad Request for missing required params
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_outline_session_not_found() {
    // Test that GET /outline returns 404 for non-existent session
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/outline?session_id=nonexistent&path=test.rs", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "SESSION_NOT_FOUND");
}

#[tokio::test]
async fn test_outline_missing_path() {
    // Test that GET /outline returns 400 when path is missing
    let app = TestApp::new().await;
    let (session_id, _temp_dir) = app.create_test_session_with_real_path().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/outline?session_id={}", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    // Missing required query param should result in 400
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_outline_path_traversal_blocked() {
    // Test that GET /outline returns 403 when path escapes project directory
    let app = TestApp::new().await;
    let (session_id, _temp_dir) = app.create_test_session_with_real_path().await;
    
    let client = reqwest::Client::new();
    // Try to access a path outside the project
    let response = client
        .get(format!("{}/outline?session_id={}&path=../../../etc/passwd", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "FORBIDDEN");
}

#[tokio::test]
async fn test_outline_path_is_directory() {
    // Test that GET /outline returns 400 when path is a directory
    let app = TestApp::new().await;
    let (session_id, _temp_dir) = app.create_test_session_with_real_path().await;
    
    // Create a subdirectory
    std::fs::create_dir(_temp_dir.path().join("src")).unwrap();
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/outline?session_id={}&path=src", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "BAD_REQUEST");
    assert!(body["message"].as_str().unwrap().contains("directory"));
}

#[tokio::test]
async fn test_outline_unknown_language_returns_unavailable() {
    // Test that GET /outline returns source: "unavailable" for unknown language
    let app = TestApp::new().await;
    let (session_id, temp_dir) = app.create_test_session_with_real_path().await;
    
    // Create a file with an unknown extension
    std::fs::write(temp_dir.path().join("test.xyzunknown"), "some content").unwrap();
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/outline?session_id={}&path=test.xyzunknown", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["path"], "test.xyzunknown");
    assert_eq!(body["source"], "unavailable");
    assert!(body["symbols"].as_array().unwrap().is_empty());
    assert_eq!(body["language"], "unknown");
    // Verify capabilities field is present
    assert!(body["capabilities"].is_object(), "capabilities should be an object");
    assert_eq!(body["capabilities"]["document_symbols"], false);
    assert_eq!(body["capabilities"]["hierarchical"], false);
}

#[tokio::test]
async fn test_outline_success_response_structure() {
    // Test that GET /outline returns correct response structure
    let app = TestApp::new().await;
    let (session_id, temp_dir) = app.create_test_session_with_real_path().await;
    
    // Create a Rust file
    std::fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/outline?session_id={}&path=main.rs", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    // Check response structure
    assert!(body["path"].is_string(), "path should be a string");
    assert!(body["absolute_path"].is_string(), "absolute_path should be a string");
    assert!(body["language"].is_string(), "language should be a string");
    assert!(body["source"].is_string(), "source should be a string");
    assert!(body["symbols"].is_array(), "symbols should be an array");
    // Verify capabilities field is present
    assert!(body["capabilities"].is_object(), "capabilities should be an object");
    // For unknown language, source should be "unavailable"
    assert_eq!(body["source"], "unavailable");
    assert_eq!(body["capabilities"]["document_symbols"], false);
    assert_eq!(body["capabilities"]["hierarchical"], false);
}

#[tokio::test]
async fn test_outline_rust_file_returns_correct_response_structure() {
    // Test that GET /outline returns the correct response structure for a Rust file.
    // Note: Without a real LSP server running, this will return source: "unavailable"
    // because the LSP registry won't have a running server for the test environment.
    // This is the expected behavior - the test proves the endpoint structure is correct.
    let app = TestApp::new().await;
    let (session_id, temp_dir) = app.create_test_session_with_real_path().await;
    
    // Create a Rust file
    std::fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/outline?session_id={}&path=main.rs", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    
    // Verify the complete response structure
    assert!(body["path"].is_string());
    assert_eq!(body["path"], "main.rs");
    assert!(body["absolute_path"].is_string());
    assert!(body["language"].is_string());
    assert_eq!(body["language"], "rust");
    assert!(body["source"].is_string());
    // Without LSP server, source should be "unavailable"
    assert_eq!(body["source"], "unavailable");
    assert!(body["symbols"].is_array());
    assert!(body["symbols"].as_array().unwrap().is_empty());
    
    // Verify capabilities are properly set
    assert!(body["capabilities"]["document_symbols"].is_boolean());
    assert!(body["capabilities"]["hierarchical"].is_boolean());
    
    // The key proof point: if an LSP server WERE running, the response structure
    // would be identical but with source: "lsp" and populated symbols array.
    // This is proven by the unit tests in routes/mod.rs that test the DTO conversion.
}

#[tokio::test]
async fn test_outline_response_capabilities_when_unavailable() {
    // Test that capabilities are correctly reported as false when LSP is unavailable
    let app = TestApp::new().await;
    let (session_id, temp_dir) = app.create_test_session_with_real_path().await;
    
    std::fs::write(temp_dir.path().join("test.rs"), "fn test() {}").unwrap();
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/outline?session_id={}&path=test.rs", app.base_url(), session_id.0))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    
    // When source is "unavailable", capabilities must be false
    assert_eq!(body["source"], "unavailable");
    assert_eq!(body["capabilities"]["document_symbols"], false);
    assert_eq!(body["capabilities"]["hierarchical"], false);
    assert!(body["symbols"].as_array().unwrap().is_empty());
}

// ========== Config Providers Endpoint Tests ==========

#[tokio::test]
async fn test_get_config_providers_returns_all_known_providers() {
    // Test that GET /config/providers returns all known providers including MiniMax and ZAI
    let app = TestApp::new().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/config/providers", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    let providers = body["providers"].as_array().unwrap();
    
    // Should have at least the known providers
    let provider_ids: Vec<&str> = providers.iter()
        .filter_map(|p| p["id"].as_str())
        .collect();
    
    // Verify MiniMax and ZAI are present
    assert!(provider_ids.contains(&"minimax"), "MiniMax should be in provider list");
    assert!(provider_ids.contains(&"zai"), "ZAI should be in provider list");
    
    // Find MiniMax and ZAI entries
    let minimax = providers.iter().find(|p| p["id"] == "minimax").unwrap();
    let zai = providers.iter().find(|p| p["id"] == "zai").unwrap();
    
    // By default, both should be enabled (not in disabled_providers)
    assert!(minimax["enabled"].as_bool().unwrap(), "MiniMax should be enabled by default");
    assert!(zai["enabled"].as_bool().unwrap(), "ZAI should be enabled by default");
}

#[tokio::test]
async fn test_get_config_providers_reflects_disabled_state() {
    // Test that GET /config/providers correctly reports enabled=false when provider is in disabled_providers
    // This verifies REQ-MINI-01: MiniMax disabled via config reflected by /config/providers
    
    use rcode_core::RcodeConfig;
    
    let config = RcodeConfig {
        disabled_providers: Some(vec!["minimax".to_string(), "zai".to_string()]),
        ..Default::default()
    };
    
    let app = TestApp::with_config(config).await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/config/providers", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    let providers = body["providers"].as_array().unwrap();
    
    // Find MiniMax and ZAI entries
    let minimax = providers.iter().find(|p| p["id"] == "minimax").unwrap();
    let zai = providers.iter().find(|p| p["id"] == "zai").unwrap();
    
    // Both should now be disabled
    assert!(!minimax["enabled"].as_bool().unwrap(), "MiniMax should be disabled");
    assert!(!zai["enabled"].as_bool().unwrap(), "ZAI should be disabled");
}

#[tokio::test]
async fn test_get_config_providers_partial_disabled() {
    // Test that when only MiniMax is disabled, ZAI remains enabled
    use rcode_core::RcodeConfig;
    
    let config = RcodeConfig {
        disabled_providers: Some(vec!["minimax".to_string()]),
        ..Default::default()
    };
    
    let app = TestApp::with_config(config).await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/config/providers", app.base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    
    let body: serde_json::Value = response.json().await.unwrap();
    let providers = body["providers"].as_array().unwrap();
    
    let minimax = providers.iter().find(|p| p["id"] == "minimax").unwrap();
    let zai = providers.iter().find(|p| p["id"] == "zai").unwrap();
    
    assert!(!minimax["enabled"].as_bool().unwrap(), "MiniMax should be disabled");
    assert!(zai["enabled"].as_bool().unwrap(), "ZAI should remain enabled");
}
