//! Agent trait and related types

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{agent_definition::AgentDefinition, error::Result, message::Message};

#[async_trait]
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;

    async fn run(&self, ctx: &mut AgentContext) -> Result<AgentResult>;

    fn system_prompt(&self) -> String;

    fn supported_tools(&self) -> Vec<String> {
        vec![]
    }

    /// Returns whether this agent should be hidden from the agent list
    fn is_hidden(&self) -> bool {
        false
    }

    /// Returns the underlying `AgentDefinition`, if this agent was created from one.
    /// Built-in agents return `None`; `DynamicAgent` returns `Some`.
    fn definition(&self) -> Option<&AgentDefinition> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct AgentContext {
    pub session_id: String,
    pub project_path: std::path::PathBuf,
    pub cwd: std::path::PathBuf,
    pub user_id: Option<String>,
    pub model_id: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub message: Message,
    pub should_continue: bool,
    pub stop_reason: StopReason,
    // G4: Token usage from the last LLM call
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::provider::TokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndOfTurn,
    MaxSteps,
    UserStopped,
    Error,
    ToolCalls(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
}
