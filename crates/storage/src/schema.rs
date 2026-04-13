//! Database schema definitions

use rusqlite::{Connection, Result};

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            project_path TEXT NOT NULL,
            project_id TEXT,
            agent_id TEXT NOT NULL,
            model_id TEXT NOT NULL,
            parent_id TEXT,
            title TEXT,
            status TEXT NOT NULL DEFAULT 'idle',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            prompt_tokens INTEGER DEFAULT 0,
            completion_tokens INTEGER DEFAULT 0,
            total_cost_usd REAL DEFAULT 0.0,
            summary_message_id TEXT
        );

        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            canonical_path TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (session_id) REFERENCES sessions(id)
        );
        
        CREATE TABLE IF NOT EXISTS message_parts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id TEXT NOT NULL,
            part_type TEXT NOT NULL,
            content TEXT,
            tool_call_id TEXT,
            tool_call_name TEXT,
            is_error INTEGER DEFAULT 0,
            FOREIGN KEY (message_id) REFERENCES messages(id)
        );
        
        CREATE TABLE IF NOT EXISTS config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        
        CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT,
            event_type TEXT NOT NULL,
            data TEXT NOT NULL,
            sequence INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );
        
        CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
        CREATE INDEX IF NOT EXISTS idx_message_parts_message ON message_parts(message_id);
        CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
        
        CREATE TABLE IF NOT EXISTS model_catalog_cache (
            provider_id TEXT PRIMARY KEY,
            models TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )?;

    // Migration: Add parent_id column if it doesn't exist (for databases created before this column was added)
    let has_parent_id: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='parent_id'",
        [],
        |row| row.get::<_, i64>(0).map(|c| c > 0),
    )?;
    if !has_parent_id {
        conn.execute("ALTER TABLE sessions ADD COLUMN parent_id TEXT", [])?;
    }

    // Migration: Add title column if it doesn't exist (for databases created before this column was added)
    let has_title: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='title'",
        [],
        |row| row.get::<_, i64>(0).map(|c| c > 0),
    )?;
    if !has_title {
        conn.execute("ALTER TABLE sessions ADD COLUMN title TEXT", [])?;
    }

    // G3: Migration: Add token usage columns if they don't exist
    let has_prompt_tokens: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='prompt_tokens'",
        [],
        |row| row.get::<_, i64>(0).map(|c| c > 0),
    )?;
    if !has_prompt_tokens {
        conn.execute(
            "ALTER TABLE sessions ADD COLUMN prompt_tokens INTEGER DEFAULT 0",
            [],
        )?;
    }

    let has_completion_tokens: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='completion_tokens'",
        [],
        |row| row.get::<_, i64>(0).map(|c| c > 0),
    )?;
    if !has_completion_tokens {
        conn.execute(
            "ALTER TABLE sessions ADD COLUMN completion_tokens INTEGER DEFAULT 0",
            [],
        )?;
    }

    let has_total_cost_usd: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='total_cost_usd'",
        [],
        |row| row.get::<_, i64>(0).map(|c| c > 0),
    )?;
    if !has_total_cost_usd {
        conn.execute(
            "ALTER TABLE sessions ADD COLUMN total_cost_usd REAL DEFAULT 0.0",
            [],
        )?;
    }

    let has_summary_message_id: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='summary_message_id'",
        [],
        |row| row.get::<_, i64>(0).map(|c| c > 0),
    )?;
    if !has_summary_message_id {
        conn.execute(
            "ALTER TABLE sessions ADD COLUMN summary_message_id TEXT",
            [],
        )?;
    }

    let has_project_id: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='project_id'",
        [],
        |row| row.get::<_, i64>(0).map(|c| c > 0),
    )?;
    if !has_project_id {
        conn.execute("ALTER TABLE sessions ADD COLUMN project_id TEXT", [])?;
    }

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_sessions_project_id ON sessions(project_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_projects_canonical_path ON projects(canonical_path)",
        [],
    )?;

    backfill_projects(conn)?;

    Ok(())
}

