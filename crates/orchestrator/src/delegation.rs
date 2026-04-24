//! Worker delegation for the ReflexiveOrchestrator
//!
//! This module handles delegating tasks to worker agents via the AgentRuntime.
//! Uses the new `AgentTask` / `AgentTaskResult` contract from `rcode-core`.

use std::collections::HashMap;
use std::sync::Arc;

use rcode_core::{
    AgentContext, AgentRegistry, AgentTask, AgentTaskContext, ExecutionConstraints,
    ResourceRequirements,
};
use rcode_runtime::AgentRuntime;
use serde::{Deserialize, Serialize};

/// Report from a worker agent execution.
/// This is the orchestrator-level view of a task result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerReport {
    /// The worker agent type that executed.
    pub agent_type: String,
    /// Summary of what the worker did.
    pub summary: String,
    /// Whether the orchestrator should continue after this report.
    pub should_continue: bool,
    /// Any artifacts produced by the worker.
    pub artifacts: Vec<WorkerArtifact>,
    /// Errors encountered (if any).
    pub errors: Vec<String>,
    /// Full text output from the agent.
    pub result_message: Option<String>,
    /// Runtime identifier that executed the task.
    pub runtime_id: String,
    /// Wall-clock duration in ms.
    pub duration_ms: Option<u64>,
}

/// An artifact produced by a worker agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerArtifact {
    pub artifact_type: String,
    pub path: String,
    pub description: String,
}

/// Delegate a task to a worker agent.
///
/// 1. Looks up the agent definition from the registry by `agent_type`.
/// 2. Builds an `AgentTask` (serializable).
/// 3. Spawns via the runtime.
/// 4. Awaits result and constructs a `WorkerReport`.
pub async fn delegate_to_worker(
    runtime: &Arc<dyn AgentRuntime>,
    registry: &Arc<AgentRegistry>,
    agent_type: &str,
    task: &str,
    ctx: &AgentContext,
) -> Result<WorkerReport, rcode_core::error::RCodeError> {
    tracing::info!(agent = %agent_type, "Delegating task to worker");

    // Look up agent definition
    let agent = match registry.get(agent_type) {
        Some(a) => a,
        None => {
            tracing::warn!(agent = %agent_type, "Worker agent not found in registry");
            return Ok(WorkerReport {
                agent_type: agent_type.to_string(),
                summary: format!(
                    "Worker '{}' not found. Available: {:?}",
                    agent_type,
                    registry.agent_ids()
                ),
                should_continue: false,
                artifacts: vec![],
                errors: vec![format!("Agent '{}' not found", agent_type)],
                result_message: None,
                runtime_id: runtime.descriptor().id.clone(),
                duration_ms: None,
            });
        }
    };

    let runtime_id = runtime.descriptor().id.clone();

    // Build AgentTaskContext from AgentContext
    let task_ctx = AgentTaskContext {
        session_id: format!("{}-{}", ctx.session_id, agent_type),
        project_path: ctx.project_path.clone(),
        cwd: ctx.cwd.clone(),
        messages: ctx.messages.clone(),
        user_id: ctx.user_id.clone(),
        model_id: ctx.model_id.clone(),
        metadata: HashMap::new(),
    };

    // Use the dynamic agent's definition — falls back to a minimal stub if not available
    let definition = match agent.definition() {
        Some(def) => def.clone(),
        None => {
            // Built-in agent without a definition: build a minimal one
            use rcode_core::agent_definition::{AgentDefinition, AgentMode, AgentPermissionConfig};
            AgentDefinition {
                identifier: agent_type.to_string(),
                name: agent.name().to_string(),
                description: agent.description().to_string(),
                when_to_use: String::new(),
                system_prompt: agent.system_prompt(),
                mode: AgentMode::Subagent,
                hidden: false,
                permission: AgentPermissionConfig::default(),
                tools: agent.supported_tools(),
                model: None,
                max_tokens: None,
                reasoning_effort: None,
            }
        }
    };

    // Determine resource requirements from agent definition
    // (future: parse from definition YAML frontmatter)
    let requirements = ResourceRequirements::lightweight();

    let agent_task = AgentTask {
        task_id: format!("{}-{}-{}", ctx.session_id, agent_type, uuid_v4()),
        definition,
        prompt: task.to_string(),
        context: task_ctx,
        requirements,
        constraints: ExecutionConstraints::default(),
    };

    // Spawn via runtime
    let handle = runtime
        .spawn(agent_task)
        .await
        .map_err(|e| rcode_core::error::RCodeError::Agent(format!("Spawn failed: {e}")))?;

    // Await result
    let task_result = handle
        .await_result()
        .await
        .map_err(|e| rcode_core::error::RCodeError::Agent(format!("Execution failed: {e}")))?;

    let content = extract_text_content(&task_result.message);
    tracing::debug!(agent = %agent_type, runtime = %runtime_id, "Worker completed");

    Ok(WorkerReport {
        agent_type: agent_type.to_string(),
        summary: format!(
            "Worker '{}' completed\nRuntime: {}\nOutput: {}",
            agent_type,
            runtime_id,
            content.chars().take(500).collect::<String>()
        ),
        should_continue: true,
        artifacts: vec![],
        errors: vec![],
        result_message: Some(content),
        runtime_id,
        duration_ms: task_result.usage.duration_ms,
    })
}

