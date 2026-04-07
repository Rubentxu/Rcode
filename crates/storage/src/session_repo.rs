//! Session repository for SQLite persistence

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::sync::Mutex;

use crate::StorageError;
use rcode_core::{Session, SessionId, SessionStatus};

pub struct SessionRepository {
    conn: Mutex<Connection>,
}

impl SessionRepository {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;
        conn.execute(
            r#"
            INSERT INTO sessions (id, project_path, agent_id, model_id, parent_id, title, status, created_at, updated_at, prompt_tokens, completion_tokens, total_cost_usd, summary_message_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(id) DO UPDATE SET
                project_path = excluded.project_path,
                agent_id = excluded.agent_id,
                model_id = excluded.model_id,
                parent_id = excluded.parent_id,
                title = excluded.title,
                status = excluded.status,
                updated_at = excluded.updated_at,
                prompt_tokens = excluded.prompt_tokens,
                completion_tokens = excluded.completion_tokens,
                total_cost_usd = excluded.total_cost_usd,
                summary_message_id = excluded.summary_message_id
            "#,
            params![
                session.id.0,
                session.project_path.to_string_lossy().to_string(),
                session.agent_id,
                session.model_id,
                session.parent_id,
                session.title,
                serde_json::to_string(&session.status)?,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
                session.prompt_tokens as i64,
                session.completion_tokens as i64,
                session.total_cost_usd,
                session.summary_message_id,
            ],
        )
        .context("Failed to save session")?;
        Ok(())
    }

    pub fn load(&self, id: &SessionId) -> Result<Option<Session>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, project_path, agent_id, model_id, parent_id, title, status, created_at, updated_at, prompt_tokens, completion_tokens, total_cost_usd, summary_message_id
                FROM sessions WHERE id = ?1
                "#,
            )
            .context("Failed to prepare statement")?;

        let mut rows = stmt.query(params![id.0])?;

        if let Some(row) = rows.next()? {
            let status_str: String = row.get(6)?;
            let session = Session {
                id: SessionId(row.get(0)?),
                project_path: std::path::PathBuf::from(row.get::<_, String>(1)?),
                agent_id: row.get(2)?,
                model_id: row.get(3)?,
                parent_id: row.get(4)?,
                title: row.get(5)?,
                status: serde_json::from_str(&status_str).unwrap_or(SessionStatus::Idle),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                    .map_err(|e| StorageError::InvalidTimestamp(e.to_string()))?
                    .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                    .map_err(|e| StorageError::InvalidTimestamp(e.to_string()))?
                    .with_timezone(&chrono::Utc),
                // G3: Token usage fields
                prompt_tokens: row.get::<_, i64>(9)? as u64,
                completion_tokens: row.get::<_, i64>(10)? as u64,
                total_cost_usd: row.get(11)?,
                summary_message_id: row.get(12)?,
            };
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    pub fn list(&self) -> Result<Vec<Session>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, project_path, agent_id, model_id, parent_id, title, status, created_at, updated_at, prompt_tokens, completion_tokens, total_cost_usd, summary_message_id
                FROM sessions
                ORDER BY updated_at DESC
                "#,
            )
            .context("Failed to prepare statement")?;

        // First collect raw session strings from DB
        #[allow(clippy::type_complexity)]
        let mut raw_rows: Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            i64,
            i64,
            f64,
            Option<String>,
        )> = Vec::new();
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))
            })
            .context("Failed to query sessions")?;

        for result in rows {
            match result {
                Ok(row) => raw_rows.push(row),
                Err(e) => eprintln!("Error reading row: {:?}", e),
            }
        }

        // Now build sessions with parsed timestamps
        let sessions: Vec<Session> = raw_rows
            .into_iter()
            .map(
                |(
                    id,
                    project_path,
                    agent_id,
                    model_id,
                    parent_id,
                    title,
                    status_str,
                    created_at_str,
                    updated_at_str,
                    prompt_tokens,
                    completion_tokens,
                    total_cost_usd,
                    summary_message_id,
                )|
                 -> Result<Session, StorageError> {
                    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                        .map_err(|e| StorageError::InvalidTimestamp(format!("created_at: {e}")))?
                        .with_timezone(&chrono::Utc);
                    let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at_str)
                        .map_err(|e| StorageError::InvalidTimestamp(format!("updated_at: {e}")))?
                        .with_timezone(&chrono::Utc);
                    Ok(Session {
                        id: SessionId(id),
                        project_path: std::path::PathBuf::from(project_path),
                        agent_id,
                        model_id,
                        parent_id,
                        title,
                        status: serde_json::from_str(&status_str).unwrap_or(SessionStatus::Idle),
                        created_at,
                        updated_at,
                        prompt_tokens: prompt_tokens as u64,
                        completion_tokens: completion_tokens as u64,
                        total_cost_usd,
                        summary_message_id,
                    })
                },
            )
            .collect::<Result<Vec<Session>, StorageError>>()?;

        Ok(sessions)
    }

    pub fn delete(&self, id: &SessionId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id.0])
            .context("Failed to delete session")?;
        Ok(())
    }

    /// Update the model_id for a session
    pub fn update_model(&self, session_id: &str, model_id: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE sessions SET model_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![model_id, now, session_id],
        )
        .context("Failed to update session model")?;
        Ok(())
    }

    /// G4: Update token usage for a session
    pub fn update_usage(
        &self,
        session_id: &str,
        prompt_tokens: u64,
        completion_tokens: u64,
        total_cost_usd: f64,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE sessions SET prompt_tokens = ?1, completion_tokens = ?2, total_cost_usd = ?3, updated_at = ?4 WHERE id = ?5",
            params![prompt_tokens as i64, completion_tokens as i64, total_cost_usd, now, session_id],
        )
        .context("Failed to update session usage")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use tempfile::tempdir;

    fn create_test_repo() -> (SessionRepository, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        schema::init_schema(&conn).unwrap();
        (SessionRepository::new(conn), dir)
    }

    #[test]
    fn test_save_and_load_session() {
        let (repo, _dir) = create_test_repo();
        let session = Session::new(
            std::path::PathBuf::from("/test/path"),
            "test-agent".to_string(),
            "claude-3-5".to_string(),
        );

        repo.save(&session).unwrap();
        let loaded = repo.load(&session.id).unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.agent_id, "test-agent");
        assert_eq!(loaded.model_id, "claude-3-5");
    }

    #[test]
    fn test_load_nonexistent() {
        let (repo, _dir) = create_test_repo();
        let loaded = repo.load(&SessionId::new()).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_list_sessions() {
        let (repo, _dir) = create_test_repo();
        let session1 = Session::new(
            std::path::PathBuf::from("/test/path1"),
            "agent1".to_string(),
            "model1".to_string(),
        );
        let session2 = Session::new(
            std::path::PathBuf::from("/test/path2"),
            "agent2".to_string(),
            "model2".to_string(),
        );

        repo.save(&session1).unwrap();
        repo.save(&session2).unwrap();

        let sessions = repo.list().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_delete_session() {
        let (repo, _dir) = create_test_repo();
        let session = Session::new(
            std::path::PathBuf::from("/test/path"),
            "test-agent".to_string(),
            "test-model".to_string(),
        );

        repo.save(&session).unwrap();
        assert!(repo.load(&session.id).unwrap().is_some());

        repo.delete(&session.id).unwrap();
        assert!(repo.load(&session.id).unwrap().is_none());
    }
}
