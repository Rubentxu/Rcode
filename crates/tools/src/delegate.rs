//! DelegateTool — delegates work to a background worker agent.
//!
//! Unlike `TaskTool` (synchronous, blocks until done), `DelegateTool`
//! spawns the work asynchronously via `SubagentRunner`, stores the
//! result in a shared `DelegationStore`, and returns a delegation ID
//! immediately. The caller can poll with `DelegationReadTool`.
//!
//! # Example flow
//!
//! ```text
//! delegate(prompt, agent) → id
//!   ↓ (tokio::spawn)
//!   SubagentRunner::run_subagent(agent_id, prompt, …)
//!   ↓ (completes)
//!   store[id].status = Completed | Failed
//!
//! delegation_read(id) → result
//! ```

use std::sync::Arc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use async_trait::async_trait;
use tokio::sync::RwLock;

use rcode_core::{
    Tool, ToolContext, ToolResult, SubagentRunner,
    error::{Result, RCodeError},
};

// ─── Status & Records ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DelegationStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct DelegationRecord {
    pub id: String,
    pub status: DelegationStatus,
    pub agent_type: String,
    pub prompt: String,
    pub result: Option<ToolResult>,
    pub error: Option<String>,
    pub created_at: u64,
}

/// Shared delegation store passed to both `DelegateTool` and `DelegationReadTool`.
pub type DelegationStore = Arc<RwLock<HashMap<String, DelegationRecord>>>;

static DELEGATION_COUNTER: AtomicU64 = AtomicU64::new(0);

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── DelegateTool ─────────────────────────────────────────────────────────────

/// Delegate a task to a worker agent and return a delegation ID immediately.
///
/// If a `SubagentRunner` is wired in, the task is spawned immediately via
/// `tokio::spawn` and the store is updated on completion.
///
/// Without a runner the record stays `Pending` (legacy/test mode).
pub struct DelegateTool {
    store: DelegationStore,
    runner: Option<Arc<dyn SubagentRunner>>,
}

impl DelegateTool {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            runner: None,
        }
    }

    /// Wire in a real `SubagentRunner` so delegations actually execute.
    pub fn with_runner(runner: Arc<dyn SubagentRunner>) -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            runner: Some(runner),
        }
    }

    /// Use an existing store (for sharing with `DelegationReadTool`).
    pub fn with_store(store: DelegationStore) -> Self {
        Self { store, runner: None }
    }

    /// Use an existing store *and* a runner.
    pub fn with_store_and_runner(store: DelegationStore, runner: Arc<dyn SubagentRunner>) -> Self {
        Self { store, runner: Some(runner) }
    }

    pub fn store(&self) -> DelegationStore {
        Arc::clone(&self.store)
    }
}

#[derive(Debug, serde::Deserialize)]
struct DelegateParams {
    prompt: String,
    agent: String,
}

#[derive(Debug, serde::Deserialize)]
struct ReadParams {
    id: String,
}