fn backfill_projects(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        r#"
        SELECT DISTINCT project_path
        FROM sessions
        WHERE project_path IS NOT NULL AND TRIM(project_path) != ''
        "#,
    )?;

    let paths = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    for path in paths {
        let path_buf = std::path::PathBuf::from(&path);
        let canonical_path = match path_buf.canonicalize() {
            Ok(path) => path,
            Err(_) => continue,
        };
        let canonical_str = canonical_path.to_string_lossy().to_string();
        let name = canonical_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(&canonical_str)
            .to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let project_id = uuid::Uuid::new_v4().to_string();

        conn.execute(
            r#"
            INSERT INTO projects (id, name, canonical_path, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(canonical_path) DO NOTHING
            "#,
            rusqlite::params![project_id, name, canonical_str, now, now],
        )?;

        let resolved_project_id: String = conn.query_row(
            "SELECT id FROM projects WHERE canonical_path = ?1",
            rusqlite::params![canonical_path.to_string_lossy().to_string()],
            |row| row.get(0),
        )?;

        conn.execute(
            "UPDATE sessions SET project_id = ?1 WHERE project_path = ?2 AND project_id IS NULL",
            rusqlite::params![resolved_project_id, path],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_migration_adds_parent_id_column() {
        // Create a database with old schema (without parent_id column)
        let conn = Connection::open_in_memory().unwrap();

        // Create old schema without parent_id and title
        conn.execute_batch(
            r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'idle',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .unwrap();

        // Run migration
        init_schema(&conn).unwrap();

        // Verify parent_id column exists
        let has_parent_id: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='parent_id'",
                [],
                |row| row.get::<_, i64>(0).map(|c| c > 0),
            )
            .unwrap();
        assert!(
            has_parent_id,
            "parent_id column should exist after migration"
        );

        // Verify title column exists
        let has_title: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='title'",
                [],
                |row| row.get::<_, i64>(0).map(|c| c > 0),
            )
            .unwrap();
        assert!(has_title, "title column should exist after migration");
    }

    #[test]
    fn test_schema_migration_idempotent() {
        // Create a fresh database and run init_schema twice
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_schema(&conn).unwrap(); // Should not error

        // Both columns should exist
        let has_parent_id: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='parent_id'",
                [],
                |row| row.get::<_, i64>(0).map(|c| c > 0),
            )
            .unwrap();
        assert!(has_parent_id);

        let has_title: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='title'",
                [],
                |row| row.get::<_, i64>(0).map(|c| c > 0),
            )
            .unwrap();
        assert!(has_title);
    }

    #[test]
    fn test_schema_default_status_is_idle() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        // Insert a session and check default status
        conn.execute(
            "INSERT INTO sessions (id, project_path, agent_id, model_id, created_at, updated_at) VALUES ('test', '/path', 'agent', 'model', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
            [],
        ).unwrap();

        let status: String = conn
            .query_row("SELECT status FROM sessions WHERE id = 'test'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(status, "idle");
    }

    #[test]
    fn test_schema_creates_projects_table_and_project_id_column() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        let has_projects_table: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='projects'",
                [],
                |row| row.get::<_, i64>(0).map(|c| c > 0),
            )
            .unwrap();
        assert!(has_projects_table);

        let has_project_id: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='project_id'",
                [],
                |row| row.get::<_, i64>(0).map(|c| c > 0),
            )
            .unwrap();
        assert!(has_project_id);
    }

    #[test]
    fn test_schema_backfills_projects_from_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let project_a = dir.path().join("a");
        let project_b = dir.path().join("b");
        std::fs::create_dir_all(&project_a).unwrap();
        std::fs::create_dir_all(&project_b).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'idle',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .unwrap();

        let now = "2024-01-01T00:00:00Z";
        conn.execute(
            "INSERT INTO sessions (id, project_path, agent_id, model_id, created_at, updated_at) VALUES (?1, ?2, 'a', 'm', ?3, ?3)",
            rusqlite::params!["s1", project_a.to_string_lossy().to_string(), now],
        ).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, project_path, agent_id, model_id, created_at, updated_at) VALUES (?1, ?2, 'a', 'm', ?3, ?3)",
            rusqlite::params!["s2", project_b.to_string_lossy().to_string(), now],
        ).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, project_path, agent_id, model_id, created_at, updated_at) VALUES (?1, ?2, 'a', 'm', ?3, ?3)",
            rusqlite::params!["s3", project_a.to_string_lossy().to_string(), now],
        ).unwrap();

        init_schema(&conn).unwrap();

        let project_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(project_count, 2);

        let linked_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE project_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(linked_count, 3);
    }
}
