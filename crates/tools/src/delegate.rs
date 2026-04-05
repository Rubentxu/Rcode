use std::sync::Arc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use async_trait::async_trait;
use tokio::sync::RwLock;

use rcode_core::{Tool, ToolContext, ToolResult, error::{Result, RCodeError}};

#[derive(Debug, Clone)]
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

static DELEGATION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct DelegateTool {
    store: Arc<RwLock<HashMap<String, DelegationRecord>>>,
}

impl DelegateTool {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_store(store: Arc<RwLock<HashMap<String, DelegationRecord>>>) -> Self {
        Self { store }
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
        "Delegate work to a background agent. Returns delegation ID for later retrieval."
    }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Instructions for the background agent"
                },
                "agent": {
                    "type": "string", 
                    "description": "Agent type to delegate to"
                }
            },
            "required": ["prompt", "agent"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, _context: &ToolContext) -> Result<ToolResult> {
        let params: DelegateParams = serde_json::from_value(args)
            .map_err(|e| RCodeError::Validation { field: "params".into(), message: e.to_string() })?;
        
        let id = format!("del_{}_{}", 
            DELEGATION_COUNTER.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
        
        let record = DelegationRecord {
            id: id.clone(),
            status: DelegationStatus::Pending,
            agent_type: params.agent.clone(),
            prompt: params.prompt.clone(),
            result: None,
            error: None,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        
        self.store.write().await.insert(id.clone(), record);
        
        Ok(ToolResult {
            title: "Delegation Created".to_string(),
            content: format!("Delegation '{}' created. Running in background. Use delegation_read to retrieve results.", id),
            metadata: Some(serde_json::json!({
                "delegation_id": id,
                "status": "pending"
            })),
            attachments: vec![],
        })
    }
}

pub struct DelegationReadTool {
    store: Arc<RwLock<HashMap<String, DelegationRecord>>>,
}

impl DelegationReadTool {
    pub fn new(store: Arc<RwLock<HashMap<String, DelegationRecord>>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for DelegationReadTool {
    fn id(&self) -> &str { "delegation_read" }
    fn name(&self) -> &str { "Delegation Read" }
    fn description(&self) -> &str { "Read result of a background delegation" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Delegation ID from delegate"
                }
            },
            "required": ["id"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, _context: &ToolContext) -> Result<ToolResult> {
        let params: ReadParams = serde_json::from_value(args)
            .map_err(|e| RCodeError::Validation { field: "params".into(), message: e.to_string() })?;
        
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
                    content: record.result.as_ref().map(|r| r.content.clone())
                        .unwrap_or_else(|| format!("Delegation {} - no result yet", status)),
                    metadata: Some(serde_json::json!({
                        "delegation_id": record.id,
                        "status": status,
                        "result": record.result,
                        "error": record.error,
                    })),
                    attachments: vec![],
                })
            }
            None => Err(RCodeError::Tool(format!("Delegation '{}' not found", params.id))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;
    use tokio::sync::RwLock as TokioRwLock;
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

    #[tokio::test]
    async fn test_delegate_creates_record() {
        let tool = DelegateTool::new();
        let args = serde_json::json!({
            "prompt": "Do something",
            "agent": "researcher"
        });
        let ctx = make_context();
        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(result.content.contains("Delegation"));
        assert!(result.metadata.is_some());
        let meta = result.metadata.unwrap();
        assert_eq!(meta["status"], "pending");
    }

    #[tokio::test]
    async fn test_delegate_unique_ids() {
        let tool = DelegateTool::new();
        let ctx = make_context();
        let args = serde_json::json!({"prompt": "t1", "agent": "a1"});
        let r1 = tool.execute(args.clone(), &ctx).await.unwrap();
        let r2 = tool.execute(args, &ctx).await.unwrap();
        let id1 = r1.metadata.unwrap()["delegation_id"].as_str().unwrap().to_string();
        let id2 = r2.metadata.unwrap()["delegation_id"].as_str().unwrap().to_string();
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn test_delegate_invalid_params() {
        let tool = DelegateTool::new();
        let ctx = make_context();
        let result = tool.execute(serde_json::json!({"bad": "args"}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delegation_read_found() {
        let store = Arc::new(TokioRwLock::new(HashMap::new()));
        let delegate_tool = DelegateTool::with_store(store.clone());
        let read_tool = DelegationReadTool::new(store);

        let ctx = make_context();
        let args = serde_json::json!({"prompt": "test", "agent": "test"});
        let create_result = delegate_tool.execute(args, &ctx).await.unwrap();
        let metadata = create_result.metadata.unwrap();
        let delegation_id = metadata["delegation_id"].as_str().unwrap();

        let read_args = serde_json::json!({"id": delegation_id});
        let read_result = read_tool.execute(read_args, &ctx).await.unwrap();
        assert!(read_result.content.contains("pending"));
    }

    #[tokio::test]
    async fn test_delegation_read_not_found() {
        let store = Arc::new(TokioRwLock::new(HashMap::new()));
        let read_tool = DelegationReadTool::new(store);
        let ctx = make_context();
        let result = read_tool.execute(serde_json::json!({"id": "nonexistent"}), &ctx).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not found"));
    }

    #[tokio::test]
    async fn test_delegation_status_pending() {
        let store = Arc::new(TokioRwLock::new(HashMap::new()));
        let delegate_tool = DelegateTool::with_store(store.clone());
        let ctx = make_context();

        DELEGATION_COUNTER.store(0, Ordering::Relaxed);
        delegate_tool.execute(serde_json::json!({"prompt": "t", "agent": "a"}), &ctx).await.unwrap();

        let guard = store.read().await;
        let record: &DelegationRecord = guard.values().next().unwrap();
        assert!(matches!(record.status, DelegationStatus::Pending));
        assert_eq!(record.agent_type, "a");
        assert_eq!(record.prompt, "t");
        assert!(record.result.is_none());
        assert!(record.error.is_none());
    }

    #[test]
    fn test_delegation_status_debug() {
        assert!(format!("{:?}", DelegationStatus::Pending).contains("Pending"));
        assert!(format!("{:?}", DelegationStatus::Running).contains("Running"));
        assert!(format!("{:?}", DelegationStatus::Completed).contains("Completed"));
        assert!(format!("{:?}", DelegationStatus::Failed).contains("Failed"));
    }

    #[test]
    fn test_delegation_record_clone() {
        let record = DelegationRecord {
            id: "del_1".into(),
            status: DelegationStatus::Pending,
            agent_type: "researcher".into(),
            prompt: "do work".into(),
            result: None,
            error: None,
            created_at: 12345,
        };
        let cloned = record.clone();
        assert_eq!(cloned.id, record.id);
        assert_eq!(cloned.agent_type, record.agent_type);
    }
}