#[async_trait]
impl Tool for DelegateTool {
    fn id(&self) -> &str { "delegate" }
    fn name(&self) -> &str { "Delegate" }
    fn description(&self) -> &str {
        "Delegate work to a background agent. Returns a delegation ID immediately; \
         use delegation_read to retrieve the result when it completes."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Instructions for the worker agent"
                },
                "agent": {
                    "type": "string",
                    "description": "Worker agent identifier (e.g. explore, implement, test, verify, research)"
                }
            },
            "required": ["prompt", "agent"]
        })
    }

    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let params: DelegateParams = serde_json::from_value(args)
            .map_err(|e| RCodeError::Validation {
                field: "params".into(),
                message: e.to_string(),
            })?;

        let id = format!(
            "del_{}_{}",
            DELEGATION_COUNTER.fetch_add(1, Ordering::Relaxed),
            now_secs()
        );

        let record = DelegationRecord {
            id: id.clone(),
            status: DelegationStatus::Pending,
            agent_type: params.agent.clone(),
            prompt: params.prompt.clone(),
            result: None,
            error: None,
            created_at: now_secs(),
        };

        self.store.write().await.insert(id.clone(), record);

        // If a runner is wired in, spawn execution in the background.
        if let Some(runner) = &self.runner {
            let store = Arc::clone(&self.store);
            let runner = Arc::clone(runner);
            let id_inner = id.clone();
            let session_id = context.session_id.clone();
            let agent_id = params.agent.clone();
            let prompt = params.prompt.clone();

            tokio::spawn(async move {
                // Mark as Running
                {
                    let mut s = store.write().await;
                    if let Some(r) = s.get_mut(&id_inner) {
                        r.status = DelegationStatus::Running;
                    }
                }

                let outcome = runner
                    .run_subagent(&session_id, &agent_id, &prompt, &[])
                    .await;

                let mut s = store.write().await;
                if let Some(r) = s.get_mut(&id_inner) {
                    match outcome {
                        Ok(sub_result) => {
                            r.status = DelegationStatus::Completed;
                            r.result = Some(ToolResult {
                                title: format!("Worker {} completed", agent_id),
                                content: sub_result.response_text,
                                metadata: Some(serde_json::json!({
                                    "child_session_id": sub_result.child_session_id,
                                })),
                                attachments: vec![],
                            });
                        }
                        Err(e) => {
                            r.status = DelegationStatus::Failed;
                            r.error = Some(e.to_string());
                        }
                    }
                }
            });
        }

        Ok(ToolResult {
            title: "Delegation Created".to_string(),
            content: format!(
                "Delegation '{}' created for agent '{}'. {}",
                id,
                params.agent,
                if self.runner.is_some() {
                    "Running in background. Use delegation_read to retrieve results."
                } else {
                    "No runner configured — record stored but agent not spawned."
                }
            ),
            metadata: Some(serde_json::json!({
                "delegation_id": id,
                "status": "pending",
                "agent": params.agent,
            })),
            attachments: vec![],
        })
    }
}

// ─── DelegationReadTool ───────────────────────────────────────────────────────

/// Read the status/result of a background delegation by ID.
pub struct DelegationReadTool {
    store: DelegationStore,
}

impl DelegationReadTool {
    pub fn new(store: DelegationStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for DelegationReadTool {
    fn id(&self) -> &str { "delegation_read" }
    fn name(&self) -> &str { "Delegation Read" }
    fn description(&self) -> &str { "Read the status and result of a background delegation" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Delegation ID returned by delegate"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _context: &ToolContext) -> Result<ToolResult> {
        let params: ReadParams = serde_json::from_value(args)
            .map_err(|e| RCodeError::Validation {
                field: "params".into(),
                message: e.to_string(),
            })?;

        let store = self.store.read().await;
        match store.get(&params.id) {
            Some(record) => {
                let status = match record.status {
                    DelegationStatus::Pending => "pending",
                    DelegationStatus::Running => "running",
                    DelegationStatus::Completed => "completed",
                    DelegationStatus::Failed => "failed",
                };
                Ok(ToolResult {
                    title: format!("Delegation {}", status),
                    content: record
                        .result
                        .as_ref()
                        .map(|r| r.content.clone())
                        .or_else(|| record.error.clone().map(|e| format!("Error: {}", e)))
                        .unwrap_or_else(|| format!("Delegation {} — no result yet", status)),
                    metadata: Some(serde_json::json!({
                        "delegation_id": record.id,
                        "status": status,
                        "agent": record.agent_type,
                        "result": record.result,
                        "error": record.error,
                    })),
                    attachments: vec![],
                })
            }
            None => Err(RCodeError::Tool(format!(
                "Delegation '{}' not found",
                params.id
            ))),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{ToolContext, SubagentResult};
    use std::sync::atomic::Ordering;

    fn make_context() -> ToolContext {
        ToolContext {
            session_id: "test_session".into(),
            project_path: "/tmp".into(),
            cwd: "/tmp".into(),
            user_id: None,
            agent: "default".into(),
        }
    }

    // ── Mock runner ───────────────────────────────────────────────────────────

    struct MockRunner {
        response: String,
        fail: bool,
    }

    impl MockRunner {
        fn ok(msg: &str) -> Arc<Self> {
            Arc::new(Self { response: msg.to_string(), fail: false })
        }
        fn failing() -> Arc<Self> {
            Arc::new(Self { response: String::new(), fail: true })
        }
    }

    #[async_trait]
    impl SubagentRunner for MockRunner {
        async fn run_subagent(
            &self,
            _session: &str,
            _agent: &str,
            _prompt: &str,
            _tools: &[&str],
        ) -> std::result::Result<SubagentResult, Box<dyn std::error::Error + Send + Sync>> {
            if self.fail {
                Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "mock failure",
                )))
            } else {
                Ok(SubagentResult {
                    response_text: self.response.clone(),
                    child_session_id: "mock_child_session".to_string(),
                })
            }
        }
    }

    // ── Basic creation ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delegate_creates_pending_record() {
        let tool = DelegateTool::new();
        let args = serde_json::json!({"prompt": "Do something", "agent": "explore"});
        let result = tool.execute(args, &make_context()).await.unwrap();
        assert!(result.content.contains("del_"));
        let meta = result.metadata.unwrap();
        assert_eq!(meta["status"], "pending");
        assert_eq!(meta["agent"], "explore");
    }

