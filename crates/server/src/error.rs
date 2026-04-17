//! Server errors

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[derive(Debug)]
pub enum ServerError {
    NotFound,
    BadRequest(String),
    UnprocessableEntity(String),
    Conflict(String),
    ConflictWithDetails {
        message: String,
        details: serde_json::Value,
    },
    Internal(String),
    InvalidTransition(String),
    Forbidden(String),
    RequestTimeout(String),
}

impl ServerError {
    pub fn not_found() -> Self {
        ServerError::NotFound
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        ServerError::BadRequest(msg.into())
    }

    pub fn unprocessable_entity(msg: impl Into<String>) -> Self {
        ServerError::UnprocessableEntity(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        ServerError::Conflict(msg.into())
    }

    /// Creates a CONFLICT error with structured details in the error response.
    /// The details are embedded in the JSON response body for API consumers.
    pub fn conflict_with_details(msg: impl Into<String>, details: serde_json::Value) -> Self {
        ServerError::ConflictWithDetails {
            message: msg.into(),
            details,
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        ServerError::Internal(msg.into())
    }

    pub fn invalid_transition(msg: impl Into<String>) -> Self {
        ServerError::InvalidTransition(msg.into())
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        ServerError::Forbidden(msg.into())
    }

    pub fn request_timeout(msg: impl Into<String>) -> Self {
        ServerError::RequestTimeout(msg.into())
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            ServerError::NotFound => (
                StatusCode::NOT_FOUND,
                "SESSION_NOT_FOUND",
                "Session not found",
            ),
            ServerError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.as_str()),
            ServerError::UnprocessableEntity(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "UNPROCESSABLE_ENTITY",
                msg.as_str(),
            ),
            ServerError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg.as_str()),
            ServerError::ConflictWithDetails { message, details } => {
                let error_response =
                    ErrorResponse::new("CONFLICT", message).with_details(details.clone());
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::to_value(error_response).unwrap_or_else(|_| {
                        json!({
                            "code": "CONFLICT",
                            "message": message
                        })
                    })),
                )
                    .into_response();
            }
            ServerError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                msg.as_str(),
            ),
            ServerError::InvalidTransition(msg) => {
                (StatusCode::CONFLICT, "INVALID_TRANSITION", msg.as_str())
            }
            ServerError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg.as_str()),
            ServerError::RequestTimeout(msg) => {
                (StatusCode::REQUEST_TIMEOUT, "REQUEST_TIMEOUT", msg.as_str())
            }
        };

        let error_response = ErrorResponse::new(code, message);

        (
            status,
            Json(serde_json::to_value(error_response).unwrap_or_else(|_| {
                json!({
                    "code": "INTERNAL_ERROR",
                    "message": "Failed to serialize error response"
                })
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_response_new() {
        let err = ErrorResponse::new("TEST_ERROR", "Test message");
        assert_eq!(err.code, "TEST_ERROR");
        assert_eq!(err.message, "Test message");
        assert!(err.details.is_none());
    }

    #[test]
    fn test_error_response_with_details() {
        let err =
            ErrorResponse::new("TEST_ERROR", "Test message").with_details(json!({"key": "value"}));
        assert_eq!(err.code, "TEST_ERROR");
        assert!(err.details.is_some());
    }

    #[test]
    fn test_server_error_not_found() {
        let err = ServerError::not_found();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_server_error_bad_request() {
        let err = ServerError::bad_request("Invalid input");
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_server_error_internal() {
        let err = ServerError::internal("Something went wrong");
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_server_error_invalid_transition() {
        let err = ServerError::invalid_transition("Cannot transition from Completed to Running");
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
}
