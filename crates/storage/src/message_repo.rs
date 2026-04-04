//! Message repository for SQLite persistence

use anyhow::{Context, Result};
use base64::Engine;
use rusqlite::{params, Connection};
use std::sync::Mutex;

use crate::StorageError;
use rcode_core::{Message, MessageId, PaginatedMessages, PaginationParams, Part, Role};

pub struct MessageRepository {
    conn: Mutex<Connection>,
}

impl MessageRepository {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    pub fn save_message(&self, session_id: &str, message: &Message) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;

        // Start transaction
        conn.execute("BEGIN TRANSACTION", [])?;

        let save_result = (|| {
            // Save message
            conn.execute(
                r#"
                INSERT INTO messages (id, session_id, role, created_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(id) DO UPDATE SET
                    session_id = excluded.session_id,
                    role = excluded.role,
                    created_at = excluded.created_at
                "#,
                params![
                    message.id.0,
                    session_id,
                    serde_json::to_string(&message.role)?,
                    message.created_at.to_rfc3339(),
                ],
            )
            .context("Failed to save message")?;

            // Delete existing parts for this message (for update scenario)
            conn.execute(
                "DELETE FROM message_parts WHERE message_id = ?1",
                params![message.id.0],
            )?;

            // Save each part
            for part in &message.parts {
                let (part_type, content, tool_call_id, tool_call_name, is_error) = match part {
                    Part::Text { content } => {
                        ("text".to_string(), Some(content.clone()), None, None, false)
                    }
                    Part::ToolCall {
                        id,
                        name,
                        arguments,
                    } => (
                        "tool_call".to_string(),
                        Some(serde_json::to_string(arguments)?),
                        Some(id.clone()),
                        Some(name.clone()),
                        false,
                    ),
                    Part::ToolResult {
                        tool_call_id,
                        content,
                        is_error,
                    } => (
                        "tool_result".to_string(),
                        Some(content.clone()),
                        Some(tool_call_id.clone()),
                        None,
                        *is_error,
                    ),
                    Part::Reasoning { content } => (
                        "reasoning".to_string(),
                        Some(content.clone()),
                        None,
                        None,
                        false,
                    ),
                    Part::Attachment {
                        id,
                        name,
                        mime_type,
                        content,
                    } => (
                        "attachment".to_string(),
                        Some(serde_json::to_string(&serde_json::json!({
                            "id": id,
                            "name": name,
                            "mime_type": mime_type,
                            "content": base64::engine::general_purpose::STANDARD.encode(content)
                        }))?),
                        None,
                        None,
                        false,
                    ),
                };

                conn.execute(
                    r#"
                    INSERT INTO message_parts (message_id, part_type, content, tool_call_id, tool_call_name, is_error)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    "#,
                    params![
                        message.id.0,
                        part_type,
                        content,
                        tool_call_id,
                        tool_call_name,
                        is_error as i32,
                    ],
                )
                .context("Failed to save message part")?;
            }

            Ok::<(), anyhow::Error>(())
        })();

        match save_result {
            Ok(()) => {
                conn.execute("COMMIT", [])?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    pub fn load_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, role, created_at
                FROM messages
                WHERE session_id = ?1
                ORDER BY created_at ASC
                "#,
            )
            .context("Failed to prepare statement")?;

        // First collect the raw message data
        let raw_messages: Vec<(String, String, String)> = stmt
            .query_map(params![session_id], |row| {
                let id: String = row.get(0)?;
                let role_str: String = row.get(1)?;
                let created_at_str: String = row.get(2)?;
                Ok((id, role_str, created_at_str))
            })?
            .filter_map(|r| r.ok())
            .collect();

        drop(stmt);

        // Now build messages with parts
        let messages: Vec<Message> = raw_messages
            .into_iter()
            .map(|(id, role_str, created_at_str)| {
                let role: Role = serde_json::from_str(&role_str).unwrap_or(Role::User);
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| StorageError::InvalidTimestamp(e.to_string()))?
                    .with_timezone(&chrono::Utc);
                let parts = self.load_parts_for_message(&conn, &id);
                Ok::<Message, StorageError>(Message {
                    id: MessageId(id),
                    session_id: session_id.to_string(),
                    role,
                    parts,
                    created_at,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    fn load_parts_for_message(&self, conn: &Connection, message_id: &str) -> Vec<Part> {
        let mut stmt = match conn.prepare(
            r#"
            SELECT part_type, content, tool_call_id, tool_call_name, is_error
            FROM message_parts
            WHERE message_id = ?1
            "#,
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let mut parts = Vec::new();
        let mut rows = match stmt.query(params![message_id]) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        while let Ok(Some(row)) = rows.next() {
            let part_type: String = match row.get::<_, String>(0) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let content: Option<String> = row.get(1).ok();
            let tool_call_id: Option<String> = row.get(2).ok();
            let tool_call_name: Option<String> = row.get(3).ok();
            let is_error: i32 = row.get(4).unwrap_or(0);

            let part: Option<Part> = match part_type.as_str() {
                "text" => content.clone().map(|c| Part::Text { content: c }),
                "tool_call" => {
                    let args: serde_json::Value = content
                        .as_ref()
                        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
                        .unwrap_or(serde_json::Value::Null);
                    // Use and_then to properly handle Option chaining
                    tool_call_id.as_ref().and_then(|t| {
                        tool_call_name.as_ref().map(|n| Part::ToolCall {
                            id: t.clone(),
                            name: n.clone(),
                            arguments: Box::new(args),
                        })
                    })
                }
                "tool_result" => {
                    let cont = content.unwrap_or_default();
                    tool_call_id.as_ref().map(|t| Part::ToolResult {
                        tool_call_id: t.clone(),
                        content: cont,
                        is_error: is_error != 0,
                    })
                }
                "reasoning" => content.clone().map(|c| Part::Reasoning { content: c }),
                _ => None,
            };

            if let Some(p) = part {
                parts.push(p);
            }
        }

        parts
    }

    pub fn get_messages_paginated(
        &self,
        session_id: &str,
        pagination: &PaginationParams,
    ) -> Result<PaginatedMessages> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;

        // Get total count
        let total: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Get paginated messages
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, role, created_at
                FROM messages
                WHERE session_id = ?1
                ORDER BY created_at ASC
                LIMIT ?2 OFFSET ?3
                "#,
            )
            .context("Failed to prepare statement")?;

        let raw_messages: Vec<(String, String, String)> = stmt
            .query_map(
                params![
                    session_id,
                    pagination.limit as i64,
                    pagination.offset as i64
                ],
                |row| {
                    let id: String = row.get(0)?;
                    let role_str: String = row.get(1)?;
                    let created_at_str: String = row.get(2)?;
                    Ok((id, role_str, created_at_str))
                },
            )?
            .filter_map(|r| r.ok())
            .collect();

        drop(stmt);

        // Now build messages with parts
        let messages: Vec<Message> = raw_messages
            .into_iter()
            .map(|(id, role_str, created_at_str)| {
                let role: Role = serde_json::from_str(&role_str).unwrap_or(Role::User);
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| StorageError::InvalidTimestamp(e.to_string()))?
                    .with_timezone(&chrono::Utc);
                let parts = self.load_parts_for_message(&conn, &id);
                Ok::<Message, StorageError>(Message {
                    id: MessageId(id),
                    session_id: session_id.to_string(),
                    role,
                    parts,
                    created_at,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PaginatedMessages {
            messages,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
        })
    }

    pub fn delete_messages_for_session(&self, session_id: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;

        // First delete all parts for messages in this session
        conn.execute(
            r#"
            DELETE FROM message_parts 
            WHERE message_id IN (SELECT id FROM messages WHERE session_id = ?1)
            "#,
            params![session_id],
        )?;

        // Then delete all messages
        conn.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_id],
        )?;

        Ok(())
    }

    /// Delete a single message by its ID
    pub fn delete_message(&self, message_id: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;

        // First delete all parts for this message
        conn.execute(
            "DELETE FROM message_parts WHERE message_id = ?1",
            params![message_id],
        )?;

        // Then delete the message
        conn.execute("DELETE FROM messages WHERE id = ?1", params![message_id])?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use tempfile::tempdir;

    fn create_test_repo() -> (MessageRepository, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        schema::init_schema(&conn).unwrap();

        // Create a test session first
        conn.execute(
            r#"
            INSERT INTO sessions (id, project_path, agent_id, model_id, status, created_at, updated_at)
            VALUES ('test-session', '/test', 'agent', 'model', '"idle"', datetime('now'), datetime('now'))
            "#,
            [],
        )
        .unwrap();

        (MessageRepository::new(conn), dir)
    }

    #[test]
    fn test_save_and_load_message() {
        let (repo, _dir) = create_test_repo();

        let message = Message::user(
            "test-session".to_string(),
            vec![
                Part::Text {
                    content: "Hello".to_string(),
                },
                Part::Text {
                    content: "World".to_string(),
                },
            ],
        );

        repo.save_message("test-session", &message).unwrap();
        let messages = repo.load_messages("test-session").unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].parts.len(), 2);
        assert!(matches!(messages[0].parts[0], Part::Text { .. }));
    }

    #[test]
    fn test_save_message_with_tool_call() {
        let (repo, _dir) = create_test_repo();

        let message = Message::assistant(
            "test-session".to_string(),
            vec![
                Part::ToolCall {
                    id: "tool_1".to_string(),
                    name: "read_file".to_string(),
                    arguments: Box::new(serde_json::json!({"path": "/test.txt"})),
                },
                Part::ToolResult {
                    tool_call_id: "tool_1".to_string(),
                    content: "file contents".to_string(),
                    is_error: false,
                },
            ],
        );

        repo.save_message("test-session", &message).unwrap();
        let messages = repo.load_messages("test-session").unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].parts.len(), 2);
    }

    #[test]
    fn test_paginated_messages() {
        let (repo, _dir) = create_test_repo();

        // Create 5 messages
        for i in 0..5 {
            let message = Message::user(
                "test-session".to_string(),
                vec![Part::Text {
                    content: format!("Message {}", i),
                }],
            );
            repo.save_message("test-session", &message).unwrap();
        }

        // Get first page
        let pagination = PaginationParams::new(0, 2);
        let result = repo
            .get_messages_paginated("test-session", &pagination)
            .unwrap();

        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.total, 5);
        assert_eq!(result.offset, 0);
        assert_eq!(result.limit, 2);

        // Get second page
        let pagination = PaginationParams::new(2, 2);
        let result = repo
            .get_messages_paginated("test-session", &pagination)
            .unwrap();

        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.offset, 2);

        // Get last page
        let pagination = PaginationParams::new(4, 2);
        let result = repo
            .get_messages_paginated("test-session", &pagination)
            .unwrap();

        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.offset, 4);
    }

    #[test]
    fn test_delete_messages_for_session() {
        let (repo, _dir) = create_test_repo();

        let message = Message::user(
            "test-session".to_string(),
            vec![Part::Text {
                content: "Test".to_string(),
            }],
        );
        repo.save_message("test-session", &message).unwrap();

        assert_eq!(repo.load_messages("test-session").unwrap().len(), 1);

        repo.delete_messages_for_session("test-session").unwrap();

        assert_eq!(repo.load_messages("test-session").unwrap().len(), 0);
    }
}
