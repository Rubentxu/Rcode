//! SQLite storage layer for Engram observations

use crate::error::{EngramError, Result};
use crate::types::Observation;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::path::Path;

/// SQLite storage for observations with FTS5 support
pub struct EngramStorage {
    conn: Mutex<Connection>,
}

impl EngramStorage {
    /// Create a new EngramStorage with the database at the given path
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS observations (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                type TEXT NOT NULL,
                scope TEXT NOT NULL,
                topic_key TEXT,
                project TEXT,
                session_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_observations_project ON observations(project);
            CREATE INDEX IF NOT EXISTS idx_observations_topic ON observations(topic_key);
            CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(session_id);
            "#,
        )?;

        // Create FTS5 virtual table for full-text search (ignore errors if already exists)
        let _ = conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS observations_fts USING fts5(title, content)",
            [],
        );

        // Populate FTS index if empty
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM observations_fts", [], |row| {
            row.get(0)
        })?;
        if count == 0 {
            let _ = conn.execute(
                "INSERT INTO observations_fts(rowid, title, content) SELECT id, title, content FROM observations",
                [],
            );
        }

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Insert a new observation and return its ID
    pub fn insert(&self, obs: &Observation) -> Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO observations (title, content, type, scope, topic_key, project, session_id, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                obs.title,
                obs.content,
                obs.obs_type.to_string(),
                obs.scope.to_string(),
                obs.topic_key,
                obs.project,
                obs.session_id,
                obs.created_at.to_rfc3339(),
                obs.updated_at.to_rfc3339(),
            ],
        )?;
        let id = conn.last_insert_rowid();

        // Insert into FTS index
        let _ = conn.execute(
            "INSERT INTO observations_fts(rowid, title, content) VALUES (?1, ?2, ?3)",
            params![id, obs.title, obs.content],
        );

        Ok(id)
    }

    /// Get an observation by ID
    pub fn get(&self, id: i64) -> Result<Option<Observation>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, title, content, type, scope, topic_key, project, session_id, created_at, updated_at
            FROM observations WHERE id = ?1
            "#,
        )?;

        let mut rows = stmt.query(params![id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_observation(row)?))
        } else {
            Ok(None)
        }
    }

    /// Update an existing observation
    pub fn update(&self, id: i64, obs: &Observation) -> Result<()> {
        let conn = self.conn.lock();
        let rows_affected = conn.execute(
            r#"
            UPDATE observations 
            SET title = ?1, content = ?2, type = ?3, scope = ?4, 
                topic_key = ?5, project = ?6, session_id = ?7, updated_at = ?8
            WHERE id = ?9
            "#,
            params![
                obs.title,
                obs.content,
                obs.obs_type.to_string(),
                obs.scope.to_string(),
                obs.topic_key,
                obs.project,
                obs.session_id,
                Utc::now().to_rfc3339(),
                id,
            ],
        )?;

        if rows_affected == 0 {
            return Err(EngramError::NotFound(format!(
                "Observation {} not found",
                id
            )));
        }

        // Update FTS index
        let _ = conn.execute(
            "INSERT INTO observations_fts(rowid, title, content) VALUES (?1, ?2, ?3) ON CONFLICT(rowid) DO UPDATE SET title = ?2, content = ?3",
            params![id, obs.title, obs.content],
        );

        Ok(())
    }

    /// Delete an observation by ID
    pub fn delete(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM observations WHERE id = ?1", params![id])?;
        let _ = conn.execute("DELETE FROM observations_fts WHERE rowid = ?1", params![id]);
        Ok(())
    }

    /// Search observations by query string using FTS5
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Observation>> {
        let conn = self.conn.lock();

        // Escape special FTS5 characters and format as phrase match
        let fts_query = format!("\"{}\"", query.replace('"', "\"\""));

        let mut stmt = conn.prepare(
            r#"
            SELECT o.id, o.title, o.content, o.type, o.scope, o.topic_key, o.project, o.session_id, o.created_at, o.updated_at
            FROM observations o
            JOIN observations_fts fts ON o.id = fts.rowid
            WHERE observations_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
            "#,
        )?;

        let limit = limit as i64;
        let mut rows = stmt.query(params![fts_query, limit])?;

        let mut observations = Vec::new();
        while let Some(row) = rows.next()? {
            observations.push(self.row_to_observation(row)?);
        }

        Ok(observations)
    }

    /// Get recent observations ordered by updated_at
    pub fn get_recent(&self, limit: usize) -> Result<Vec<Observation>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, title, content, type, scope, topic_key, project, session_id, created_at, updated_at
            FROM observations
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
        )?;

        let limit = limit as i64;
        let mut rows = stmt.query(params![limit])?;

        let mut observations = Vec::new();
        while let Some(row) = rows.next()? {
            observations.push(self.row_to_observation(row)?);
        }

        Ok(observations)
    }

    /// Get observations by topic key
    pub fn get_by_topic(&self, topic: &str) -> Result<Vec<Observation>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, title, content, type, scope, topic_key, project, session_id, created_at, updated_at
            FROM observations
            WHERE topic_key = ?1
            ORDER BY updated_at DESC
            "#,
        )?;

        let mut rows = stmt.query(params![topic])?;

        let mut observations = Vec::new();
        while let Some(row) = rows.next()? {
            observations.push(self.row_to_observation(row)?);
        }

        Ok(observations)
    }

    /// Get observations by project
    pub fn get_by_project(&self, project: &str) -> Result<Vec<Observation>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, title, content, type, scope, topic_key, project, session_id, created_at, updated_at
            FROM observations
            WHERE project = ?1
            ORDER BY updated_at DESC
            "#,
        )?;

        let mut rows = stmt.query(params![project])?;

        let mut observations = Vec::new();
        while let Some(row) = rows.next()? {
            observations.push(self.row_to_observation(row)?);
        }

        Ok(observations)
    }

    fn row_to_observation(&self, row: &rusqlite::Row) -> Result<Observation> {
        let type_str: String = row.get(3)?;
        let scope_str: String = row.get(4)?;
        let created_at_str: String = row.get(8)?;
        let updated_at_str: String = row.get(9)?;

        Ok(Observation {
            id: Some(row.get(0)?),
            title: row.get(1)?,
            content: row.get(2)?,
            obs_type: type_str.parse().map_err(|e: String| {
                EngramError::InvalidInput(format!("Invalid observation type: {}", e))
            })?,
            scope: scope_str
                .parse()
                .map_err(|e: String| EngramError::InvalidInput(format!("Invalid scope: {}", e)))?,
            topic_key: row.get(5)?,
            project: row.get(6)?,
            session_id: row.get(7)?,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| EngramError::InvalidInput(format!("Invalid date: {}", e)))?
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|e| EngramError::InvalidInput(format!("Invalid date: {}", e)))?
                .with_timezone(&Utc),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ObservationType;
    use tempfile::tempdir;

    fn create_test_storage() -> (EngramStorage, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let storage = EngramStorage::new(&dir.path().join("test.db")).unwrap();
        (storage, dir)
    }

    #[test]
    fn test_insert_and_get() {
        let (storage, _dir) = create_test_storage();

        let obs = Observation::new(
            "Test title".to_string(),
            "Test content".to_string(),
            ObservationType::Discovery,
        );

        let id = storage.insert(&obs).unwrap();
        assert!(id > 0);

        let retrieved = storage.get(id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.title, "Test title");
        assert_eq!(retrieved.content, "Test content");
    }

    #[test]
    fn test_update() {
        let (storage, _dir) = create_test_storage();

        let obs = Observation::new(
            "Original title".to_string(),
            "Original content".to_string(),
            ObservationType::Decision,
        );

        let id = storage.insert(&obs).unwrap();

        let mut updated = obs.clone();
        updated.title = "Updated title".to_string();

        storage.update(id, &updated).unwrap();

        let retrieved = storage.get(id).unwrap().unwrap();
        assert_eq!(retrieved.title, "Updated title");
    }

    #[test]
    fn test_delete() {
        let (storage, _dir) = create_test_storage();

        let obs = Observation::new(
            "To delete".to_string(),
            "Content".to_string(),
            ObservationType::Bugfix,
        );

        let id = storage.insert(&obs).unwrap();
        storage.delete(id).unwrap();

        assert!(storage.get(id).unwrap().is_none());
    }

    #[test]
    fn test_search() {
        let (storage, _dir) = create_test_storage();

        let obs1 = Observation::new(
            "Rust error handling".to_string(),
            "Use thiserror for error types".to_string(),
            ObservationType::Pattern,
        );
        let obs2 = Observation::new(
            "Python tips".to_string(),
            "Use pip for package management".to_string(),
            ObservationType::Learning,
        );

        storage.insert(&obs1).unwrap();
        storage.insert(&obs2).unwrap();

        let results = storage.search("error handling", 10).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].title.contains("Rust"));
    }

    #[test]
    fn test_get_recent() {
        let (storage, _dir) = create_test_storage();

        for i in 0..5 {
            let obs = Observation::new(
                format!("Title {}", i),
                format!("Content {}", i),
                ObservationType::Discovery,
            );
            storage.insert(&obs).unwrap();
        }

        let recent = storage.get_recent(3).unwrap();
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn test_get_by_topic() {
        let (storage, _dir) = create_test_storage();

        let obs1 = Observation::with_topic(
            Observation::new(
                "Architecture decision".to_string(),
                "Use SQLite".to_string(),
                ObservationType::Decision,
            ),
            "architecture/database".to_string(),
        );
        let obs2 = Observation::new(
            "Bugfix".to_string(),
            "Fixed a bug".to_string(),
            ObservationType::Bugfix,
        );

        storage.insert(&obs1).unwrap();
        storage.insert(&obs2).unwrap();

        let results = storage.get_by_topic("architecture/database").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Architecture decision");
    }
}
