//! Container runtime stub — executes AgentTask via an OCI container (Docker/Podman).
//!
//! # Status: STUB
//!
//! This runtime declares `IsolationLevel::Container` capabilities so the
//! `RuntimeRegistry` can route tasks that require container isolation here.
//! The actual container execution is **not yet implemented** — `spawn` returns
//! a `RuntimeError::NotImplemented` error.
//!
//! # Planned implementation
//!
//! 1. Serialize `AgentTask` to JSON.
//! 2. Pull / reuse image `rcode-agent:<version>`.
//! 3. `docker run --rm -e RCODE_TASK=<base64(json)> rcode-agent:<version>`
//! 4. Parse stdout as `AgentTaskResult`.

use async_trait::async_trait;
use rcode_core::{AgentTask, RuntimeDescriptor};
use std::sync::{Arc, Mutex};

use crate::{AgentRuntime, Result, RuntimeError, RuntimeHandle};

/// OCI container runtime (Docker / Podman).
///
/// `available` is `false` by default — set it to `true` once the Docker
/// daemon is confirmed reachable.
#[derive(Debug)]
pub struct ContainerRuntime {
    descriptor: RuntimeDescriptor,
    /// Active task IDs (stub — always empty).
    active_ids: Arc<Mutex<Vec<String>>>,
}

impl ContainerRuntime {
    /// Create a stub instance with the given image reference.
    pub fn new(image: &str) -> Self {
        Self {
            descriptor: RuntimeDescriptor::container(image),
            active_ids: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Enable this runtime (marks it as available for the RuntimeRegistry).
    ///
    /// Call this after confirming the container daemon is reachable.
    pub fn enable(&mut self) {
        self.descriptor.available = true;
    }
}

#[async_trait]
impl AgentRuntime for ContainerRuntime {
    fn descriptor(&self) -> &RuntimeDescriptor {
        &self.descriptor
    }

    async fn spawn(&self, task: AgentTask) -> Result<RuntimeHandle> {
        Err(RuntimeError::NotImplemented(format!(
            "ContainerRuntime is not yet implemented. \
             Task '{}' for agent '{}' cannot be executed. \
             Use InProcessRuntime or ProcessRuntime instead.",
            task.task_id, task.definition.identifier
        )))
    }

    async fn list_active(&self) -> Vec<String> {
        self.active_ids.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{
        AgentDefinition, AgentTask, AgentTaskContext, IsolationLevel,
    };

    fn make_task() -> AgentTask {
        let definition = AgentDefinition {
            identifier: "explore".to_string(),
            name: "Explore".to_string(),
            description: String::new(),
            when_to_use: String::new(),
            system_prompt: "test".to_string(),
            mode: rcode_core::agent_definition::AgentMode::Subagent,
            hidden: false,
            tools: vec![],
            model: None,
            permission: Default::default(),
            max_tokens: None,
            reasoning_effort: None,
        };
        let ctx = AgentTaskContext {
            session_id: "test-session".to_string(),
            project_path: std::path::PathBuf::from("/tmp"),
            cwd: std::path::PathBuf::from("/tmp"),
            messages: vec![],
            user_id: None,
            model_id: "test".to_string(),
            metadata: Default::default(),
        };
        AgentTask::new("task-1", definition, "test prompt", ctx)
    }

    #[test]
    fn test_container_runtime_descriptor() {
        let rt = ContainerRuntime::new("ghcr.io/rubentxu/rcode-agent:latest");
        let d = rt.descriptor();
        assert_eq!(d.capabilities.isolation, IsolationLevel::Container);
        assert!(!d.available, "stub must start as unavailable");
    }

    #[test]
    fn test_container_runtime_enable() {
        let mut rt = ContainerRuntime::new("ghcr.io/rubentxu/rcode-agent:latest");
        assert!(!rt.descriptor().available);
        rt.enable();
        assert!(rt.descriptor().available);
    }

    #[tokio::test]
    async fn test_container_spawn_returns_not_implemented() {
        let rt = ContainerRuntime::new("test-image:latest");
        let result = rt.spawn(make_task()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RuntimeError::NotImplemented(_)));
    }
}
