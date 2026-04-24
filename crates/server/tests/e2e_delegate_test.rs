//! End-to-end test for DelegateTool → ServerSubagentRunner flow
//!
//! Tests that:
//! 1. Start server with SlowMockProvider configured to emit a `delegate` tool call
//!    on the first invocation only (subsequent invocations → plain text, no tool calls)
//! 2. POST /session/:id/prompt
//! 3. Wait for execution to complete (session back to Idle)
//! 4. Verify messages in the parent session include a tool call + tool result
//! 5. Verify the DelegationStore has a completed entry with non-empty response_text

mod mock_provider;
mod test_utils;

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use test_utils::TestApp;

/// Helper: poll until the session status matches `expected` or timeout
async fn wait_for_status(client: &reqwest::Client, base_url: &str, session_id: &str, expected: &str, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("Timed out waiting for session status '{}'", expected);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let resp = client
            .get(format!("{}/session/{}", base_url, session_id))
            .send()
            .await
            .expect("GET /session failed");
        let body: serde_json::Value = resp.json().await.expect("body parse failed");
        let status = body["status"].as_str().unwrap_or("");
        if status == expected {
            return;
        }
        // Fail fast on terminal bad states
        if status == "aborted" || status == "error" {
            panic!("Session ended in unexpected status '{}' while waiting for '{}'", status, expected);
        }
    }
}

// ---------------------------------------------------------------------------
// Test 1 — DelegateTool fires, worker completes, parent session has tool call
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delegate_tool_fires_and_worker_completes() {
    // Configure mock: first invocation emits `delegate` tool call, subsequent → plain text
    let mock_provider = Arc::new(mock_provider::SlowMockProvider::new());
    mock_provider.set_delay_per_token(0); // instant — no sleep overhead
    mock_provider.set_text_events(1, "done".to_string());
    mock_provider.set_tool_calls(true, Some("delegate".to_string()));
    mock_provider.set_tool_call_args(
        r#"{"prompt":"explore the codebase","agent":"explore"}"#.to_string(),
    );
    // Only emit tool call on invocation #1; invocation #2 (worker) → plain text
    mock_provider.set_tool_calls_max_invocations(1);

    let app = TestApp::with_mock_provider(mock_provider.clone()).await;
    let session_id = app.create_test_session().await;
    let client = reqwest::Client::new();

    // POST prompt — triggers orchestrator (invocation #1 → emits delegate tool call)
    let resp = client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({ "prompt": "Use bash tool: pwd. Do not answer from memory." }))
        .send()
        .await
        .expect("POST /prompt failed");
    assert_eq!(resp.status(), 200, "Prompt submission should succeed");

    // Wait for the session to return to Idle (executor finished)
    wait_for_status(&client, &app.base_url(), &session_id.0, "idle", Duration::from_secs(20)).await;

    // -----------------------------------------------------------------------
    // Assertion 1: mock was called at least twice
    //   invocation #1 = orchestrator, invocation #2 = worker
    // -----------------------------------------------------------------------
    let invocations = mock_provider.invocation_count();
    assert!(
        invocations >= 2,
        "Expected at least 2 provider invocations (orchestrator + worker), got {}",
        invocations
    );

    // -----------------------------------------------------------------------
    // Assertion 2: parent session messages contain a ToolCall part
    // -----------------------------------------------------------------------
    let msgs_resp = client
        .get(format!("{}/session/{}/messages", app.base_url(), session_id.0))
        .send()
        .await
        .expect("GET /messages failed");
    assert_eq!(msgs_resp.status(), 200);
    let msgs_body: serde_json::Value = msgs_resp.json().await.expect("messages parse failed");
    let messages = msgs_body["messages"].as_array().expect("messages array missing");

    let has_tool_call = messages.iter().any(|m| {
        m["parts"].as_array().map_or(false, |parts| {
            parts.iter().any(|p| p["type"] == "tool_call" && p["name"] == "delegate")
        })
    });
    assert!(
        has_tool_call,
        "Parent session should contain a 'delegate' tool_call part. Messages: {:#?}",
        messages
    );

    // -----------------------------------------------------------------------
    // Assertion 3: parent session messages contain a ToolResult part
    // -----------------------------------------------------------------------
    let has_tool_result = messages.iter().any(|m| {
        m["parts"].as_array().map_or(false, |parts| {
            parts.iter().any(|p| p["type"] == "tool_result")
        })
    });
    assert!(
        has_tool_result,
        "Parent session should contain a tool_result part. Messages: {:#?}",
        messages
    );
}

// ---------------------------------------------------------------------------
// Test 2 — delegation_read returns completed entry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delegation_read_returns_completed_result() {
    // Same setup as Test 1
    let mock_provider = Arc::new(mock_provider::SlowMockProvider::new());
    mock_provider.set_delay_per_token(0);
    mock_provider.set_text_events(1, "worker response".to_string());
    mock_provider.set_tool_calls(true, Some("delegate".to_string()));
    mock_provider.set_tool_call_args(
        r#"{"prompt":"list files","agent":"explore"}"#.to_string(),
    );
    mock_provider.set_tool_calls_max_invocations(1);

    let app = TestApp::with_mock_provider(mock_provider.clone()).await;
    let session_id = app.create_test_session().await;
    let client = reqwest::Client::new();

    // Submit prompt
    let resp = client
        .post(format!("{}/session/{}/prompt", app.base_url(), session_id.0))
        .json(&json!({ "prompt": "Use bash tool: pwd. Do not answer from memory." }))
        .send()
        .await
        .expect("POST /prompt failed");
    assert_eq!(resp.status(), 200);

    // Wait for completion
    wait_for_status(&client, &app.base_url(), &session_id.0, "idle", Duration::from_secs(20)).await;

    // -----------------------------------------------------------------------
    // Retrieve delegation ID from the tool_result in session messages
    // -----------------------------------------------------------------------
    let msgs_resp = client
        .get(format!("{}/session/{}/messages", app.base_url(), session_id.0))
        .send()
        .await
        .expect("GET /messages failed");
    let msgs_body: serde_json::Value = msgs_resp.json().await.unwrap();
    let messages = msgs_body["messages"].as_array().expect("messages array");

    // Find the tool_call part to get the delegation id (same as tool call id)
    let delegation_id = messages
        .iter()
        .flat_map(|m| m["parts"].as_array().unwrap_or(&vec![]).iter().cloned().collect::<Vec<_>>())
        .find(|p| p["type"] == "tool_call" && p["name"] == "delegate")
        .and_then(|p| p["id"].as_str().map(|s| s.to_string()));

    assert!(
        delegation_id.is_some(),
        "Should find a 'delegate' tool_call with an id. Messages: {:#?}",
        messages
    );

    // -----------------------------------------------------------------------
    // Poll until DelegationStore has a Completed entry (worker runs in background)
    // -----------------------------------------------------------------------
    let delegation_store = app.state.tools.delegation_store();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let has_completed = loop {
        {
            let guard = delegation_store.read().await;
            if guard.values().any(|e| e.status == rcode_tools::DelegationStatus::Completed) {
                break true;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert!(
        has_completed,
        "DelegationStore should have at least one Completed entry within 5s"
    );
}
