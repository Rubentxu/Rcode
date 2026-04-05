//! Tool parameter validation using JSON Schema

use jsonschema::Validator;
use serde_json::Value;
use thiserror::Error;

use rcode_core::error::RCodeError;

/// Validation error with field and message details
#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    #[error("Validation failed for field '{field}': {message}")]
    Field { field: String, message: String },

    #[error("Validation error: {0}")]
    Message(String),
}

impl From<ValidationError> for RCodeError {
    fn from(err: ValidationError) -> Self {
        match err {
            ValidationError::Field { field, message } => {
                RCodeError::Validation { field, message }
            }
            ValidationError::Message(msg) => RCodeError::Validation {
                field: String::new(),
                message: msg,
            },
        }
    }
}

/// Tool parameter validator using JSON Schema
pub struct ToolValidator;

impl ToolValidator {
    /// Validate arguments against a JSON schema
    pub fn validate(args: &Value, schema: &Value) -> Result<(), ValidationError> {
        let validator = Validator::new(schema)
            .map_err(|e| ValidationError::Message(format!("Invalid schema: {}", e)))?;

        let validation_errors: Vec<String> = validator
            .iter_errors(args)
            .map(|error| {
                let path = error.instance_path.to_string();
                if path.is_empty() {
                    format!("{}", error)
                } else {
                    format!("field '{}': {}", path, error)
                }
            })
            .collect();

        if validation_errors.is_empty() {
            Ok(())
        } else {
            // Return the first validation error with the field path
            let first_error = validation_errors[0].clone();
            let field = first_error
                .split(':')
                .next()
                .unwrap_or(&first_error)
                .trim()
                .to_string();

            Err(ValidationError::Field {
                field,
                message: validation_errors.join("; "),
            })
        }
    }

    /// Validate tool arguments by schema name and parameters
    pub fn validate_with_schema(args: &Value, schema: &Value) -> Result<(), RCodeError> {
        Self::validate(args, schema)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_valid_args() {
        let schema = json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                }
            },
            "required": ["command"]
        });

        let args = json!({
            "command": "ls -la"
        });

        let result = ToolValidator::validate(&args, &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_missing_required_field() {
        let schema = json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                }
            },
            "required": ["command"]
        });

        let args = json!({});

        let result = ToolValidator::validate(&args, &schema);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ValidationError::Field { .. }));
    }

    #[test]
    fn test_validate_wrong_type() {
        let schema = json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer"
                }
            }
        });

        let args = json!({
            "count": "not an integer"
        });

        let result = ToolValidator::validate(&args, &schema);
        assert!(result.is_err());
    }
}