/// Delegate with retry support.
#[allow(dead_code)]
pub async fn delegate_with_retry(
    runtime: &Arc<dyn AgentRuntime>,
    registry: &Arc<AgentRegistry>,
    agent_type: &str,
    task: &str,
    ctx: &AgentContext,
    max_retries: u32,
) -> Result<WorkerReport, rcode_core::error::RCodeError> {
    let mut last_error = None;
    for attempt in 0..=max_retries {
        match delegate_to_worker(runtime, registry, agent_type, task, ctx).await {
            Ok(r) => return Ok(r),
            Err(e) => {
                tracing::warn!(attempt = attempt + 1, "Worker delegation attempt failed: {e}");
                last_error = Some(e);
                if attempt < max_retries {
                    let delay = std::time::Duration::from_millis(100 * (attempt + 1) as u64);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| {
        rcode_core::error::RCodeError::Agent(format!(
            "Worker delegation failed after {} attempts",
            max_retries + 1
        ))
    }))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn extract_text_content(message: &rcode_core::Message) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| {
            if let rcode_core::Part::Text { content } = part {
                Some(content.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Minimal UUID v4 via timestamp+random to avoid pulling uuid crate into orchestrator.
/// Production code should use uuid::Uuid::new_v4().to_string().
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{ts:x}")
}

// ─── Report parsing ───────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct ReportSummary {
    pub worker_type: String,
    pub action_taken: String,
    pub artifacts_count: usize,
    pub had_errors: bool,
    pub should_continue: bool,
}

#[allow(dead_code)]
pub fn parse_report(report: &WorkerReport) -> ReportSummary {
    ReportSummary {
        worker_type: report.agent_type.clone(),
        action_taken: extract_action(&report.summary),
        artifacts_count: report.artifacts.len(),
        had_errors: !report.errors.is_empty(),
        should_continue: report.should_continue,
    }
}

#[allow(dead_code)]
fn extract_action(summary: &str) -> String {
    if summary.contains("read") || summary.contains("analyze") {
        "read/analyze".to_string()
    } else if summary.contains("write") || summary.contains("edit") || summary.contains("implement") {
        "write/edit".to_string()
    } else if summary.contains("test") {
        "test".to_string()
    } else if summary.contains("verify") || summary.contains("validate") {
        "verify".to_string()
    } else {
        "unknown".to_string()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{AgentContext, AgentRegistry};
    use rcode_runtime::{AgentRuntime, InProcessRuntime};

    #[test]
    fn parse_report_summary_basic() {
        let report = WorkerReport {
            agent_type: "explore".to_string(),
            summary: "Worker explore analyzed code structure".to_string(),
            should_continue: true,
            artifacts: vec![],
            errors: vec![],
            result_message: None,
            runtime_id: "in-process".to_string(),
            duration_ms: None,
        };
        let summary = parse_report(&report);
        assert_eq!(summary.worker_type, "explore");
        assert!(summary.should_continue);
        assert!(!summary.had_errors);
    }

    #[tokio::test]
    async fn delegate_returns_not_found_when_agent_missing() {
        let runtime: Arc<dyn AgentRuntime> = Arc::new(InProcessRuntime::new());
        let registry = Arc::new(AgentRegistry::new());
        let ctx = AgentContext {
            session_id: "test-session".to_string(),
            project_path: std::path::PathBuf::from("/tmp"),
            cwd: std::path::PathBuf::from("/tmp"),
            user_id: None,
            model_id: "test".to_string(),
            messages: vec![],
        };

        let result = delegate_to_worker(&runtime, &registry, "explore", "Analyze this", &ctx)
            .await
            .unwrap();

        assert!(!result.errors.is_empty());
        assert!(!result.should_continue);
    }
}
