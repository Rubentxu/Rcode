//! Tool trait and registry

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{error::Result, message::Part};

#[async_trait]
pub trait Tool: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    
    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult>;
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub session_id: String,
    pub project_path: std::path::PathBuf,
    pub cwd: std::path::PathBuf,
    pub user_id: Option<String>,
    /// The agent ID that is executing the tool
    pub agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub title: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attachments: Vec<ToolAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAttachment {
    pub path: String,
    #[serde(rename = "type")]
    pub mime_type: String,
}

pub trait ToolRegistry: Send + Sync {
    fn register(&self, tool: Arc<dyn Tool>);
    fn get(&self, id: &str) -> Option<Arc<dyn Tool>>;
    fn list(&self) -> Vec<ToolInfo>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

impl Part {
    pub fn tool_result(tool_call_id: String, result: ToolResult) -> Self {
        Part::ToolResult {
            tool_call_id,
            content: result.content,
            is_error: result.metadata
                .as_ref()
                .and_then(|m| m.get("is_error"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }
    }
}
