//! JSON-RPC 2.0 protocol types

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.into(),
            params,
        }
    }

    pub fn with_id(mut self, id: serde_json::Value) -> Self {
        self.id = Some(id);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<serde_json::Value>, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    pub fn new(code: JsonRpcErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code as i32,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub enum JsonRpcErrorCode {
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
    ServerError = -32000,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_new() {
        let req = JsonRpcRequest::new("method_name", Some(serde_json::json!({"key": "value"})));
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "method_name");
        assert!(req.params.is_some());
        assert!(req.id.is_none());
    }

    #[test]
    fn test_json_rpc_request_with_id() {
        let req = JsonRpcRequest::new("method", None).with_id(serde_json::json!(42));
        assert!(req.id.is_some());
        assert_eq!(req.id, Some(serde_json::json!(42)));
    }

    #[test]
    fn test_json_rpc_request_serialization() {
        let req = JsonRpcRequest::new("test", Some(serde_json::json!({"a": 1})));
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "test");
    }

    #[test]
    fn test_json_rpc_response_success() {
        let resp = JsonRpcResponse::success(
            Some(serde_json::json!(1)),
            serde_json::json!({"result": "ok"}),
        );
        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, Some(serde_json::json!(1)));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_json_rpc_response_error() {
        let error = JsonRpcError::new(JsonRpcErrorCode::InvalidParams, "Bad params");
        let resp = JsonRpcResponse::error(Some(serde_json::json!(1)), error);
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_json_rpc_error_new() {
        let error = JsonRpcError::new(JsonRpcErrorCode::ParseError, "Parse error");
        assert_eq!(error.code, -32700);
        assert_eq!(error.message, "Parse error");
        assert!(error.data.is_none());
    }

    #[test]
    fn test_json_rpc_error_with_data() {
        let error = JsonRpcError::new(JsonRpcErrorCode::InvalidRequest, "Invalid")
            .with_data(serde_json::json!({"field": "value"}));
        assert!(error.data.is_some());
        assert_eq!(error.data, Some(serde_json::json!({"field": "value"})));
    }

    #[test]
    fn test_json_rpc_error_codes() {
        assert_eq!(JsonRpcErrorCode::ParseError as i32, -32700);
        assert_eq!(JsonRpcErrorCode::InvalidRequest as i32, -32600);
        assert_eq!(JsonRpcErrorCode::MethodNotFound as i32, -32601);
        assert_eq!(JsonRpcErrorCode::InvalidParams as i32, -32602);
        assert_eq!(JsonRpcErrorCode::InternalError as i32, -32603);
        assert_eq!(JsonRpcErrorCode::ServerError as i32, -32000);
    }

    #[test]
    fn test_json_rpc_notification_new() {
        let notif = JsonRpcNotification::new("method", Some(serde_json::json!({"a": 1})));
        assert_eq!(notif.jsonrpc, "2.0");
        assert_eq!(notif.method, "method");
        assert!(notif.params.is_some());
    }

    #[test]
    fn test_json_rpc_notification_no_params() {
        let notif = JsonRpcNotification::new("method", None);
        assert!(notif.params.is_none());
    }

    #[test]
    fn test_json_rpc_notification_serialization() {
        let notif = JsonRpcNotification::new(
            "notifications/event",
            Some(serde_json::json!({"data": "value"})),
        );
        let json = serde_json::to_value(&notif).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "notifications/event");
    }

    #[test]
    fn test_json_rpc_response_serialization_roundtrip() {
        let resp = JsonRpcResponse::success(
            Some(serde_json::json!(1)),
            serde_json::json!({"key": "value"}),
        );
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.jsonrpc, "2.0");
        assert_eq!(parsed.id, Some(serde_json::json!(1)));
    }

    #[test]
    fn test_json_rpc_request_deserialization() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "test",
            "params": {"key": "value"},
            "id": 1
        });
        let req: JsonRpcRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.method, "test");
        assert_eq!(req.id, Some(serde_json::json!(1)));
    }
}
