//! Kubernetes runtime stub — executes AgentTask as a K8s Job.
//!
//! # Status: STUB
//!
//! This runtime declares `IsolationLevel::Container` capabilities (K8s pods
//! are containers) so the `RuntimeRegistry` can route tasks that require
//! container isolation here when running in a Kubernetes cluster.
//!
//! The actual K8s Job creation is **not yet implemented** — `spawn` returns
//! a `RuntimeError::NotImplemented` error.
//!
//! # Planned implementation
//!
//! 1. Serialize `AgentTask` to JSON.
//! 2. Create a `batch/v1 Job` manifest with the task embedded as an env var.
//! 3. Submit via the K8s API (using `kube` crate).
//! 4. Watch the Job until completion; stream logs as `AgentTaskResult`.

use async_trait::async_trait;
use rcode_core::{AgentTask, RuntimeDescriptor};
use std::sync::{Arc, Mutex};

use crate::{AgentRuntime, Result, RuntimeError, RuntimeHandle};

/// Kubernetes runtime — runs each `AgentTask` as a K8s `batch/v1` Job.
///
/// `available` is `false` by default — set it to `true` once the K8s API
/// server is reachable and credentials are configured.
#[derive(Debug)]
pub struct KubernetesRuntime {
    descriptor: RuntimeDescriptor,
    /// Active task IDs (stub — always empty).
    active_ids: Arc<Mutex<Vec<String>>>,
}

impl KubernetesRuntime {
    /// Create a stub instance targeting the given namespace.
    pub fn new(namespace: &str) -> Self {
        Self {
            descriptor: RuntimeDescriptor::kubernetes(namespace),
            active_ids: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Enable this runtime (marks it as available for the RuntimeRegistry).
    ///
    /// Call this after confirming the K8s API server is reachable.
    pub fn enable(&mut self) {
        self.descriptor.available = true;
    }
}

#[async_trait]
impl AgentRuntime for KubernetesRuntime {
    fn descriptor(&self) -> &RuntimeDescriptor {
        &self.descriptor
    }

    async fn spawn(&self, task: AgentTask) -> Result<RuntimeHandle> {
        Err(RuntimeError::NotImplemented(format!(
            "KubernetesRuntime is not yet implemented. \
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
        AgentDefinition, AgentTask, AgentTaskContext, IsolationLevel, ResourceRequirements,
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
    fn test_k8s_runtime_descriptor() {
        let rt = KubernetesRuntime::new("rcode-system");
        let d = rt.descriptor();
        assert_eq!(d.capabilities.isolation, IsolationLevel::Container);
        assert!(!d.available, "stub must start as unavailable");
    }

    #[test]
    fn test_k8s_runtime_enable() {
        let mut rt = KubernetesRuntime::new("default");
        assert!(!rt.descriptor().available);
        rt.enable();
        assert!(rt.descriptor().available);
    }

    #[tokio::test]
    async fn test_k8s_spawn_returns_not_implemented() {
        let rt = KubernetesRuntime::new("default");
        let result = rt.spawn(make_task()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RuntimeError::NotImplemented(_)));
    }

    /// The RuntimeRegistry must NOT select a disabled K8s runtime,
    /// and must fall back to InProcessRuntime.
    #[tokio::test]
    async fn test_registry_skips_unavailable_k8s() {
        use crate::{InProcessRuntime, RuntimeRegistry};
        use std::sync::Arc;

        let mut registry = RuntimeRegistry::new();
        registry.register(Arc::new(KubernetesRuntime::new("default")));
        registry.register(Arc::new(InProcessRuntime::new()));

        let selected = registry.select(&ResourceRequirements::default());
        assert!(selected.is_some(), "must find a runtime");
        assert_eq!(
            selected.unwrap().descriptor().id,
            "in-process",
            "must fall back to in-process when k8s is unavailable"
        );
    }
}
