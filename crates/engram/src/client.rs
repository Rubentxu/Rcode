//! Engram client for persistent memory operations

use crate::error::Result;
use crate::storage::EngramStorage;
use crate::types::Observation;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

/// In-memory index for fast lookups
pub struct EngramIndex {
    /// Cache of recent observation IDs
    recent_ids: Vec<i64>,
    /// Map of topic -> observation IDs
    topic_map: std::collections::HashMap<String, Vec<i64>>,
    /// Map of project -> observation IDs
    project_map: std::collections::HashMap<String, Vec<i64>>,
}

impl EngramIndex {
    fn new() -> Self {
        Self {
            recent_ids: Vec::new(),
            topic_map: std::collections::HashMap::new(),
            project_map: std::collections::HashMap::new(),
        }
    }

    fn add(&mut self, id: i64, obs: &Observation) {
        // Add to recent
        self.recent_ids.retain(|&x| x != id);
        self.recent_ids.insert(0, id);
        if self.recent_ids.len() > 100 {
            self.recent_ids.pop();
        }

        // Add to topic map
        if let Some(ref topic) = obs.topic_key {
            self.topic_map
                .entry(topic.clone())
                .or_default()
                .retain(|&x| x != id);
            self.topic_map.get_mut(topic).unwrap().insert(0, id);
        }

        // Add to project map
        if let Some(ref project) = obs.project {
            self.project_map
                .entry(project.clone())
                .or_default()
                .retain(|&x| x != id);
            self.project_map.get_mut(project).unwrap().insert(0, id);
        }
    }

    fn remove(&mut self, id: i64, obs: &Observation) {
        self.recent_ids.retain(|&x| x != id);

        if let Some(ref topic) = obs.topic_key {
            if let Some(ids) = self.topic_map.get_mut(topic) {
                ids.retain(|&x| x != id);
            }
        }

        if let Some(ref project) = obs.project {
            if let Some(ids) = self.project_map.get_mut(project) {
                ids.retain(|&x| x != id);
            }
        }
    }
}

/// Client for Engram persistent memory operations
pub struct EngramClient {
    storage_path: PathBuf,
    storage: Arc<EngramStorage>,
    index: Arc<RwLock<EngramIndex>>,
}

impl EngramClient {
    /// Create a new EngramClient with storage at the given path
    pub fn new(storage_path: &std::path::Path) -> Result<Self> {
        let storage = EngramStorage::new(storage_path)?;
        let index = Self::build_index(&storage)?;

        Ok(Self {
            storage_path: storage_path.to_path_buf(),
            storage: Arc::new(storage),
            index: Arc::new(RwLock::new(index)),
        })
    }

    /// Build the in-memory index from storage
    fn build_index(storage: &EngramStorage) -> Result<EngramIndex> {
        let recent = storage.get_recent(100)?;
        let mut index = EngramIndex::new();

        for obs in recent {
            if let Some(id) = obs.id {
                index.add(id, &obs);
            }
        }

        Ok(index)
    }

    /// Save an observation and return its ID
    pub async fn save(&self, observation: Observation) -> Result<i64> {
        let id = self.storage.insert(&observation)?;
        let mut obs_with_id = observation;
        obs_with_id.id = Some(id);

        // Update index
        self.index.write().add(id, &obs_with_id);

        Ok(id)
    }

    /// Search observations by query string
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Observation>> {
        self.storage.search(query, limit)
    }

    /// Get an observation by ID
    pub async fn get(&self, id: i64) -> Result<Option<Observation>> {
        self.storage.get(id)
    }

    /// Update an existing observation
    pub async fn update(&self, id: i64, observation: Observation) -> Result<()> {
        let old_obs = self.storage.get(id)?;
        self.storage.update(id, &observation)?;

        // Update index
        if let Some(old) = old_obs {
            self.index.write().remove(id, &old);
        }
        self.index.write().add(id, &observation);

        Ok(())
    }

    /// Delete an observation by ID
    pub async fn delete(&self, id: i64) -> Result<()> {
        let old_obs = self.storage.get(id)?;
        self.storage.delete(id)?;

        // Update index
        if let Some(old) = old_obs {
            self.index.write().remove(id, &old);
        }

        Ok(())
    }

    /// Get recent observations
    pub async fn get_context(&self, limit: usize) -> Result<Vec<Observation>> {
        self.storage.get_recent(limit)
    }

    /// Get observations by topic
    pub async fn get_topic(&self, topic: &str) -> Result<Vec<Observation>> {
        self.storage.get_by_topic(topic)
    }

    /// Get observations by project
    pub async fn get_project(&self, project: &str) -> Result<Vec<Observation>> {
        self.storage.get_by_project(project)
    }

    /// Get the storage path
    pub fn storage_path(&self) -> &std::path::Path {
        &self.storage_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ObservationType;
    use tempfile::tempdir;

    fn create_test_client() -> (EngramClient, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let client = EngramClient::new(&dir.path().join("test.db")).unwrap();
        (client, dir)
    }

    #[tokio::test]
    async fn test_save_and_get() {
        let (client, _dir) = create_test_client();

        let obs = Observation::new(
            "Test".to_string(),
            "Content".to_string(),
            ObservationType::Discovery,
        );
        let id = client.save(obs).await.unwrap();

        let retrieved = client.get(id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test");
    }

    #[tokio::test]
    async fn test_search() {
        let (client, _dir) = create_test_client();

        let obs = Observation::new(
            "Rust error handling".to_string(),
            "Use thiserror".to_string(),
            ObservationType::Pattern,
        );
        client.save(obs).await.unwrap();

        let results = client.search("error handling", 10).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_get_context() {
        let (client, _dir) = create_test_client();

        for i in 0..5 {
            let obs = Observation::new(
                format!("Title {}", i),
                format!("Content {}", i),
                ObservationType::Discovery,
            );
            client.save(obs).await.unwrap();
        }

        let context = client.get_context(3).await.unwrap();
        assert_eq!(context.len(), 3);
    }

    #[tokio::test]
    async fn test_get_topic() {
        let (client, _dir) = create_test_client();

        let obs = Observation::with_topic(
            Observation::new(
                "Architecture".to_string(),
                "Use SQLite".to_string(),
                ObservationType::Decision,
            ),
            "architecture".to_string(),
        );
        client.save(obs).await.unwrap();

        let results = client.get_topic("architecture").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_update() {
        let (client, _dir) = create_test_client();

        let obs = Observation::new(
            "Original".to_string(),
            "Content".to_string(),
            ObservationType::Decision,
        );
        let id = client.save(obs).await.unwrap();

        let mut updated = Observation::new(
            "Updated".to_string(),
            "New content".to_string(),
            ObservationType::Decision,
        );
        client.update(id, updated).await.unwrap();

        let retrieved = client.get(id).await.unwrap().unwrap();
        assert_eq!(retrieved.title, "Updated");
    }

    #[tokio::test]
    async fn test_delete() {
        let (client, _dir) = create_test_client();

        let obs = Observation::new(
            "To delete".to_string(),
            "Content".to_string(),
            ObservationType::Bugfix,
        );
        let id = client.save(obs).await.unwrap();

        client.delete(id).await.unwrap();
        assert!(client.get(id).await.unwrap().is_none());
    }
}
