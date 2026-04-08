//! Database schema definitions

use rusqlite::{Connection, Result};

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            project_path TEXT NOT NULL,
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
}
