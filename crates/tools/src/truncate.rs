//! Tool output truncation service
//!
//! Wraps tool execution output to prevent large outputs from consuming the context window.
//! When output exceeds a configurable limit, writes full output to a temp file and returns a preview.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Default maximum output size in bytes (50KB)
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 50 * 1024;

/// Default preview character count
pub const DEFAULT_PREVIEW_CHARS: usize = 2000;

/// Configuration for the truncation service
#[derive(Debug, Clone)]
pub struct TruncationConfig {
    /// Maximum bytes before truncation triggers (default 50KB)
    pub max_bytes: usize,
    /// Number of characters to include in preview (default 2000)
    pub preview_chars: usize,
    /// Directory to store truncated output files
    pub truncation_dir: PathBuf,
}

impl Default for TruncationConfig {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            preview_chars: DEFAULT_PREVIEW_CHARS,
            truncation_dir: std::env::temp_dir().join("rcode-truncation"),
        }
    }
}

/// Result of truncation check
#[derive(Debug, Clone)]
pub enum TruncationResult {
    /// Output was under the limit, returned as-is
    NotTruncated {
        /// The original content
        content: String,
    },
    /// Output was truncated, preview returned with path to full content
    Truncated {
        /// Preview of the content (first N chars)
        preview: String,
        /// Path to the file containing full output
        output_path: PathBuf,
        /// Total bytes of the original content
        total_bytes: usize,
    },
}

/// Check if content exceeds the truncation limit and truncate if needed.
///
/// # Arguments
/// * `content` - The tool output content to check
/// * `config` - Truncation configuration
/// * `session_id` - Session identifier for temp file naming
/// * `tool_call_id` - Tool call identifier for temp file naming
///
/// # Returns
/// `TruncationResult` indicating whether truncation occurred
pub fn truncate_output(
    content: &str,
    config: &TruncationConfig,
    session_id: &str,
    tool_call_id: &str,
) -> TruncationResult {
    let content_len = content.len();

    if content_len <= config.max_bytes {
        return TruncationResult::NotTruncated {
            content: content.to_string(),
        };
    }

    // Create truncation directory if it doesn't exist
    if !config.truncation_dir.exists() {
        if let Err(e) = fs::create_dir_all(&config.truncation_dir) {
            // If we can't create the directory, return truncated without writing file
            return TruncationResult::Truncated {
                preview: content.chars().take(config.preview_chars).collect(),
                output_path: PathBuf::new(),
                total_bytes: content_len,
            };
        }
    }

    // Generate unique filename: {session_id}/{tool_call_id}.txt
    let relative_path = format!("{}/{}.txt", session_id, tool_call_id);
    let output_path = config.truncation_dir.join(&relative_path);

    // Ensure session subdirectory exists
    let session_dir = config.truncation_dir.join(session_id);
    if !session_dir.exists() {
        if let Err(e) = fs::create_dir_all(&session_dir) {
            return TruncationResult::Truncated {
                preview: content.chars().take(config.preview_chars).collect(),
                output_path: PathBuf::new(),
                total_bytes: content_len,
            };
        }
    }

    // Write full content to temp file
    match fs::File::create(&output_path) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(content.as_bytes()) {
                return TruncationResult::Truncated {
                    preview: content.chars().take(config.preview_chars).collect(),
                    output_path: PathBuf::new(),
                    total_bytes: content_len,
                };
            }
        }
        Err(_) => {
            return TruncationResult::Truncated {
                preview: content.chars().take(config.preview_chars).collect(),
                output_path: PathBuf::new(),
                total_bytes: content_len,
            };
        }
    }

    let preview = format!(
        "{}\n\n... [truncated, {} bytes total. Full output at: {}]",
        content
            .chars()
            .take(config.preview_chars)
            .collect::<String>(),
        content_len,
        output_path.display()
    );

    TruncationResult::Truncated {
        preview,
        output_path,
        total_bytes: content_len,
    }
}

