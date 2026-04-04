//! Diff parsing and retrieval routes

use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::state::AppState;
use crate::error::ServerError;
use rcode_core::{Message, Part, SessionId};

/// Response for listing diffs
#[derive(Debug, serde::Serialize)]
pub struct DiffListResponse {
    pub diffs: Vec<DiffSummary>,
}

/// A summary of a diff block
#[derive(Debug, serde::Serialize)]
pub struct DiffSummary {
    pub file: String,
    pub summary: String,
}

/// Response for getting a specific diff
#[derive(Debug, serde::Serialize)]
pub struct DiffContentResponse {
    pub file: String,
    pub content: String,
}

/// Represents a parsed unified diff block
#[derive(Debug, Clone)]
struct ParsedDiff {
    file: String,
    content: String,
}

/// Parse all diffs from message content
fn parse_diffs_from_content(content: &str) -> Vec<ParsedDiff> {
    let mut diffs = Vec::new();
    
    // Simple pattern to detect diff blocks:
    // A diff block starts with lines matching "---" and "+++" or contains "@@"
    let lines: Vec<&str> = content.lines().collect();
    let mut in_diff = false;
    let mut current_file = String::new();
    let mut diff_lines: Vec<String> = Vec::new();
    
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        
        // Check for diff start patterns
        if trimmed.starts_with("--- ") && i + 1 < lines.len() {
            // Check if next line is +++
            if lines[i + 1].trim().starts_with("+++ ") {
                // Start of a new diff
                if in_diff && !diff_lines.is_empty() {
                    // Save previous diff
                    diffs.push(ParsedDiff {
                        file: current_file.clone(),
                        content: diff_lines.join("\n"),
                    });
                }
                in_diff = true;
                diff_lines = vec![line.to_string()];
                
                // Extract filename from --- line
                // Format: --- a/path/to/file or --- path/to/file
                if let Some(file_part) = trimmed.strip_prefix("--- ") {
                    let file = file_part.trim_start_matches('a').trim_start_matches('b').trim();
                    current_file = file.to_string();
                }
                continue;
            }
        }
        
        // Also detect diffs that start with @@@ or contain @@ after first line
        if trimmed.starts_with("@@") && !in_diff {
            in_diff = true;
            diff_lines = vec![line.to_string()];
            current_file = "unknown".to_string();
            continue;
        }
        
        if in_diff {
            diff_lines.push(line.to_string());
            
            // End of diff block: blank line or new diff starts
            if trimmed.is_empty() && diff_lines.len() > 10 {
                // Could be end of diff, check if next lines start a new diff
                if i + 1 < lines.len() {
                    let next = lines[i + 1].trim();
                    if next.starts_with("--- ") || next.starts_with("diff ") || next.starts_with("@@") {
                        diffs.push(ParsedDiff {
                            file: current_file.clone(),
                            content: diff_lines.join("\n"),
                        });
                        diff_lines.clear();
                        in_diff = false;
                        current_file = String::new();
                    }
                }
            }
        }
    }
    
    // Don't forget the last diff
    if in_diff && !diff_lines.is_empty() {
        diffs.push(ParsedDiff {
            file: current_file,
            content: diff_lines.join("\n"),
        });
    }
    
    diffs
}