    #[tokio::test]
    async fn test_delegate_unique_ids() {
        let tool = DelegateTool::new();
        let ctx = make_context();
        let a = serde_json::json!({"prompt": "t1", "agent": "a1"});
        let r1 = tool.execute(a.clone(), &ctx).await.unwrap();
        let r2 = tool.execute(a, &ctx).await.unwrap();
        let id1 = r1.metadata.unwrap()["delegation_id"].as_str().unwrap().to_string();
        let id2 = r2.metadata.unwrap()["delegation_id"].as_str().unwrap().to_string();
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn test_delegate_invalid_params() {
        let tool = DelegateTool::new();
        let result = tool.execute(serde_json::json!({"bad": "args"}), &make_context()).await;
        assert!(result.is_err());
    }

    // ── DelegationReadTool ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_read_not_found() {
        let store = Arc::new(RwLock::new(HashMap::new()));
        let read = DelegationReadTool::new(store);
        let result = read
            .execute(serde_json::json!({"id": "nonexistent"}), &make_context())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_read_pending_record() {
        let store = Arc::new(RwLock::new(HashMap::new()));
        let delegate = DelegateTool::with_store(Arc::clone(&store));
        let read = DelegationReadTool::new(Arc::clone(&store));

        let ctx = make_context();
        let args = serde_json::json!({"prompt": "test", "agent": "explore"});
        let created = delegate.execute(args, &ctx).await.unwrap();
        let id = created.metadata.unwrap()["delegation_id"]
            .as_str()
            .unwrap()
            .to_string();

        let read_result = read
            .execute(serde_json::json!({"id": id}), &ctx)
            .await
            .unwrap();
        assert!(read_result.title.contains("pending"));
        let meta = read_result.metadata.unwrap();
        assert_eq!(meta["status"], "pending");
        assert_eq!(meta["agent"], "explore");
    }

    // ── Async execution with runner ───────────────────────────────────────────

    #[tokio::test]
    async fn test_delegate_with_runner_completes() {
        let store = Arc::new(RwLock::new(HashMap::new()));
        let runner = MockRunner::ok("Worker output: analysis complete");
        let tool = DelegateTool::with_store_and_runner(Arc::clone(&store), runner);
        let read = DelegationReadTool::new(Arc::clone(&store));
        let ctx = make_context();

        let args = serde_json::json!({"prompt": "explore the codebase", "agent": "explore"});
        let created = tool.execute(args, &ctx).await.unwrap();
        let id = created.metadata.unwrap()["delegation_id"]
            .as_str()
            .unwrap()
            .to_string();

        // Give the background task time to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let result = read
            .execute(serde_json::json!({"id": id}), &ctx)
            .await
            .unwrap();
        let meta = result.metadata.unwrap();
        assert_eq!(meta["status"], "completed", "task should have completed");
        assert!(
            result.content.contains("Worker output"),
            "content should contain runner output"
        );
    }

    #[tokio::test]
    async fn test_delegate_with_failing_runner() {
        let store = Arc::new(RwLock::new(HashMap::new()));
        let runner = MockRunner::failing();
        let tool = DelegateTool::with_store_and_runner(Arc::clone(&store), runner);
        let read = DelegationReadTool::new(Arc::clone(&store));
        let ctx = make_context();

        let args = serde_json::json!({"prompt": "do work", "agent": "implement"});
        let created = tool.execute(args, &ctx).await.unwrap();
        let id = created.metadata.unwrap()["delegation_id"]
            .as_str()
            .unwrap()
            .to_string();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let result = read
            .execute(serde_json::json!({"id": id}), &ctx)
            .await
            .unwrap();
        let meta = result.metadata.unwrap();
        assert_eq!(meta["status"], "failed", "task should have failed");
        assert!(
            result.content.contains("mock failure"),
            "error message should be surfaced"
        );
    }

    // ── Status/clone ──────────────────────────────────────────────────────────

    #[test]
    fn test_status_debug() {
        assert!(format!("{:?}", DelegationStatus::Pending).contains("Pending"));
        assert!(format!("{:?}", DelegationStatus::Running).contains("Running"));
        assert!(format!("{:?}", DelegationStatus::Completed).contains("Completed"));
        assert!(format!("{:?}", DelegationStatus::Failed).contains("Failed"));
    }

    #[test]
    fn test_record_clone() {
        let record = DelegationRecord {
            id: "del_1".into(),
            status: DelegationStatus::Pending,
            agent_type: "explore".into(),
            prompt: "do work".into(),
            result: None,
            error: None,
            created_at: 12345,
        };
        let cloned = record.clone();
        assert_eq!(cloned.id, record.id);
        assert_eq!(cloned.agent_type, record.agent_type);
    }

    // ── Nivel 3: Concurrencia ────────────────────────────────────────────────

    /// Nivel 3-A: N delegaciones concurrentes comparten el mismo store y todas completan
    #[tokio::test]
    async fn test_concurrent_delegations_all_complete() {
        const N: usize = 20;
        let store = Arc::new(RwLock::new(HashMap::new()));
        let runner = MockRunner::ok("concurrent result");

        // Todas las instancias comparten store y runner
        let mut handles = Vec::with_capacity(N);
        let ctx = make_context();

        for i in 0..N {
            let s = Arc::clone(&store);
            let r = Arc::clone(&runner);
            let tool = DelegateTool::with_store_and_runner(s, r);
            let c = ctx.clone();
            handles.push(tokio::spawn(async move {
                let args = serde_json::json!({
                    "prompt": format!("task {}", i),
                    "agent": "explore",
                });
                tool.execute(args, &c).await.unwrap()
                    .metadata.unwrap()["delegation_id"]
                    .as_str().unwrap().to_string()
            }));
        }

        let ids: Vec<String> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.expect("task panicked"))
            .collect();

        // Todos los IDs deben ser únicos
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), N, "IDs should all be unique");

        // Esperar que todas completen (max 10 s)
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let s = store.read().await;
            let completed = ids.iter()
                .filter(|id| matches!(s.get(*id).map(|r| &r.status), Some(DelegationStatus::Completed)))
                .count();
            if completed == N { break; }
            if tokio::time::Instant::now() >= deadline {
                let s = store.read().await;
                let still_pending = ids.iter()
                    .filter(|id| !matches!(s.get(*id).map(|r| &r.status), Some(DelegationStatus::Completed)))
                    .count();
                panic!("{} delegations did not complete in time", still_pending);
            }
        }

        // Verificar que el store tiene exactamente N entradas
        assert_eq!(store.read().await.len(), N);
    }

    /// Nivel 3-B: escrituras concurrentes al store no corrompen los datos
    #[tokio::test]
    async fn test_concurrent_writes_store_integrity() {
        const N: usize = 50;
        let store = Arc::new(RwLock::new(HashMap::new()));
        let runner = MockRunner::ok("integrity check");
        let ctx = make_context();

        // Lanzar N delegaciones concurrentes
        let futs: Vec<_> = (0..N).map(|i| {
            let s = Arc::clone(&store);
            let r = Arc::clone(&runner);
            let c = ctx.clone();
            let tool = DelegateTool::with_store_and_runner(s, r);
            async move {
                let args = serde_json::json!({
                    "prompt": format!("integrity task {}", i),
                    "agent": "test",
                });
                tool.execute(args, &c).await.unwrap()
            }
        }).collect();

        let results = futures::future::join_all(futs).await;

        // Todos los execute deben haber tenido éxito
        assert_eq!(results.len(), N);

        // Esperar que todas completen
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let s = store.read().await;
            let done = s.values()
                .filter(|r| matches!(r.status, DelegationStatus::Completed | DelegationStatus::Failed))
                .count();
            if done == N { break; }
            if tokio::time::Instant::now() >= deadline {
                panic!("not all delegations completed: {}/{}", done, N);
            }
        }

        // Verificar integridad: ningún registro debe tener ID vacío o status inválido
        let s = store.read().await;
        assert_eq!(s.len(), N, "store should have exactly {} entries", N);
        for record in s.values() {
            assert!(!record.id.is_empty(), "record id must not be empty");
            assert!(
                matches!(record.status, DelegationStatus::Completed | DelegationStatus::Failed),
                "unexpected status {:?}", record.status
            );
        }
    }

    /// Nivel 3-C: leer mientras se escribe concurrentemente no causa deadlock ni panic
    #[tokio::test]
    async fn test_concurrent_read_write_no_deadlock() {
        const WRITERS: usize = 10;
        const READERS: usize = 10;

        let store = Arc::new(RwLock::new(HashMap::new()));
        let runner: Arc<dyn SubagentRunner> = MockRunner::ok("read-write result");
        let ctx = make_context();

        // Pre-poblar una entrada para que los readers tengan algo que leer
        let seed_tool = DelegateTool::with_store_and_runner(Arc::clone(&store), Arc::clone(&runner));
        let seed = seed_tool
            .execute(serde_json::json!({"prompt": "seed", "agent": "explore"}), &ctx)
            .await
            .unwrap();
        let seed_id = seed.metadata.unwrap()["delegation_id"]
            .as_str().unwrap().to_string();

        // Lanzar readers y writers en paralelo
        let read_store = Arc::clone(&store);
        let readers: Vec<_> = (0..READERS).map(|_| {
            let s = Arc::clone(&read_store);
            let id = seed_id.clone();
            tokio::spawn(async move {
                for _ in 0..10 {
                    let _ = s.read().await.get(&id).map(|r| r.status.clone());
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
            })
        }).collect();

        let writers: Vec<_> = (0..WRITERS).map(|i| {
            let s = Arc::clone(&store);
            let r = Arc::clone(&runner);
            let c = ctx.clone();
            tokio::spawn(async move {
                let tool = DelegateTool::with_store_and_runner(s, r);
                let args = serde_json::json!({
                    "prompt": format!("concurrent write {}", i),
                    "agent": "implement",
                });
                tool.execute(args, &c).await.unwrap()
            })
        }).collect();

        // Esperar todos sin timeout rígido — si hay deadlock el test falla por timeout del harness
        for r in readers { r.await.expect("reader panicked"); }
        for w in writers { w.await.expect("writer panicked"); }

        // El store debe tener seed + WRITERS entradas
        let count = store.read().await.len();
        assert_eq!(count, 1 + WRITERS, "expected {} entries, got {}", 1 + WRITERS, count);
    }
}
