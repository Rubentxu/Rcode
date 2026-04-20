//! Read tool - file reading with binary attachment support

use async_trait::async_trait;
use std::path::Path;
use tokio::fs;

/// Maximum size for binary attachments in bytes (5MB)
const MAX_ATTACHMENT_SIZE: usize = 5 * 1024 * 1024;

pub struct ReadTool;

impl ReadTool {
    pub fn new() -> Self { Self }
}

impl Default for ReadTool {
    fn default() -> Self { Self::new() }
}

/// Check if a MIME type represents a binary attachment that should be returned
/// as an attachment rather than text content.
fn is_binary_attachment(mime_type: &str) -> bool {
    matches!(mime_type,
        "image/png"
        | "image/jpeg"
        | "image/gif"
        | "image/webp"
        | "image/svg+xml"
        | "application/pdf"
    )
}

/// Get MIME type from magic bytes, falling back to extension-based detection.
fn detect_mime_type(path: &Path, bytes: &[u8]) -> String {
    if let Some(kind) = infer::get(bytes) {
        return kind.mime_type().to_string();
    }
    // Fallback to extension-based detection
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| match ext.to_lowercase().as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            _ => "application/octet-stream",
        })
        .unwrap_or("application/octet-stream")
        .to_string()
}

#[async_trait]
impl rcode_core::Tool for ReadTool {
    fn id(&self) -> &str { "read" }
    fn name(&self) -> &str { "Read" }
    fn description(&self) -> &str { "Read file contents. For binary files (images, PDFs), returns file metadata and attachment reference." }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value, context: &rcode_core::ToolContext) -> rcode_core::error::Result<rcode_core::ToolResult> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| rcode_core::RCodeError::Tool("Missing 'path' argument".into()))?;

        let full_path = context.cwd.join(path);

        // Read file as bytes first to detect binary content
        let bytes = fs::read(&full_path).await
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Failed to read {}: {}", path, e)))?;

        let file_size = bytes.len();

        // Check size limit for binary attachments
        let mime_type = detect_mime_type(&full_path, &bytes);

        if is_binary_attachment(&mime_type) {
            if file_size > MAX_ATTACHMENT_SIZE {
                return Ok(rcode_core::ToolResult {
                    title: format!("Read: {}", path),
                    content: format!(
                        "Error: File '{}' ({} bytes) exceeds maximum attachment size of {} bytes",
                        path, file_size, MAX_ATTACHMENT_SIZE
                    ),
                    metadata: Some(serde_json::json!({
                        "is_error": true,
                        "size": file_size,
                        "mime_type": mime_type,
                    })),
                    attachments: vec![],
                });
            }

            // Return attachment for binary files
            let name = full_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path)
                .to_string();

            return Ok(rcode_core::ToolResult {
                title: format!("Read: {}", path),
                content: format!("Read binary file {} ({} bytes, {})", name, file_size, mime_type),
                metadata: Some(serde_json::json!({
                    "is_error": false,
                    "size": file_size,
                    "mime_type": mime_type,
                    "is_binary": true,
                })),
                attachments: vec![rcode_core::ToolAttachment {
                    path: full_path.to_string_lossy().to_string(),
                    mime_type,
                    name,
                    size: file_size,
                }],
            });
        }

        // For text files, return content as string
        let content = String::from_utf8(bytes)
            .map_err(|_| rcode_core::RCodeError::Tool(format!("File '{}' is not valid UTF-8 text", path)))?;

        Ok(rcode_core::ToolResult {
            title: format!("Read: {}", path),
            content,
            metadata: None,
            attachments: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{Tool, ToolContext};
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
    async fn test_read_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();
        let tool = ReadTool::new();
        let result = tool.execute(serde_json::json!({"path": "hello.txt"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();
        assert_eq!(result.content, "hello world");
        assert!(result.attachments.is_empty());
    }

    #[tokio::test]
    async fn test_read_missing_path() {
        let tool = ReadTool::new();
        let result = tool.execute(serde_json::json!({}), &ctx("/tmp")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let tool = ReadTool::new();
        let result = tool.execute(serde_json::json!({"path": "nonexistent.txt"}), &ctx("/tmp")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_binary_png_attachment() {
        let dir = tempfile::tempdir().unwrap();
        // Minimal valid PNG (1x1 transparent pixel)
        let png_bytes: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
            0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, // IDAT chunk
            0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
            0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, // IEND chunk
            0x42, 0x60, 0x82,
        ];
        std::fs::write(dir.path().join("test.png"), &png_bytes).unwrap();

        let tool = ReadTool::new();
        let result = tool.execute(serde_json::json!({"path": "test.png"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();

        assert!(result.content.contains("Read binary file"));
        assert!(result.content.contains("image/png"));
        assert_eq!(result.attachments.len(), 1);
        assert_eq!(result.attachments[0].mime_type, "image/png");
        assert_eq!(result.attachments[0].name, "test.png");
        assert_eq!(result.attachments[0].size, png_bytes.len());
    }

    #[tokio::test]
    async fn test_read_binary_pdf_attachment() {
        let dir = tempfile::tempdir().unwrap();
        // Minimal PDF (empty document)
        let pdf_bytes = b"%PDF-1.0\n%\xe2\xe3\xcf\xd3\n\x25\x25\x45\x4f\x46\n\x25\x25\x45\x4f\x46";
        std::fs::write(dir.path().join("test.pdf"), &pdf_bytes[..]).unwrap();

        let tool = ReadTool::new();
        let result = tool.execute(serde_json::json!({"path": "test.pdf"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();

        assert!(result.content.contains("Read binary file"));
        assert!(result.content.contains("application/pdf"));
        assert_eq!(result.attachments.len(), 1);
        assert_eq!(result.attachments[0].mime_type, "application/pdf");
    }

    #[tokio::test]
    async fn test_read_file_exceeds_max_size() {
        let dir = tempfile::tempdir().unwrap();
        // Create a file larger than 5MB
        let large_content: Vec<u8> = (0..6 * 1024 * 1024).map(|i| (i % 256) as u8).collect();
        std::fs::write(dir.path().join("large.png"), &large_content).unwrap();

        let tool = ReadTool::new();
        let result = tool.execute(serde_json::json!({"path": "large.png"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();

        assert!(result.content.contains("exceeds maximum attachment size"));
        assert!(result.attachments.is_empty());
    }

    #[test]
    fn test_is_binary_attachment_detection() {
        assert!(is_binary_attachment("image/png"));
        assert!(is_binary_attachment("image/jpeg"));
        assert!(is_binary_attachment("image/gif"));
        assert!(is_binary_attachment("image/webp"));
        assert!(is_binary_attachment("image/svg+xml"));
        assert!(is_binary_attachment("application/pdf"));
        assert!(!is_binary_attachment("text/plain"));
        assert!(!is_binary_attachment("text/html"));
        assert!(!is_binary_attachment("application/json"));
    }
}
