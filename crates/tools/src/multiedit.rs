//! Multiedit tool - apply multiple edits to a file atomically
//!
//! This tool allows multiple edits to be applied in a single call,
//! ensuring atomicity: all edits succeed or none are applied.

use async_trait::async_trait;
use tokio::fs;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

use crate::edit_fuzzy::{build_diff, FuzzyMatcher};
use crate::file_lock::FileLock;

/// Maximum number of edits per multiedit call
const MAX_EDITS_PER_CALL: usize = 20;

/// Metadata for a single edit in a multiedit operation
#[derive(serde::Serialize)]
struct MultieditEditMetadata {
    index: usize,
    strategy: String,
    confidence: f64,
}

/// Combined diff metadata for multiedit result
#[derive(serde::Serialize)]
struct MultieditMetadata {
    file_path: String,
    edits_applied: usize,
    edit_details: Vec<MultieditEditMetadata>,
    combined_diff: String,
}

/// Multiedit tool - applies multiple edits atomically
pub struct MultieditTool;

impl MultieditTool {
    /// Create a new MultieditTool instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for MultieditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MultieditTool {
    fn id(&self) -> &str {
        "multiedit"
    }

    fn name(&self) -> &str {
        "Multiedit"
    }

    fn description(&self) -> &str {
        "Apply multiple edits to a file atomically. All edits succeed or none are applied."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "edits": {
                    "type": "array",
                    "description": "Array of edits to apply",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_string": {
                                "type": "string",
                                "description": "Text to replace (supports fuzzy matching)"
                            },
                            "new_string": {
                                "type": "string",
                                "description": "Replacement text"
                            }
                        },
                        "required": ["old_string", "new_string"]
                    },
                    "maxItems": MAX_EDITS_PER_CALL
                }
            },
            "required": ["file_path", "edits"]
        })
    }

    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let file_path = args["file_path"].as_str().ok_or_else(|| {
            rcode_core::RCodeError::Tool("Missing required parameter: file_path".to_string())
        })?;

        let edits_array = args["edits"].as_array().ok_or_else(|| {
            rcode_core::RCodeError::Tool("Missing required parameter: edits".to_string())
        })?;

        if edits_array.is_empty() {
            return Err(rcode_core::RCodeError::Tool(
                "No edits to apply. Provide at least one edit.".to_string()
            ));
        }

        if edits_array.len() > MAX_EDITS_PER_CALL {
            return Err(rcode_core::RCodeError::Tool(format!(
                "Too many edits: {} exceeds maximum of {} per call",
                edits_array.len(),
                MAX_EDITS_PER_CALL
            )));
        }

        let full_path = context.cwd.join(file_path);

        // Acquire per-file lock ONCE for all edits
        let _lock = FileLock::lock(&full_path).await;

        // Read file content once
        let original_content = fs::read_to_string(&full_path).await?;

        // Apply all edits sequentially, building the modified content
        let mut current_content = original_content.clone();
        let mut edit_details: Vec<MultieditEditMetadata> = Vec::new();
        let matcher = FuzzyMatcher::new();

        for (index, edit) in edits_array.iter().enumerate() {
            let old_string = edit["old_string"].as_str().ok_or_else(|| {
                rcode_core::RCodeError::Tool(format!(
                    "Edit {}: missing 'old_string' parameter",
                    index
                ))
            })?;

            let new_string = edit["new_string"].as_str().ok_or_else(|| {
                rcode_core::RCodeError::Tool(format!(
                    "Edit {}: missing 'new_string' parameter",
                    index
                ))
            })?;

            // Try to find the old_string in the current content (modified by previous edits)
            let match_result = matcher.find_match(old_string, &current_content);

            let (matched_text, start, end, strategy, confidence) = match match_result {
                Some(result) => (
                    result.matched_text,
                    result.start,
                    result.end,
                    result.strategy,
                    result.confidence,
                ),
                None => {
                    // Atomicity violation: this edit failed, rollback by NOT writing
                    return Err(rcode_core::RCodeError::Tool(format!(
                        "Edit {} failed: '{}' not found in file. No edits were applied (atomic rollback).",
                        index,
                        old_string
                    )));
                }
            };

            // Apply the replacement to current content
            current_content = current_content[..start].to_string()
                + new_string
                + &current_content[end..];

            edit_details.push(MultieditEditMetadata {
                index,
                strategy: strategy.to_string(),
                confidence,
            });
        }

        // All edits matched successfully - now write the modified content
        fs::write(&full_path, &current_content).await?;

        // Build combined diff between original and final content
        let combined_diff = build_diff(&original_content, &current_content);

        let metadata = MultieditMetadata {
            file_path: file_path.to_string(),
            edits_applied: edits_array.len(),
            edit_details,
            combined_diff,
        };

        Ok(ToolResult {
            title: format!("Multiedit: {} edits to {}", edits_array.len(), file_path),
            content: format!(
                "Applied {} edits to {} atomically",
                edits_array.len(),
                file_path
            ),
            metadata: Some(serde_json::to_value(metadata).unwrap()),
            attachments: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;
    use std::path::PathBuf;

    fn ctx(cwd: &str) -> ToolContext {
        ToolContext {
            session_id: "s1".into(),
            project_path: PathBuf::from(cwd),
            cwd: PathBuf::from(cwd),
            user_id: None,
            agent: "test".into(),
        }
    }

    #[tokio::test]
    async fn test_multiedit_single_edit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        let tool = MultieditTool::new();
        let result = tool.execute(
            serde_json::json!({
                "file_path": "test.txt",
                "edits": [{
                    "old_string": "hello",
                    "new_string": "goodbye"
                }]
            }),
            &ctx(dir.path().to_str().unwrap())
        ).await.unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("test.txt")).unwrap(),
            "goodbye world"
        );
        assert!(result.content.contains("Applied 1 edit"));
    }

    #[tokio::test]
    async fn test_multiedit_multiple_edits() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world foo bar").unwrap();
        let tool = MultieditTool::new();
        let result = tool.execute(
            serde_json::json!({
                "file_path": "test.txt",
                "edits": [
                    {"old_string": "hello", "new_string": "hi"},
                    {"old_string": "world", "new_string": "there"},
                    {"old_string": "foo", "new_string": "baz"}
                ]
            }),
            &ctx(dir.path().to_str().unwrap())
        ).await.unwrap();

        // All edits applied in sequence
        assert_eq!(
            std::fs::read_to_string(dir.path().join("test.txt")).unwrap(),
            "hi there baz bar"
        );
        assert!(result.content.contains("Applied 3 edits"));
    }

    #[tokio::test]
    async fn test_multiedit_atomicity_rollback() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        let tool = MultieditTool::new();

        // First edit succeeds, second edit fails - should rollback
        let result = tool.execute(
            serde_json::json!({
                "file_path": "test.txt",
                "edits": [
                    {"old_string": "hello", "new_string": "hi"},
                    {"old_string": "nonexistent", "new_string": "fail"}
                ]
            }),
            &ctx(dir.path().to_str().unwrap())
        ).await;

        // Should return error
        assert!(result.is_err());

        // File should be unchanged (atomic rollback)
        assert_eq!(
            std::fs::read_to_string(dir.path().join("test.txt")).unwrap(),
            "hello world"
        );
    }

    #[tokio::test]
    async fn test_multiedit_empty_edits_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        let tool = MultieditTool::new();
        let result = tool.execute(
            serde_json::json!({
                "file_path": "test.txt",
                "edits": []
            }),
            &ctx(dir.path().to_str().unwrap())
        ).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No edits to apply"));
    }

    #[tokio::test]
    async fn test_multiedit_sequential_editing() {
        // Test that edit N+1 operates on the result of edit N
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "aaa").unwrap();
        let tool = MultieditTool::new();
        let result = tool.execute(
            serde_json::json!({
                "file_path": "test.txt",
                "edits": [
                    {"old_string": "aaa", "new_string": "bbb"},
                    {"old_string": "bbb", "new_string": "ccc"}
                ]
            }),
            &ctx(dir.path().to_str().unwrap())
        ).await.unwrap();

        // First edit: aaa -> bbb, Second edit: bbb -> ccc
        assert_eq!(
            std::fs::read_to_string(dir.path().join("test.txt")).unwrap(),
            "ccc"
        );
        assert!(result.content.contains("Applied 2 edits"));
    }

    #[tokio::test]
    async fn test_multiedit_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let tool = MultieditTool::new();
        let result = tool.execute(
            serde_json::json!({
                "file_path": "nonexistent.txt",
                "edits": [{"old_string": "a", "new_string": "b"}]
            }),
            &ctx(dir.path().to_str().unwrap())
        ).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_multiedit_returns_metadata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        let tool = MultieditTool::new();
        let result = tool.execute(
            serde_json::json!({
                "file_path": "test.txt",
                "edits": [{"old_string": "hello", "new_string": "goodbye"}]
            }),
            &ctx(dir.path().to_str().unwrap())
        ).await.unwrap();

        // Should have metadata with combined_diff
        assert!(result.metadata.is_some());
        let meta = result.metadata.unwrap();
        assert!(meta.get("combined_diff").is_some());
        assert!(meta.get("edits_applied").is_some());
        assert!(meta.get("edit_details").is_some());
    }

    #[tokio::test]
    async fn test_multiedit_fuzzy_matching() {
        let dir = tempfile::tempdir().unwrap();
        // File has tabs, old_string has spaces
        std::fs::write(dir.path().join("test.txt"), "fn hello() {\n\tworld\n}").unwrap();
        let tool = MultieditTool::new();
        let result = tool.execute(
            serde_json::json!({
                "file_path": "test.txt",
                "edits": [{
                    "old_string": "fn hello() {\n    world\n}",
                    "new_string": "fn goodbye() {\n    universe\n}"
                }]
            }),
            &ctx(dir.path().to_str().unwrap())
        ).await;

        // Should succeed due to fuzzy matching
        assert!(result.is_ok(), "Fuzzy matching should succeed: {:?}", result.err());
    }
}