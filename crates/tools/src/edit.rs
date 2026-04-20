//! Edit tool - in-place file editing with fuzzy matching
//!
//! This tool supports fuzzy matching of old_text against file content,
//! handling whitespace differences, indentation variations, and minor typos.

use async_trait::async_trait;
use tokio::fs;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

use crate::edit_fuzzy::{build_diff, FuzzyMatcher, MatchResult};
use crate::file_lock::FileLock;

pub struct EditTool;

impl EditTool {
    pub fn new() -> Self { Self }
}

impl Default for EditTool {
    fn default() -> Self { Self::new() }
}

/// Edit metadata returned in ToolResult
#[derive(serde::Serialize)]
struct EditMetadata {
    strategy: String,
    confidence: f64,
    diff: String,
}

#[async_trait]
impl Tool for EditTool {
    fn id(&self) -> &str { "edit" }
    fn name(&self) -> &str { "Edit" }
    fn description(&self) -> &str { "Edit a file by replacing text (supports fuzzy matching)" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to edit"
                },
                "old_text": {
                    "type": "string",
                    "description": "Text to replace (supports fuzzy matching)"
                },
                "new_text": {
                    "type": "string",
                    "description": "Replacement text"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let path = args["path"].as_str().unwrap();
        let old_text = args["old_text"].as_str().unwrap();
        let new_text = args["new_text"].as_str().unwrap();

        let full_path = context.cwd.join(path);

        // Acquire per-file lock before editing
        let _lock = FileLock::lock(&full_path).await;

        let content = fs::read_to_string(&full_path).await?;

        // Try fuzzy matching strategies in order
        let matcher = FuzzyMatcher::new();
        let strategies_tried = matcher.strategy_names();

        let match_result = matcher.find_match(old_text, &content);

        let (matched_text, start, end, strategy, confidence) = match match_result {
            Some(result) => (result.matched_text, result.start, result.end, result.strategy, result.confidence),
            None => {
                return Err(rcode_core::RCodeError::Tool(format!(
                    "Text not found in file '{}'. Tried strategies: {:?}. File content may differ significantly from old_text.",
                    path,
                    strategies_tried
                )));
            }
        };

        // Build diff before modification
        let diff = build_diff(&matched_text, new_text);

        // Perform replacement using the actual matched text (from content)
        // and only replace the first occurrence
        let new_content = content[..start].to_string()
            + new_text
            + &content[end..];

        fs::write(&full_path, &new_content).await?;

        // Build metadata
        let metadata = EditMetadata {
            strategy: strategy.to_string(),
            confidence,
            diff,
        };

        Ok(ToolResult {
            title: format!("Edited: {}", path),
            content: format!("Replaced {} with {} in {} (using {} strategy, confidence {:.2})",
                matched_text.len(), new_text.len(), path, strategy, confidence),
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
        ToolContext { session_id: "s1".into(), project_path: PathBuf::from(cwd), cwd: PathBuf::from(cwd), user_id: None, agent: "test".into() }
    }

    #[tokio::test]
    async fn test_edit_replace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        let tool = EditTool::new();
        let result = tool.execute(serde_json::json!({"path": "test.txt", "old_text": "hello", "new_text": "goodbye"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("test.txt")).unwrap(), "goodbye world");
        assert!(result.content.contains("Replaced"));
    }

    #[tokio::test]
    async fn test_edit_text_not_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        let tool = EditTool::new();
        let result = tool.execute(serde_json::json!({"path": "test.txt", "old_text": "completely different text", "new_text": "x"}), &ctx(dir.path().to_str().unwrap())).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_nonexistent_file() {
        let tool = EditTool::new();
        let result = tool.execute(serde_json::json!({"path": "nope.txt", "old_text": "a", "new_text": "b"}), &ctx("/tmp")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_replacen_first_occurrence_only() {
        // Regression test: replace() replaces ALL, replacen() replaces only first
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello hello hello").unwrap();
        let tool = EditTool::new();
        let result = tool.execute(
            serde_json::json!({"path": "test.txt", "old_text": "hello", "new_text": "hi"}),
            &ctx(dir.path().to_str().unwrap())
        ).await.unwrap();
        // Should only replace FIRST occurrence, not all three
        assert_eq!(std::fs::read_to_string(dir.path().join("test.txt")).unwrap(), "hi hello hello");
    }

    #[tokio::test]
    async fn test_edit_with_fuzzy_matching() {
        let dir = tempfile::tempdir().unwrap();
        // File has tabs, old_text has spaces
        std::fs::write(dir.path().join("test.txt"), "fn hello() {\n\tworld\n}").unwrap();
        let tool = EditTool::new();
        let result = tool.execute(
            serde_json::json!({"path": "test.txt", "old_text": "fn hello() {\n    world\n}", "new_text": "fn goodbye() {\n    universe\n}"}),
            &ctx(dir.path().to_str().unwrap())
        ).await;
        // Should succeed due to whitespace normalization
        assert!(result.is_ok(), "Fuzzy matching should succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_edit_returns_metadata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        let tool = EditTool::new();
        let result = tool.execute(
            serde_json::json!({"path": "test.txt", "old_text": "hello", "new_text": "goodbye"}),
            &ctx(dir.path().to_str().unwrap())
        ).await.unwrap();

        // Should have metadata with diff
        assert!(result.metadata.is_some());
        let meta = result.metadata.unwrap();
        assert!(meta.get("diff").is_some());
        assert!(meta.get("strategy").is_some());
        assert!(meta.get("confidence").is_some());
    }
}
