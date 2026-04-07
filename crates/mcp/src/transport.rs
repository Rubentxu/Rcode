//! MCP transport implementations

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use super::error::{McpError, Result};
use super::types::{JsonRpcRequest, JsonRpcResponse};

/// Trait for MCP transports
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a JSON-RPC request
    async fn send(&self, request: JsonRpcRequest) -> Result<()>;

    /// Receive a JSON-RPC response
    async fn receive(&self) -> Result<JsonRpcResponse>;
}

/// HTTP transport for connecting to MCP servers via HTTP
pub struct HttpTransport {
    client: reqwest::Client,
    url: String,
}

impl HttpTransport {
    /// Create a new HTTP transport
    pub fn new(url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
        }
    }

    /// Create a new HTTP transport with a custom client
    pub fn with_client(url: &str, client: reqwest::Client) -> Self {
        Self {
            client,
            url: url.to_string(),
        }
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<()> {
        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(McpError::Connection(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        Ok(())
    }

    async fn receive(&self) -> Result<JsonRpcResponse> {
        // For HTTP, we typically send and receive in one call
        // This is handled by the client directly
        Err(McpError::Transport(
            "HTTP transport does not support separate receive".to_string(),
        ))
    }
}

/// Stdio transport for connecting to MCP servers via stdin/stdout
pub struct StdioTransport {
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Arc<Mutex<BufReader<ChildStdout>>>,
    request_id: Arc<Mutex<i64>>,
}

impl StdioTransport {
    /// Create a new stdio transport from a child process
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
            request_id: Arc::new(Mutex::new(0)),
        }
    }

    /// Spawn an MCP server process and create a transport for it
    pub async fn spawn(command: &str, args: &[&str]) -> Result<Self> {
        use tokio::process::Command;

        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| {
            McpError::Connection("Failed to take stdin".to_string())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            McpError::Connection("Failed to take stdout".to_string())
        })?;

        Ok(Self::new(stdin, stdout))
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<()> {
        let mut id = self.request_id.lock().await;
        let request_with_id = JsonRpcRequest {
            id: Some(Value::Number((*id).into())),
            ..request
        };
        *id += 1;
        drop(id);

        let json = serde_json::to_string(&request_with_id)?;
        let line = format!("{}\n", json);

        self.stdin.lock().await.write_all(line.as_bytes()).await?;
        self.stdin.lock().await.flush().await?;

        Ok(())
    }

    async fn receive(&self) -> Result<JsonRpcResponse> {
        let mut line = String::new();
        self.stdout.lock().await.read_line(&mut line).await?;

        if line.is_empty() {
            return Err(McpError::Transport("Empty response".to_string()));
        }

        let response: JsonRpcResponse = serde_json::from_str(&line)?;
        Ok(response)
    }
}

/// In-memory transport for testing
#[cfg(test)]
pub struct InMemoryTransport {
    requests: Arc<Mutex<Vec<JsonRpcRequest>>>,
    responses: Arc<Mutex<Vec<JsonRpcResponse>>>,
}

#[cfg(test)]
impl Default for InMemoryTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl InMemoryTransport {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn add_response(&self, response: JsonRpcResponse) {
        self.responses.lock().await.push(response);
    }

    pub async fn get_requests(&self) -> Vec<JsonRpcRequest> {
        self.requests.lock().await.clone()
    }
}

#[cfg(test)]
#[async_trait]
impl McpTransport for InMemoryTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<()> {
        self.requests.lock().await.push(request);
        Ok(())
    }

    async fn receive(&self) -> Result<JsonRpcResponse> {
        self.responses
            .lock()
            .await
            .pop()
            .ok_or_else(|| McpError::Transport("No response available".to_string()))
    }
}
