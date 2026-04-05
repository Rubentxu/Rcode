//! Agent loader for loading custom agents from JSON files

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::agent_definition::AgentDefinition;
use crate::error::{Result, RCodeError};

/// Loader for discovering and loading custom agents from filesystem
pub struct AgentLoader {
    search_paths: Vec<PathBuf>,
}

impl AgentLoader {
    /// Create a new AgentLoader with default search paths:
    /// - ~/.config/opencode/agents/
    /// - ./.opencode/agents/
    pub fn new() -> Self {
        let search_paths = vec![
            dirs::home_dir().map(|p| p.join(".config/opencode/agents")),
            std::env::current_dir()
                .ok()
                .map(|p| p.join(".opencode/agents")),
        ]
        .into_iter()
        .flatten()
        .collect();

        Self { search_paths }
    }

    /// Create an AgentLoader with custom search paths
    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        Self { search_paths: paths }
    }

    /// Add a search path
    pub fn add_path(&mut self, path: PathBuf) {
        self.search_paths.push(path);
    }

    /// Load all agents from the configured search paths
    pub async fn load_agents(&self) -> Result<Vec<AgentDefinition>> {
        let mut agents = Vec::new();

        for path in &self.search_paths {
            if path.exists() {
                let discovered = self.discover_agents(path).await?;
                agents.extend(discovered);
            }
        }

        Ok(agents)
    }

    /// Discover agents in a specific directory
    async fn discover_agents(&self, path: &Path) -> Result<Vec<AgentDefinition>> {
        let mut agents = Vec::new();

        if !path.is_dir() {
            return Ok(agents);
        }

        for entry in WalkDir::new(path)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let file_name = entry.file_name();
            
            // Look for agent.json files
            if file_name == "agent.json" {
                match self.load_from_file(entry.path()).await {
                    Ok(agent) => {
                        tracing::debug!("Loaded custom agent: {}", agent.identifier);
                        agents.push(agent);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load agent from {}: {}", entry.path().display(), e);
                    }
                }
            }
        }

        Ok(agents)
    }

    /// Load a single agent from a JSON file
    async fn load_from_file(&self, path: &Path) -> Result<AgentDefinition> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| RCodeError::Config(format!("Failed to read agent file {}: {}", path.display(), e)))?;
        
        let definition: AgentDefinition = serde_json::from_str(&content)
            .map_err(|e| RCodeError::Config(format!("Failed to parse agent JSON {}: {}", path.display(), e)))?;
        
        // Validate the definition
        self.validate_definition(&definition)?;
        
        Ok(definition)
    }

    /// Validate an agent definition
    fn validate_definition(&self, def: &AgentDefinition) -> Result<()> {
        if def.identifier.is_empty() {
            return Err(RCodeError::Config(
                "Agent identifier cannot be empty".into(),
            ));
        }

        if def.name.is_empty() {
            return Err(RCodeError::Config(
                format!("Agent '{}' has empty name", def.identifier),
            ));
        }

        if def.system_prompt.is_empty() {
            return Err(RCodeError::Config(
                format!("Agent '{}' has empty system_prompt", def.identifier),
            ));
        }

        Ok(())
    }
}

impl Default for AgentLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_load_agents_from_directory() {
        // Create a temporary directory with agent files
        let temp_dir = TempDir::new().unwrap();
        let agent_dir = temp_dir.path().join("agents");
        std::fs::create_dir_all(&agent_dir).unwrap();

        // Create the agent subdirectory
        let test_agent_dir = agent_dir.join("test-agent");
        std::fs::create_dir_all(&test_agent_dir).unwrap();

        // Write a test agent
        let agent_json = serde_json::json!({
            "identifier": "test-agent",
            "name": "Test Agent",
            "description": "A test agent",
            "when_to_use": "For testing",
            "system_prompt": "You are a test agent.",
            "mode": "all",
            "hidden": false,
            "tools": ["read", "write"]
        });

        std::fs::write(
            test_agent_dir.join("agent.json"),
            serde_json::to_string_pretty(&agent_json).unwrap(),
        )
        .unwrap();

        let loader = AgentLoader::with_paths(vec![agent_dir]);
        let agents = loader.load_agents().await.unwrap();

        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].identifier, "test-agent");
        assert_eq!(agents[0].name, "Test Agent");
    }

    #[tokio::test]
    async fn test_validate_empty_identifier() {
        // This test verifies that agents with invalid data (empty identifier) are skipped
        // rather than causing the entire load to fail. Validation errors are logged but
        // individual agent failures don't fail the whole process.
        let agent_json = serde_json::json!({
            "identifier": "",
            "name": "Test Agent",
            "system_prompt": "You are a test agent."
        });

        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().to_path_buf();
        std::fs::create_dir_all(&dir_path).unwrap();
        let file_path = dir_path.join("agent.json");
        std::fs::write(
            &file_path,
            serde_json::to_string_pretty(&agent_json).unwrap(),
        )
        .unwrap();

        let loader = AgentLoader::with_paths(vec![dir_path]);
        let result = loader.load_agents().await;
        
        // The load succeeds (returns Ok) but the agent list is empty because
        // the agent with invalid data was skipped
        assert!(result.is_ok(), "Expected load to succeed, got: {:?}", result);
        let agents = result.unwrap();
        assert!(agents.is_empty(), "Expected no agents to be loaded due to validation error, got {} agents", agents.len());
    }

    #[tokio::test]
    async fn test_validate_empty_name() {
        let agent_json = serde_json::json!({
            "identifier": "test-agent",
            "name": "",
            "system_prompt": "You are a test agent."
        });

        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().to_path_buf();
        std::fs::create_dir_all(&dir_path).unwrap();
        let file_path = dir_path.join("agent.json");
        std::fs::write(
            &file_path,
            serde_json::to_string_pretty(&agent_json).unwrap(),
        )
        .unwrap();

        let loader = AgentLoader::with_paths(vec![dir_path]);
        let result = loader.load_agents().await;
        
        assert!(result.is_ok());
        let agents = result.unwrap();
        assert!(agents.is_empty(), "Expected no agents due to empty name validation error");
    }

    #[tokio::test]
    async fn test_validate_empty_system_prompt() {
        let agent_json = serde_json::json!({
            "identifier": "test-agent",
            "name": "Test Agent",
            "system_prompt": ""
        });

        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().to_path_buf();
        std::fs::create_dir_all(&dir_path).unwrap();
        let file_path = dir_path.join("agent.json");
        std::fs::write(
            &file_path,
            serde_json::to_string_pretty(&agent_json).unwrap(),
        )
        .unwrap();

        let loader = AgentLoader::with_paths(vec![dir_path]);
        let result = loader.load_agents().await;
        
        assert!(result.is_ok());
        let agents = result.unwrap();
        assert!(agents.is_empty(), "Expected no agents due to empty system_prompt validation error");
    }

    #[tokio::test]
    async fn test_discover_agents_nonexistent_directory() {
        let loader = AgentLoader::with_paths(vec![PathBuf::from("/nonexistent/path")]);
        let agents = loader.load_agents().await.unwrap();
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_add_search_path() {
        let mut loader = AgentLoader::new();
        assert_eq!(loader.search_paths.len(), 2); // default paths
        
        loader.add_path(PathBuf::from("/custom/path"));
        assert_eq!(loader.search_paths.len(), 3);
    }

    #[tokio::test]
    async fn test_load_from_nonexistent_file() {
        let loader = AgentLoader::new();
        let result = loader.load_from_file(Path::new("/nonexistent/file.json")).await;
        assert!(result.is_err());
    }
}