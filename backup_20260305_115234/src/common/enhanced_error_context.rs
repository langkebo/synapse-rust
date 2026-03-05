use crate::common::ApiError;
use std::fmt;

#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub operation: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub source_error: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub request_id: Option<String>,
}

impl ErrorContext {
    pub fn new(
        operation: impl Into<String>,
        entity_type: impl Into<String>,
        entity_id: Option<String>,
        source_error: impl fmt::Display,
    ) -> Self {
        Self {
            operation: operation.into(),
            entity_type: entity_type.into(),
            entity_id,
            source_error: source_error.to_string(),
            timestamp: chrono::Utc::now(),
            request_id: None,
        }
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    pub fn to_api_error(&self, user_message: impl Into<String>) -> ApiError {
        let detailed_message = format!(
            "[{}] {} - {} (ID: {:?}): {}",
            self.timestamp.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            self.operation,
            self.entity_type,
            self.entity_id,
            self.source_error
        );
        ApiError::internal(detailed_message)
    }

    pub fn log_context(&self) {
        tracing::error!(
            operation = %self.operation,
            entity_type = %self.entity_type,
            entity_id = ?self.entity_id,
            error = %self.source_error,
            timestamp = %self.timestamp.to_rfc3339(),
            request_id = ?self.request_id,
            "Error context captured"
        );
    }
}

#[macro_export]
macro_rules! ctx {
    ($operation:expr, $entity_type:expr, $entity_id:expr, $error:expr) => {
        ErrorContext::new($operation, $entity_type, $entity_id, $error)
    };
    ($operation:expr, $entity_type:expr, $error:expr) => {
        ErrorContext::new($operation, $entity_type, None, $error)
    };
}

#[macro_export]
macro_rules! with_context {
    ($result:expr, $operation:expr, $entity_type:expr, $entity_id:expr) => {
        $result.map_err(|e| {
            let ctx = ErrorContext::new($operation, $entity_type, $entity_id, &e);
            ctx.log_context();
            ctx.to_api_error("Operation failed")
        })
    };
    ($result:expr, $operation:expr, $entity_type:expr) => {
        $result.map_err(|e| {
            let ctx = ErrorContext::new($operation, $entity_type, None, &e);
            ctx.log_context();
            ctx.to_api_error("Operation failed")
        })
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_creation() {
        let ctx = ErrorContext::new(
            "update_user_profile",
            "User",
            Some("user123".to_string()),
            "Database connection timeout",
        );

        assert_eq!(ctx.operation, "update_user_profile");
        assert_eq!(ctx.entity_type, "User");
        assert_eq!(ctx.entity_id, Some("user123".to_string()));
        assert!(ctx.source_error.contains("Database connection timeout"));
    }

    #[test]
    fn test_error_context_with_request_id() {
        let ctx = ErrorContext::new("test_op", "Test", None, "test error")
            .with_request_id("req-123");

        assert_eq!(ctx.request_id, Some("req-123".to_string()));
    }

    #[test]
    fn test_to_api_error() {
        let ctx = ErrorContext::new("delete_file", "File", Some("file456".to_string()), "IO error");
        let api_error = ctx.to_api_error("Failed to delete file");

        match api_error {
            ApiError::Internal(msg) => {
                assert!(msg.contains("delete_file"));
                assert!(msg.contains("File"));
                assert!(msg.contains("file456"));
            }
            _ => panic!("Expected Internal error"),
        }
    }
}
