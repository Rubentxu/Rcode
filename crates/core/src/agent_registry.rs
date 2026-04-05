//! Agent registry for managing available agents

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;

use crate::agent::{Agent, AgentInfo};
use crate::error::Result;
use crate::agent_loader::AgentLoader;
use crate::dynamic_agent::DynamicAgent;

/// Registry for managing agents - both built-in and custom
pub struct AgentRegistry {
    /// Registered agents indexed by ID
    agents: RwLock<HashMap<String, Arc<dyn Agent>>>,
    /// Agent loader for custom agents
    loader: AgentLoader,
}

impl AgentRegistry {
    /// Create a new AgentRegistry with default loader
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            loader: AgentLoader::new(),
        }
    }

    /// Create an AgentRegistry with a custom loader
    pub fn with_loader(loader: AgentLoader) -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            loader,
        }
    }

    /// Load all agents from configured paths and register them
    pub async fn load_all(&self) -> Result<()> {
        let definitions = self.loader.load_agents().await?;

        for def in definitions {
            let agent = DynamicAgent::from_definition(def);
            self.register(agent);
        }

        Ok(())
    }

    /// Register an agent
    pub fn register(&self, agent: Arc<dyn Agent>) {
        let mut agents = self.agents.write();
        tracing::debug!("Registering agent: {} ({})", agent.name(), agent.id());
        agents.insert(agent.id().to_string(), agent);
    }

    /// Get an agent by ID
    pub fn get(&self, id: &str) -> Option<Arc<dyn Agent>> {
        self.agents.read().get(id).cloned()
    }

    /// Check if an agent exists
    pub fn contains(&self, id: &str) -> bool {
        self.agents.read().contains_key(id)
    }

    /// List all agents (excluding hidden ones)
    pub fn list(&self) -> Vec<AgentInfo> {
        self.list_all()
            .into_iter()
            .filter(|a| !a.hidden.unwrap_or(false))
            .collect()
    }

    /// List all agents including hidden ones
    pub fn list_all(&self) -> Vec<AgentInfo> {
        self.agents
            .read()
            .values()
            .map(|a| {
                // Get hidden flag from agent (uses default false for most agents)
                let hidden = a.is_hidden();
                AgentInfo {
                    id: a.id().to_string(),
                    name: a.name().to_string(),
                    description: a.description().to_string(),
                    model: None,
                    max_tokens: None,
                    reasoning_effort: None,
                    tools: if a.supported_tools().is_empty() {
                        None
                    } else {
                        Some(a.supported_tools())
                    },
                    hidden: if hidden { Some(true) } else { None },
                }
            })
            .collect()
    }

    /// Get agent IDs
    pub fn agent_ids(&self) -> Vec<String> {
        self.agents.read().keys().cloned().collect()
    }

    /// Remove an agent by ID
    pub fn unregister(&self, id: &str) -> Option<Arc<dyn Agent>> {
        self.agents.write().remove(id)
    }

    /// Get the number of registered agents
    pub fn len(&self) -> usize {
        self.agents.read().len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.agents.read().is_empty()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_definition::AgentDefinition;
    use tempfile::TempDir;

    fn create_test_agent(id: &str, name: &str) -> Arc<dyn Agent> {
        let def = AgentDefinition {
            identifier: id.to_string(),
            name: name.to_string(),
            description: format!("Test agent {}", id),
            when_to_use: "Testing".to_string(),
            system_prompt: "You are a test agent.".to_string(),
            mode: crate::agent_definition::AgentMode::All,
            hidden: false,
            permission: Default::default(),
            tools: vec!["read".to_string()],
            model: None,
            max_tokens: None,
            reasoning_effort: None,
        };
        DynamicAgent::from_definition(def)
    }

    #[test]
    fn test_register_and_get() {
        let registry = AgentRegistry::new();
        let agent = create_test_agent("test-1", "Test Agent 1");

        registry.register(agent.clone());

        assert!(registry.contains("test-1"));
        assert_eq!(registry.get("test-1").map(|a| a.id().to_string()), Some("test-1".to_string()));
    }

    #[test]
    fn test_list_agents() {
        let registry = AgentRegistry::new();

        registry.register(create_test_agent("test-1", "Test Agent 1"));
        registry.register(create_test_agent("test-2", "Test Agent 2"));

        let agents = registry.list();
        assert_eq!(agents.len(), 2);
    }

    #[test]
    fn test_unregister() {
        let registry = AgentRegistry::new();
        let agent = create_test_agent("test-1", "Test Agent 1");

        registry.register(agent);
        assert!(registry.contains("test-1"));

        let removed = registry.unregister("test-1");
        assert!(removed.is_some());
        assert!(!registry.contains("test-1"));
    }

    #[test]
    fn test_hidden_agents_filtered() {
        let registry = AgentRegistry::new();

        // Create a hidden agent
        let def = AgentDefinition {
            identifier: "hidden-agent".to_string(),
            name: "Hidden Agent".to_string(),
            description: "A hidden agent".to_string(),
            when_to_use: "Testing".to_string(),
            system_prompt: "You are hidden.".to_string(),
            mode: crate::agent_definition::AgentMode::All,
            hidden: true,
            permission: Default::default(),
            tools: vec![],
            model: None,
            max_tokens: None,
            reasoning_effort: None,
        };

        let hidden = DynamicAgent::from_definition(def);
        registry.register(hidden);
        registry.register(create_test_agent("visible", "Visible Agent"));

        let agents = registry.list();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "visible");
    }

    #[test]
    fn test_contains_method() {
        let registry = AgentRegistry::new();
        assert!(!registry.contains("test-1"));
        
        let agent = create_test_agent("test-1", "Test Agent 1");
        registry.register(agent);
        
        assert!(registry.contains("test-1"));
        assert!(!registry.contains("nonexistent"));
    }

    #[test]
    fn test_list_all_includes_hidden() {
        let registry = AgentRegistry::new();

        let def = AgentDefinition {
            identifier: "hidden-agent".to_string(),
            name: "Hidden Agent".to_string(),
            description: "A hidden agent".to_string(),
            when_to_use: "Testing".to_string(),
            system_prompt: "You are hidden.".to_string(),
            mode: crate::agent_definition::AgentMode::All,
            hidden: true,
            permission: Default::default(),
            tools: vec![],
            model: None,
            max_tokens: None,
            reasoning_effort: None,
        };

        let hidden = DynamicAgent::from_definition(def);
        registry.register(hidden);
        registry.register(create_test_agent("visible", "Visible Agent"));

        let all_agents = registry.list_all();
        assert_eq!(all_agents.len(), 2); // Both hidden and visible
    }

    #[test]
    fn test_agent_ids() {
        let registry = AgentRegistry::new();
        registry.register(create_test_agent("agent-a", "Agent A"));
        registry.register(create_test_agent("agent-b", "Agent B"));

        let ids = registry.agent_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"agent-a".to_string()));
        assert!(ids.contains(&"agent-b".to_string()));
    }

    #[test]
    fn test_len_and_is_empty() {
        let registry = AgentRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.register(create_test_agent("test-1", "Test"));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        registry.register(create_test_agent("test-2", "Test 2"));
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_unregister_nonexistent() {
        let registry = AgentRegistry::new();
        let removed = registry.unregister("nonexistent");
        assert!(removed.is_none());
    }

    #[tokio::test]
    async fn test_load_all_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let loader = AgentLoader::with_paths(vec![temp_dir.path().to_path_buf()]);
        let registry = AgentRegistry::with_loader(loader);
        
        registry.load_all().await.unwrap();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_with_loader_constructor() {
        let loader = AgentLoader::new();
        let registry = AgentRegistry::with_loader(loader);
        assert!(registry.is_empty());
    }
}