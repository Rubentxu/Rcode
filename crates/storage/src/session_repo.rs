//! Session repository for SQLite persistence

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::sync::Mutex;

use opencode_core::{Session, SessionId, SessionStatus};

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
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO sessions (id, project_path, agent_id, model_id, title, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                project_path = excluded.project_path,
                agent_id = excluded.agent_id,
                model_id = excluded.model_id,
                title = excluded.title,
                status = excluded.status,
                updated_at = excluded.updated_at
            "#,
            params![
                session.id.0,
                session.project_path.to_string_lossy().to_string(),
                session.agent_id,
                session.model_id,
                session.title,
                serde_json::to_string(&session.status)?,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
            ],
        )
        .context("Failed to save session")?;
        Ok(())
    }

    pub fn load(&self, id: &SessionId) -> Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, project_path, agent_id, model_id, title, status, created_at, updated_at
                FROM sessions WHERE id = ?1
                "#,
            )
            .context("Failed to prepare statement")?;

        let mut rows = stmt.query(params![id.0])?;

        if let Some(row) = rows.next()? {
            let status_str: String = row.get(5)?;
            let session = Session {
                id: SessionId(row.get(0)?),
                project_path: std::path::PathBuf::from(row.get::<_, String>(1)?),
                agent_id: row.get(2)?,
                model_id: row.get(3)?,
                title: row.get(4)?,
                status: serde_json::from_str(&status_str).unwrap_or(SessionStatus::Idle),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            };
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    pub fn list(&self) -> Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, project_path, agent_id, model_id, title, status, created_at, updated_at
                FROM sessions
                ORDER BY updated_at DESC
                "#,
            )
            .context("Failed to prepare statement")?;

        let sessions = stmt
            .query_map([], |row| {
                let status_str: String = row.get(5)?;
                Ok(Session {
                    id: SessionId(row.get(0)?),
                    project_path: std::path::PathBuf::from(row.get::<_, String>(1)?),
                    agent_id: row.get(2)?,
                    model_id: row.get(3)?,
                    title: row.get(4)?,
                    status: serde_json::from_str(&status_str).unwrap_or(SessionStatus::Idle),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                })
            })
            .context("Failed to query sessions")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to collect sessions")?;

        Ok(sessions)
    }

    pub fn delete(&self, id: &SessionId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id.0])
            .context("Failed to delete session")?;
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