/// Extract text content from message parts
fn extract_text_from_parts(parts: &[Part]) -> String {
    parts
        .iter()
        .filter_map(|part| {
            if let Part::Text { content } = part {
                Some(content.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract all diffs from session messages
fn extract_diffs_from_messages(messages: &[Message]) -> Vec<ParsedDiff> {
    let mut all_diffs = Vec::new();
    
    for message in messages {
        let content = extract_text_from_parts(&message.parts);
        if !content.is_empty() {
            let diffs = parse_diffs_from_content(&content);
            all_diffs.extend(diffs);
        }
    }
    
    all_diffs
}

/// GET /session/:id/diffs - List all diffs in a session
pub async fn list_diffs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<DiffListResponse>, ServerError> {
    // Check if session exists
    let _session = state.session_service.get(&SessionId(id.clone()))
        .ok_or_else(|| ServerError::not_found())?;
    
    // Get all messages for the session
    let messages = state.session_service.get_messages(&id);
    
    // Extract diffs
    let parsed_diffs = extract_diffs_from_messages(&messages);
    
    // Build response
    let diffs: Vec<DiffSummary> = parsed_diffs
        .into_iter()
        .map(|d| {
            // Generate a summary from the first few lines
            let summary_lines: Vec<&str> = d.content.lines().take(5).collect();
            let summary = if summary_lines.len() < d.content.lines().count() {
                format!("{}...\n[{} more lines]", summary_lines.join("\n"), d.content.lines().count() - 5)
            } else {
                summary_lines.join("\n")
            };
            
            DiffSummary {
                file: d.file,
                summary,
            }
        })
        .collect();
    
    Ok(Json(DiffListResponse { diffs }))
}

/// GET /session/:id/diff/:file - Get diff content for a specific file
pub async fn get_diff(
    State(state): State<Arc<AppState>>,
    Path((id, file)): Path<(String, String)>,
) -> Result<Json<DiffContentResponse>, ServerError> {
    // Check if session exists
    let _session = state.session_service.get(&SessionId(id.clone()))
        .ok_or_else(|| ServerError::not_found())?;
    
    // Get all messages for the session
    let messages = state.session_service.get_messages(&id);
    
    // Extract diffs
    let parsed_diffs = extract_diffs_from_messages(&messages);
    
    // Find diff for the specified file
    let diff = parsed_diffs
        .into_iter()
        .find(|d| d.file == file || d.file.contains(&file) || file.contains(&d.file))
        .ok_or_else(|| ServerError::not_found())?;
    
    Ok(Json(DiffContentResponse {
        file: diff.file,
        content: diff.content,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{Message, Part, Role};

    #[test]
    fn test_parse_diffs_from_content_simple() {
        let content = r#"Here is a diff:

--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,6 @@
 fn main() {
     println!("Hello");
+    println!("World");
 }
"#;
        let diffs = parse_diffs_from_content(content);
        assert!(!diffs.is_empty());
    }

    #[test]
    fn test_parse_diffs_from_content_git_diff() {
        let content = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1234567..abcdefg 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,6 +10,7 @@ pub fn test() {
     let x = 1;
+    let y = 2;
     let z = 3;
 }"#;
        let diffs = parse_diffs_from_content(content);
        assert!(!diffs.is_empty());
    }

    #[test]
    fn test_parse_diffs_from_content_no_diffs() {
        let content = "This is just regular text without any diff content.";
        let diffs = parse_diffs_from_content(content);
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_parse_diffs_from_content_empty() {
        let diffs = parse_diffs_from_content("");
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_extract_text_from_parts() {
        let parts = vec![
            Part::Text { content: "Hello".to_string() },
            Part::Text { content: "World".to_string() },
        ];
        let text = extract_text_from_parts(&parts);
        assert_eq!(text, "Hello\nWorld");
    }

    #[test]
    fn test_extract_text_from_parts_with_tool_call() {
        let parts = vec![
            Part::Text { content: "Before".to_string() },
            Part::ToolCall {
                id: "call_1".to_string(),
                name: "bash".to_string(),
                arguments: Box::new(serde_json::json!("ls")),
            },
            Part::Text { content: "After".to_string() },
        ];
        let text = extract_text_from_parts(&parts);
        assert_eq!(text, "Before\nAfter");
    }

    #[test]
    fn test_extract_diffs_from_messages() {
        let messages = vec![
            Message {
                id: rcode_core::MessageId::new(),
                session_id: "test".to_string(),
                role: Role::User,
                parts: vec![Part::Text { content: "Please review this diff".to_string() }],
                created_at: chrono::Utc::now(),
            },
            Message {
                id: rcode_core::MessageId::new(),
                session_id: "test".to_string(),
                role: Role::Assistant,
                parts: vec![Part::Text { content: r#"--- a/file1.rs
+++ b/file1.rs
@@ -1,3 +1,4 @@
 fn test() {
+    let x = 1;
 }"#.to_string() }],
                created_at: chrono::Utc::now(),
            },
        ];
        
        let diffs = extract_diffs_from_messages(&messages);
        assert!(!diffs.is_empty());
    }

    #[test]
    fn test_extract_diffs_from_messages_no_diffs() {
        let messages = vec![
            Message {
                id: rcode_core::MessageId::new(),
                session_id: "test".to_string(),
                role: Role::User,
                parts: vec![Part::Text { content: "Hello world".to_string() }],
                created_at: chrono::Utc::now(),
            },
        ];
        
        let diffs = extract_diffs_from_messages(&messages);
        assert!(diffs.is_empty());
    }
}
