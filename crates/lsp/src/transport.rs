//! LSP transport layer for communication with language servers

use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use super::error::{LspError, Result};
use super::types::LspMessage;

/// Trait for LSP transport implementations
#[async_trait]
pub trait LspTransport: Send + Sync {
    /// Send a message to the language server
    async fn send(&self, msg: LspMessage) -> Result<()>;
    
    /// Receive a message from the language server (requires &mut self)
    async fn receive(&mut self) -> Result<LspMessage>;
}

/// Stdio-based transport for local language servers
pub struct StdioTransport {
    #[allow(dead_code)]
    child: Child,
    stdin: Mutex<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl StdioTransport {
    /// Create a new stdio transport by spawning the given command
    pub async fn spawn(cmd: &[&str], cwd: &std::path::Path) -> Result<Self> {
        let mut child = tokio::process::Command::new(cmd[0])
            .args(&cmd[1..])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| LspError::Process(format!("Failed to spawn {}: {}", cmd[0], e)))?;

        let stdin = child.stdin.take().ok_or_else(|| {
            LspError::Transport("Failed to take stdin".to_string())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            LspError::Transport("Failed to take stdout".to_string())
        })?;

        Ok(Self {
            child,
            stdin: Mutex::new(stdin),
            stdout: BufReader::new(stdout),
        })
    }

    /// Send a raw JSON-RPC message
    async fn send_raw(&self, msg: &str) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        // Send Content-Length header
        let len = msg.len();
        stdin.write_all(format!("Content-Length: {}\r\n\r\n", len).as_bytes()).await?;
        stdin.write_all(msg.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Receive a raw JSON-RPC message
    async fn receive_raw(&mut self) -> Result<String> {
        let mut line = String::new();
        
        // Read headers
        let mut content_length: Option<usize> = None;
        loop {
            line.clear();
            self.stdout.read_line(&mut line).await?;
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if line.starts_with("Content-Length:") {
                let len_str = line.trim_start_matches("Content-Length:").trim();
                content_length = Some(len_str.parse().map_err(|_| {
                    LspError::InvalidResponse("Invalid Content-Length".to_string())
                })?);
            }
        }

        let content_length = content_length.ok_or_else(|| {
            LspError::InvalidResponse("Missing Content-Length header".to_string())
        })?;

        // Read content
        let mut content = vec![0u8; content_length];
        tokio::io::AsyncReadExt::read_exact(&mut self.stdout, &mut content).await?;
        
        String::from_utf8(content).map_err(|_| {
            LspError::InvalidResponse("Invalid UTF-8 in response".to_string())
        })
    }

    /// Send a request and wait for response
    pub async fn send_request(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        static REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let json = serde_json::to_string(&request)?;
        self.send_raw(&json).await?;

        // Read response
        let response: JsonRpcResponse = {
            let json = self.receive_raw().await?;
            serde_json::from_str(&json)?
        };

        if let Some(error) = response.error {
            return Err(LspError::Server(error.message));
        }

        response.result.ok_or_else(|| {
            LspError::InvalidResponse("Missing result in response".to_string())
        })
    }
}

#[async_trait]
impl LspTransport for StdioTransport {
    async fn send(&self, msg: LspMessage) -> Result<()> {
        let json = serde_json::to_string(&msg)?;
        self.send_raw(&json).await
    }

    async fn receive(&mut self) -> Result<LspMessage> {
        let json = self.receive_raw().await?;
        let msg: LspMessage = serde_json::from_str(&json)?;
        Ok(msg)
    }
}

/// JSON-RPC request/response wrapper
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: u64,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
}
