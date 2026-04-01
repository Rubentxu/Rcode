//! Subagent management for spawning and tracking sub-agents

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;

use rcode_core::agent::Agent;
use rcode_core::error::Result;
use rcode_core::SessionId;

/// Agent ID type for identifying subagents
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubagentId(pub String);

impl SubagentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SubagentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Represents an active subagent instance
pub struct SubagentInstance {
    pub id: SubagentId,
    pub agent_type: String,
    pub session_id: SessionId,
    pub agent: Arc<dyn Agent>,
    pub status: SubagentStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// SubagentManager handles creation and lifecycle of subagents
pub struct SubagentManager {
    /// Active subagents indexed by subagent ID
    agents: RwLock<HashMap<SubagentId, SubagentInstance>>,
    /// Maps task_id to subagent_id for session continuation
    task_to_agent: RwLock<HashMap<String, SubagentId>>,
    /// Maps session_id to subagent_id
    session_to_agent: RwLock<HashMap<SessionId, SubagentId>>,
}

impl SubagentManager {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            task_to_agent: RwLock::new(HashMap::new()),
            session_to_agent: RwLock::new(HashMap::new()),
        }
    }
    
    /// Create a new subagent
    pub fn create_subagent(
        &self,
        task_id: &str,
        agent_type: &str,
        agent: Arc<dyn Agent>,
    ) -> Result<SubagentId> {
        let subagent_id = SubagentId::new(format!("subagent_{}_{}", agent_type, 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        
        let session_id = SessionId::new();
        
        let instance = SubagentInstance {
            id: subagent_id.clone(),
            agent_type: agent_type.to_string(),
            session_id: session_id.clone(),
            agent,
            status: SubagentStatus::Pending,
        };
        
        // Register the subagent
        self.agents.write().insert(subagent_id.clone(), instance);
        self.task_to_agent.write().insert(task_id.to_string(), subagent_id.clone());
        self.session_to_agent.write().insert(session_id.clone(), subagent_id.clone());
        
        Ok(subagent_id)
    }
    
    /// Get a subagent by ID
    pub fn get(&self, id: &SubagentId) -> Option<Arc<dyn Agent>> {
        self.agents
            .read()
            .get(id)
            .map(|instance| instance.agent.clone())
    }
    
    /// Get subagent ID for a task
    pub fn get_by_task_id(&self, task_id: &str) -> Option<SubagentId> {
        self.task_to_agent.read().get(task_id).cloned()
    }
    
    /// Get subagent ID for a session
    pub fn get_by_session_id(&self, session_id: &SessionId) -> Option<SubagentId> {
        self.session_to_agent.read().get(session_id).cloned()
    }
    
    /// Update subagent status
    pub fn update_status(&self, id: &SubagentId, status: SubagentStatus) -> bool {
        if let Some(instance) = self.agents.write().get_mut(id) {
            instance.status = status;
            true
        } else {
            false
        }
    }
    
    /// Get all active subagents
    pub fn list_active(&self) -> Vec<SubagentId> {
        self.agents
            .read()
            .values()
            .filter(|i| matches!(i.status, SubagentStatus::Pending | SubagentStatus::Running))
            .map(|i| i.id.clone())
            .collect()
    }
    
    /// Remove a subagent
    pub fn remove(&self, id: &SubagentId) -> Option<SubagentInstance> {
        if let Some(instance) = self.agents.write().remove(id) {
            // Clean up mappings
            self.task_to_agent.write().retain(|_, v| v != id);
            self.session_to_agent.write().retain(|_, v| v != id);
            Some(instance)
        } else {
            None
        }
    }
    
    /// Remove subagent by task_id
    pub fn remove_by_task_id(&self, task_id: &str) -> Option<SubagentInstance> {
        if let Some(subagent_id) = self.get_by_task_id(task_id) {
            self.remove(&subagent_id)
        } else {
            None
        }
    }
    
    /// Check if a subagent exists for a task
    pub fn has_task(&self, task_id: &str) -> bool {
        self.task_to_agent.read().contains_key(task_id)
    }
    
    /// Get session ID for a task
    pub fn get_session_for_task(&self, task_id: &str) -> Option<SessionId> {
        let subagent_id = self.task_to_agent.read().get(task_id)?.clone();
        let agents = self.agents.read();
        let instance = agents.get(&subagent_id)?;
        Some(instance.session_id.clone())
    }
}

impl Default for SubagentManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::agent::Agent;
    use rcode_core::{AgentContext, AgentResult};

    struct MockAgent {
        id: String,
    }
    
    impl MockAgent {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }
    
    #[async_trait::async_trait]
    impl Agent for MockAgent {
        fn id(&self) -> &str { &self.id }
        fn name(&self) -> &str { "Mock Agent" }
        fn description(&self) -> &str { "A mock agent for testing" }
        
        async fn run(&self, _ctx: &mut AgentContext) -> Result<AgentResult> {
            todo!()
        }
        
        fn system_prompt(&self) -> String {
            "Mock system prompt".to_string()
        }
    }
    
    #[test]
    fn test_create_subagent() {
        let manager = SubagentManager::new();
        let agent = Arc::new(MockAgent::new("test_agent"));
        
        let subagent_id = manager.create_subagent("task_1", "general", agent.clone()).unwrap();
        
        assert!(manager.get(&subagent_id).is_some());
        assert!(manager.has_task("task_1"));
        assert!(manager.get_session_for_task("task_1").is_some());
    }
    
    #[test]
    fn test_remove_subagent() {
        let manager = SubagentManager::new();
        let agent = Arc::new(MockAgent::new("test_agent"));
        
        let subagent_id = manager.create_subagent("task_1", "general", agent.clone()).unwrap();
        
        assert!(manager.get(&subagent_id).is_some());
        
        let removed = manager.remove(&subagent_id).unwrap();
        assert_eq!(removed.id, subagent_id);
        
        assert!(manager.get(&subagent_id).is_none());
        assert!(!manager.has_task("task_1"));
    }
    
    #[test]
    fn test_list_active() {
        let manager = SubagentManager::new();
        let agent = Arc::new(MockAgent::new("test_agent"));
        
        let _id1 = manager.create_subagent("task_1", "general", agent.clone()).unwrap();
        let id2 = manager.create_subagent("task_2", "explore", agent.clone()).unwrap();
        
        let active = manager.list_active();
        assert_eq!(active.len(), 2);
        
        // Remove one and check it's not in active list
        manager.remove(&id2);
        let active = manager.list_active();
        assert_eq!(active.len(), 1);
    }
}
