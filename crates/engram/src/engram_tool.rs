//! Engram tool for agent integration

use crate::client::EngramClient;
use crate::types::Observation;
use async_trait::async_trait;
use opencode_core::{Tool, ToolContext, ToolResult};
use serde_json::Value;
use std::sync::Arc;

/// Tool for saving and searching persistent memory
pub struct EngramTool {
    client: Arc<EngramClient>,
}

impl EngramTool {
    /// Create a new EngramTool with the given client
    pub fn new(client: Arc<EngramClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for EngramTool {
    fn id(&self) -> &str {
        "engram"
    }

    fn name(&self) -> &str {
        "Persistent Memory"
    }

    fn description(&self) -> &str {
        "Save and search persistent memory observations. Use 'save' to store decisions, discoveries, and patterns. Use 'search' to find relevant context. Use 'context' to get recent observations."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform: 'save', 'search', 'context', 'get', 'update', 'delete', 'topic'"
                },
                "title": {
                    "type": "string",
                    "description": "Title for the observation (for save action)"
                },
                "content": {
                    "type": "string",
                    "description": "Content of the observation (for save action)"
                },
                "type": {
                    "type": "string",
                    "description": "Type of observation: decision, architecture, bugfix, pattern, config, discovery, learning, tool_use, file_change, command, search, manual (default: discovery)"
                },
                "topic_key": {
                    "type": "string",
                    "description": "Topic key to filter by (for topic action or when saving)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search action)"
                },
                "id": {
                    "type": "integer",
                    "description": "Observation ID (for get, update, delete actions)"
                },
                "scope": {
                    "type": "string",
                    "description": "Scope: project or personal (default: project)"
                },
                "project": {
                    "type": "string",
                    "description": "Project name to associate with the observation"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 10)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolResult, opencode_core::OpenCodeError> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'action' argument".into()))?;

        match action {
            "save" => {
                let title = args["title"]
                    .as_str()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'title' argument".into()))?;
                let content = args["content"]
                    .as_str()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'content' argument".into()))?;
                let obs_type = args["type"]
                    .as_str()
                    .unwrap_or("discovery")
                    .parse()
                    .map_err(|e: String| opencode_core::OpenCodeError::Tool(format!("Invalid type: {}", e)))?;
                let scope = args["scope"]
                    .as_str()
                    .unwrap_or("project")
                    .parse()
                    .map_err(|e: String| opencode_core::OpenCodeError::Tool(format!("Invalid scope: {}", e)))?;
                let topic = args["topic_key"].as_str().map(String::from);
                let project = args["project"].as_str().map(String::from);

                let mut obs = Observation::new(title.to_string(), content.to_string(), obs_type);
                obs.scope = scope;
                if let Some(t) = topic {
                    obs = obs.with_topic(t);
                }
                if let Some(p) = project {
                    obs = obs.with_project(p);
                } else {
                    // Auto-associate with current project
                    obs = obs.with_project(context.project_path.to_string_lossy().to_string());
                }
                obs = obs.with_session(context.session_id.clone());

                let id = self.client.save(obs).await
                    .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to save: {}", e)))?;

                Ok(ToolResult {
                    title: "Observation saved".to_string(),
                    content: format!("Saved as observation #{}", id),
                    metadata: Some(serde_json::json!({ "id": id })),
                    attachments: vec![],
                })
            }

            "search" => {
                let query = args["query"]
                    .as_str()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'query' argument".into()))?;
                let limit = args["limit"]
                    .as_i64()
                    .unwrap_or(10) as usize;

                let results = self.client.search(query, limit).await
                    .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to search: {}", e)))?;

                if results.is_empty() {
                    return Ok(ToolResult {
                        title: "Search results".to_string(),
                        content: "No observations found matching your query.".to_string(),
                        metadata: None,
                        attachments: vec![],
                    });
                }

                let summary = results
                    .iter()
                    .map(|obs| {
                        format!(
                            "## {} [{}]\n{}\n---\n",
                            obs.title,
                            obs.obs_type,
                            obs.content
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(ToolResult {
                    title: format!("{} search results", results.len()),
                    content: summary,
                    metadata: Some(serde_json::json!({
                        "count": results.len(),
                        "query": query
                    })),
                    attachments: vec![],
                })
            }

            "context" => {
                let limit = args["limit"]
                    .as_i64()
                    .unwrap_or(10) as usize;

                let observations = self.client.get_context(limit).await
                    .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to get context: {}", e)))?;

                if observations.is_empty() {
                    return Ok(ToolResult {
                        title: "Recent context".to_string(),
                        content: "No observations in memory yet.".to_string(),
                        metadata: None,
                        attachments: vec![],
                    });
                }

                let summary = observations
                    .iter()
                    .map(|obs| {
                        format!(
                            "## {} [{}]\n{}\n---\n",
                            obs.title,
                            obs.obs_type,
                            obs.content
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(ToolResult {
                    title: format!("{} recent observations", observations.len()),
                    content: summary,
                    metadata: Some(serde_json::json!({
                        "count": observations.len()
                    })),
                    attachments: vec![],
                })
            }

            "get" => {
                let id = args["id"]
                    .as_i64()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'id' argument".into()))?;

                let obs = self.client.get(id).await
                    .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to get: {}", e)))?;

                match obs {
                    Some(obs) => Ok(ToolResult {
                        title: obs.title.clone(),
                        content: format!("## {}\n\n**Type:** {}\n**Scope:** {}\n\n{}",
                            obs.title,
                            obs.obs_type,
                            obs.scope,
                            obs.content
                        ),
                        metadata: Some(serde_json::json!({
                            "id": id,
                            "type": obs.obs_type.to_string(),
                            "scope": obs.scope.to_string(),
                            "topic_key": obs.topic_key,
                            "project": obs.project,
                            "created_at": obs.created_at,
                            "updated_at": obs.updated_at
                        })),
                        attachments: vec![],
                    }),
                    None => Err(opencode_core::OpenCodeError::Tool(format!("Observation {} not found", id)))
                }
            }

            "update" => {
                let id = args["id"]
                    .as_i64()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'id' argument".into()))?;
                let title = args["title"]
                    .as_str()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'title' argument".into()))?;
                let content = args["content"]
                    .as_str()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'content' argument".into()))?;
                let obs_type = args["type"]
                    .as_str()
                    .unwrap_or("discovery")
                    .parse()
                    .map_err(|e: String| opencode_core::OpenCodeError::Tool(format!("Invalid type: {}", e)))?;

                let obs = Observation::new(title.to_string(), content.to_string(), obs_type);
                self.client.update(id, obs).await
                    .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to update: {}", e)))?;

                Ok(ToolResult {
                    title: "Observation updated".to_string(),
                    content: format!("Observation #{} has been updated", id),
                    metadata: Some(serde_json::json!({ "id": id })),
                    attachments: vec![],
                })
            }

            "delete" => {
                let id = args["id"]
                    .as_i64()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'id' argument".into()))?;

                self.client.delete(id).await
                    .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to delete: {}", e)))?;

                Ok(ToolResult {
                    title: "Observation deleted".to_string(),
                    content: format!("Observation #{} has been deleted", id),
                    metadata: Some(serde_json::json!({ "id": id })),
                    attachments: vec![],
                })
            }

            "topic" => {
                let topic = args["topic_key"]
                    .as_str()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'topic_key' argument".into()))?;

                let observations = self.client.get_topic(topic).await
                    .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to get topic: {}", e)))?;

                if observations.is_empty() {
                    return Ok(ToolResult {
                        title: "Topic results".to_string(),
                        content: format!("No observations found for topic '{}'", topic),
                        metadata: Some(serde_json::json!({
                            "topic": topic,
                            "count": 0
                        })),
                        attachments: vec![],
                    });
                }

                let summary = observations
                    .iter()
                    .map(|obs| {
                        format!(
                            "## {} [{}]\n{}\n---\n",
                            obs.title,
                            obs.obs_type,
                            obs.content
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(ToolResult {
                    title: format!("Topic '{}': {} observations", topic, observations.len()),
                    content: summary,
                    metadata: Some(serde_json::json!({
                        "topic": topic,
                        "count": observations.len()
                    })),
                    attachments: vec![],
                })
            }

            _ => Err(opencode_core::OpenCodeError::Tool(
                format!("Unknown action: '{}'. Use: save, search, context, get, update, delete, topic", action)
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use opencode_core::ToolContext;

    fn create_test_context() -> ToolContext {
        ToolContext {
            session_id: "test-session".to_string(),
            project_path: std::path::PathBuf::from("/test/project"),
            cwd: std::path::PathBuf::from("/test"),
            user_id: None,
        }
    }

    fn create_test_tool() -> (EngramTool, TempDir) {
        let temp = TempDir::new().unwrap();
        let client = Arc::new(
            EngramClient::new(&temp.path().join("test.db")).unwrap()
        );
        (EngramTool::new(client), temp)
    }

    #[tokio::test]
    async fn test_save_observation() {
        let (tool, _dir) = create_test_tool();
        let ctx = create_test_context();

        let args = serde_json::json!({
            "action": "save",
            "title": "Rust error handling",
            "content": "Use thiserror for custom error types",
            "type": "pattern",
            "scope": "project"
        });

        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(result.content.contains("#1") || result.content.contains("saved"));
    }

    #[tokio::test]
    async fn test_search() {
        let (tool, _dir) = create_test_tool();
        let ctx = create_test_context();

        // First save something
        let save_args = serde_json::json!({
            "action": "save",
            "title": "SQLite is great",
            "content": "Use SQLite for local storage",
            "type": "decision"
        });
        tool.execute(save_args, &ctx).await.unwrap();

        // Then search
        let search_args = serde_json::json!({
            "action": "search",
            "query": "SQLite"
        });

        let result = tool.execute(search_args, &ctx).await.unwrap();
        assert!(result.content.contains("SQLite"));
    }

    #[tokio::test]
    async fn test_context_action() {
        let (tool, _dir) = create_test_tool();
        let ctx = create_test_context();

        // Save a few observations
        for i in 0..3 {
            let args = serde_json::json!({
                "action": "save",
                "title": format!("Title {}", i),
                "content": format!("Content {}", i),
                "type": "discovery"
            });
            tool.execute(args, &ctx).await.unwrap();
        }

        let context_args = serde_json::json!({
            "action": "context",
            "limit": 10
        });

        let result = tool.execute(context_args, &ctx).await.unwrap();
        assert!(result.content.contains("Title 0") || result.content.contains("Title 1"));
    }

    #[tokio::test]
    async fn test_get_update_delete() {
        let (tool, _dir) = create_test_tool();
        let ctx = create_test_context();

        // Save
        let save_args = serde_json::json!({
            "action": "save",
            "title": "Original",
            "content": "Original content",
            "type": "decision"
        });
        let result = tool.execute(save_args, &ctx).await.unwrap();
        let id = result.metadata.as_ref().unwrap()["id"].as_i64().unwrap();

        // Get
        let get_args = serde_json::json!({
            "action": "get",
            "id": id
        });
        let get_result = tool.execute(get_args, &ctx).await.unwrap();
        assert!(get_result.content.contains("Original"));

        // Update
        let update_args = serde_json::json!({
            "action": "update",
            "id": id,
            "title": "Updated",
            "content": "Updated content",
            "type": "decision"
        });
        tool.execute(update_args, &ctx).await.unwrap();

        // Verify update
        let get_args2 = serde_json::json!({
            "action": "get",
            "id": id
        });
        let get_result2 = tool.execute(get_args2, &ctx).await.unwrap();
        assert!(get_result2.content.contains("Updated"));

        // Delete
        let delete_args = serde_json::json!({
            "action": "delete",
            "id": id
        });
        tool.execute(delete_args, &ctx).await.unwrap();

        // Verify delete
        let get_args3 = serde_json::json!({
            "action": "get",
            "id": id
        });
        let get_result3 = tool.execute(get_args3, &ctx).await;
        assert!(get_result3.is_err());
    }
}
