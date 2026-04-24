//! RCode Runtime Abstraction
//!
//! This crate provides runtime abstraction for executing agent workers.
//! The `AgentRuntime` trait defines the interface for spawning and managing
//! agent executions in different environments.
//!
//! # Implementations
//!
//! | Type | Isolation | Status |
//! |------|-----------|--------|
//! | `InProcessRuntime` | None (tokio::spawn) | ✅ Ready |
//! | `ProcessRuntime`   | Process (tokio::Command) | ✅ Ready |
//! | `ContainerRuntime` | Container (OCI)  | 🔲 Stub |
//! | `KubernetesRuntime`| Container (K8s)  | 🔲 Stub |
//! | `LambdaRuntime`    | Sandbox (FaaS)   | 🔲 Stub |
//!
//! # How to add a new backend
//!
//! 1. Create `src/my_runtime.rs` implementing `AgentRuntime`.
//! 2. Return an appropriate `RuntimeDescriptor` from `descriptor()`.
//! 3. Implement `can_satisfy()` by delegating to `descriptor().capabilities.can_satisfy(req)`.
//! 4. Register it in `RuntimeRegistry` if used through the registry.

mod error;
mod in_process;
mod registry;
mod container;
mod kubernetes;

#[cfg(unix)]
mod process;

pub use error::{RuntimeError, Result};
pub use in_process::InProcessRuntime;
pub use registry::RuntimeRegistry;
pub use container::ContainerRuntime;
pub use kubernetes::KubernetesRuntime;

// Re-export runtime types from core so callers only need rcode_runtime
pub use rcode_core::{
    AgentTask, AgentTaskContext, AgentTaskResult, ExecutionConstraints, FilesystemAccess,
    IsolationLevel, NetworkingMode, ResourceLimits, ResourceRequirements, RuntimeCapabilities,
    RuntimeDescriptor, TaskPriority, TaskStatus,
};

#[cfg(unix)]
pub use process::ProcessRuntime;

use async_trait::async_trait;
use tokio::sync::{oneshot, watch};

// ─── RuntimeHandle ────────────────────────────────────────────────────────────

/// Handle returned by `AgentRuntime::spawn`.
///
/// Callers use this to:
/// - `await_result()` — block until completion and receive `AgentTaskResult`
/// - `cancel()` — request cooperative cancellation
/// - `status()` / `subscribe_status()` — poll or watch the task lifecycle
pub struct RuntimeHandle {
    pub task_id: String,
    status_rx: watch::Receiver<TaskStatus>,
    cancel_tx: Option<oneshot::Sender<()>>,
    result: tokio::task::JoinHandle<Result<AgentTaskResult>>,
}

impl RuntimeHandle {
    /// Create a new handle. `cancel_tx` is optional — runtimes that do not
    /// support cancellation may pass `None`.
    pub fn new(
        task_id: String,
        status_rx: watch::Receiver<TaskStatus>,
        cancel_tx: Option<oneshot::Sender<()>>,
        result: tokio::task::JoinHandle<Result<AgentTaskResult>>,
    ) -> Self {
        Self {
            task_id,
            status_rx,
            cancel_tx,
            result,
        }
    }

    /// Await completion and return the full task result.
    pub async fn await_result(self) -> Result<AgentTaskResult> {
        self.result.await.map_err(|e| {
            RuntimeError::ExecutionFailed(format!("Task join error: {e}"))
        })?
    }

    /// Request cancellation. Returns `Ok(())` if the signal was sent.
    /// The task may not honour it immediately.
    pub fn cancel(mut self) -> Result<()> {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
        Ok(())
    }

    /// Snapshot of the current task status.
    pub fn status(&self) -> TaskStatus {
        self.status_rx.borrow().clone()
    }

    /// Subscribe to status changes.
    pub fn subscribe_status(&self) -> watch::Receiver<TaskStatus> {
        self.status_rx.clone()
    }
}

impl std::fmt::Debug for RuntimeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeHandle")
            .field("task_id", &self.task_id)
            .field("status", &self.status())
            .finish()
    }
}

// ─── AgentRuntime trait ───────────────────────────────────────────────────────

/// Core trait for runtime abstraction.
///
/// Implementations execute `AgentTask`s in different environments:
/// in-process, subprocess, container, K8s pod, FaaS function, etc.
///
/// The key design constraint: **`AgentTask` is fully serializable**, so any
/// runtime can forward it over the wire without holding a Rust object.
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    /// Returns the stable descriptor for this runtime instance.
    fn descriptor(&self) -> &RuntimeDescriptor;

    /// Returns `true` if this runtime can satisfy the given resource requirements.
    fn can_satisfy(&self, requirements: &ResourceRequirements) -> bool {
        self.descriptor().capabilities.can_satisfy(requirements)
    }

    /// Spawns an `AgentTask` and returns a handle to track it.
    async fn spawn(&self, task: AgentTask) -> Result<RuntimeHandle>;

    /// List currently active (non-terminal) task IDs.
    async fn list_active(&self) -> Vec<String>;

    /// Graceful shutdown — wait for running tasks to finish.
    async fn shutdown(&self) -> Result<()>;
}