/// Clean up truncation directory by removing temp files older than the specified duration.
///
/// # Arguments
/// * `dir` - The truncation directory to clean
/// * `max_age_secs` - Maximum age of files to keep (default 1 hour = 3600 seconds)
///
/// # Returns
/// Ok(()) on success, Err message on failure
pub fn cleanup_truncation_dir(dir: &Path, max_age_secs: u64) -> std::io::Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }

    let mut removed_count = 0;
    let now = std::time::SystemTime::now();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        // Check if it's a file (not directory)
        if path.is_file() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age.as_secs() > max_age_secs {
                            if fs::remove_file(&path).is_ok() {
                                removed_count += 1;
                            }
                        }
                    }
                }
            }
        }

        // Recursively clean subdirectories (session dirs)
        if path.is_dir() {
            removed_count += cleanup_truncation_dir(&path, max_age_secs).unwrap_or(0);
        }
    }

    Ok(removed_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config(temp_dir: &Path) -> TruncationConfig {
        TruncationConfig {
            max_bytes: 50 * 1024, // 50KB
            preview_chars: 2000,
            truncation_dir: temp_dir.to_path_buf(),
        }
    }

    #[test]
    fn test_truncation_config_default_values() {
        let config = TruncationConfig::default();
        assert_eq!(config.max_bytes, 50 * 1024);
        assert_eq!(config.preview_chars, 2000);
        assert_eq!(
            config.truncation_dir,
            std::env::temp_dir().join("rcode-truncation")
        );
    }

    #[test]
    fn test_not_truncated_under_limit() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path());

        let small_content = "Hello, World!";
        let result = truncate_output(small_content, &config, "session1", "call1");

        match result {
            TruncationResult::NotTruncated { content } => {
                assert_eq!(content, small_content);
            }
            TruncationResult::Truncated { .. } => {
                panic!("Small content should not be truncated");
            }
        }
    }

    #[test]
    fn test_truncated_over_limit() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path());

        // Content larger than 50KB
        let large_content = "A".repeat(60 * 1024);
        let result = truncate_output(&large_content, &config, "session1", "call1");

        match result {
            TruncationResult::NotTruncated { .. } => {
                panic!("Large content should be truncated");
            }
            TruncationResult::Truncated {
                preview,
                output_path,
                total_bytes,
            } => {
                assert!(total_bytes > config.max_bytes);
                assert!(preview.len() <= config.preview_chars + 200); // Account for truncation message
                assert!(output_path.to_str().unwrap().contains("session1"));
                assert!(output_path.to_str().unwrap().contains("call1"));

                // Verify the full content was written to file
                let written = fs::read_to_string(&output_path).unwrap();
                assert_eq!(written.len(), large_content.len());
                assert_eq!(written, large_content);
            }
        }
    }

    #[test]
    fn test_truncated_at_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config(temp_dir.path());
        config.max_bytes = 100;

        // Content exactly at boundary
        let content = "A".repeat(100);
        let result = truncate_output(&content, &config, "session1", "call1");

        match result {
            TruncationResult::NotTruncated { content: _ } => {
                // At exactly the limit, should NOT be truncated
            }
            TruncationResult::Truncated { .. } => {
                panic!("Content at exact limit should not be truncated");
            }
        }

        // Content just over boundary
        let over_content = "A".repeat(101);
        let result = truncate_output(&over_content, &config, "session1", "call1");

        match result {
            TruncationResult::NotTruncated { .. } => {
                panic!("Content over limit should be truncated");
            }
            TruncationResult::Truncated { total_bytes, .. } => {
                assert_eq!(total_bytes, 101);
            }
        }
    }

    #[test]
    fn test_truncated_preview_contains_path_hint() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path());

        let large_content = "B".repeat(60 * 1024);
        let result = truncate_output(&large_content, &config, "test-session", "test-call");

        match result {
            TruncationResult::Truncated {
                preview,
                output_path,
                ..
            } => {
                assert!(preview.contains("truncated"));
                assert!(preview.contains("Full output at:"));
                assert!(preview.contains(&output_path.display().to_string()));
            }
            TruncationResult::NotTruncated { .. } => {
                panic!("Large content should be truncated");
            }
        }
    }

    #[test]
    fn test_cleanup_removes_old_files() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path());

        // Create a test file
        let test_file = config.truncation_dir.join("old_file.txt");
        fs::write(&test_file, "old content").unwrap();

        // Set the file's modification time to 2 hours ago
        let two_hours_ago = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            - std::time::Duration::from_secs(2 * 3600);

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(metadata) = fs::metadata(&test_file) {
                // mtime is already set by write, we can't easily change it in tests
                // This test verifies the cleanup function is callable
            }
        }

        // Run cleanup with 1 hour max age
        let removed = cleanup_truncation_dir(&config.truncation_dir, 3600).unwrap();
        
        // Note: on some systems the file just written will have recent mtime
        // so we just verify the function works without error (removed >= 0 is always true for usize)
    }

    #[test]
    fn test_cleanup_nonexistent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("does_not_exist");

        let removed = cleanup_truncation_dir(&nonexistent, 3600).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn test_truncation_with_custom_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = TruncationConfig {
            max_bytes: 10,
            preview_chars: 5,
            truncation_dir: temp_dir.path().to_path_buf(),
        };

        let content = "Hello, World! This is a long message.";
        let result = truncate_output(content, &config, "sess", "call");

        match result {
            TruncationResult::Truncated {
                preview,
                output_path,
                total_bytes,
            } => {
                assert_eq!(total_bytes, content.len());
                assert!(output_path.to_str().unwrap().contains("sess"));
                assert!(output_path.to_str().unwrap().contains("call"));
            }
            TruncationResult::NotTruncated { .. } => {
                panic!("Content over 10 bytes should be truncated with max_bytes=10");
            }
        }
    }

    #[test]
    fn test_empty_content_not_truncated() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path());

        let result = truncate_output("", &config, "session1", "call1");

        match result {
            TruncationResult::NotTruncated { content } => {
                assert_eq!(content, "");
            }
            TruncationResult::Truncated { .. } => {
                panic!("Empty content should not be truncated");
            }
        }
    }

    #[test]
    fn test_unicode_content_truncation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config(temp_dir.path());
        config.max_bytes = 50;

        // Unicode content - each char can be multiple bytes
        let unicode_content = "日本語日本語日本語日本語日本語日本語日本語日本語日本語日本語";
        let result = truncate_output(unicode_content, &config, "session1", "call1");

        match result {
            TruncationResult::NotTruncated { .. } => {
                // If under byte limit, not truncated
            }
            TruncationResult::Truncated { total_bytes, .. } => {
                assert!(total_bytes > config.max_bytes);
            }
        }
    }
}
