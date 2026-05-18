use super::ApiError;

pub enum ErrorLayer {
    Infrastructure,
    BusinessLogic,
    Application,
}

pub enum ErrorSeverity {
    Critical,
    High,
    Medium,
    Low,
}

/// CRITICAL FIX: Safe error handling helper that logs internal errors
/// while returning sanitized messages to users
pub fn safe_db_error<T: std::fmt::Display>(context: &str, operation: &str, error: T) -> ApiError {
    // Log the full error for debugging
    tracing::error!(
        context = context,
        operation = operation,
        error = %error,
        "Database operation failed"
    );

    // Return sanitized message to user
    ApiError::internal(format!(
        "An error occurred while {}",
        operation.to_lowercase()
    ))
}

/// Helper for handling database errors in web handlers
pub fn handle_db_error<T: std::fmt::Display>(error: T, operation: &str) -> ApiError {
    safe_db_error("web_handler", operation, error)
}

pub trait ErrorContext {
    fn layer(&self) -> ErrorLayer;
    fn severity(&self) -> ErrorSeverity;
    fn user_message(&self) -> String;
    fn technical_details(&self) -> String;
    fn log_level(&self) -> tracing::Level {
        match self.severity() {
            ErrorSeverity::Critical => tracing::Level::ERROR,
            ErrorSeverity::High => tracing::Level::ERROR,
            ErrorSeverity::Medium => tracing::Level::WARN,
            ErrorSeverity::Low => tracing::Level::INFO,
        }
    }
}

impl ErrorContext for ApiError {
    fn layer(&self) -> ErrorLayer {
        match self {
            Self::Database(_) | Self::Cache(_) => ErrorLayer::Infrastructure,
            Self::Unauthorized(_)
            | Self::Forbidden(_)
            | Self::NotFound(_)
            | Self::Gone(_)
            | Self::Conflict(_)
            | Self::RateLimited
            | Self::RateLimitedWithRetry(_) => ErrorLayer::BusinessLogic,
            Self::Internal(_)
            | Self::Authentication(_)
            | Self::Validation(_)
            | Self::InvalidInput(_)
            | Self::DecryptionError(_)
            | Self::EncryptionError(_)
            | Self::Crypto(_)
            | Self::BadRequest(_) => ErrorLayer::Application,
            _ => ErrorLayer::Application,
        }
    }

    fn severity(&self) -> ErrorSeverity {
        match self {
            Self::Internal(_) | Self::Database(_) | Self::Cache(_) => {
                ErrorSeverity::Critical
            }
            Self::Unauthorized(_)
            | Self::Forbidden(_)
            | Self::RateLimited
            | Self::RateLimitedWithRetry(_) => ErrorSeverity::High,
            Self::Gone(_)
            | Self::NotFound(_)
            | Self::Conflict(_)
            | Self::Validation(_) => ErrorSeverity::Medium,
            Self::BadRequest(_) | Self::InvalidInput(_) => ErrorSeverity::Low,
            Self::Authentication(_)
            | Self::DecryptionError(_)
            | Self::EncryptionError(_)
            | Self::Crypto(_) => ErrorSeverity::High,
            _ => ErrorSeverity::Medium,
        }
    }

    fn user_message(&self) -> String {
        match self {
            Self::BadRequest(msg) => msg.clone(),
            Self::Unauthorized(msg) => msg.clone(),
            Self::Forbidden(msg) => msg.clone(),
            Self::NotFound(msg) => msg.clone(),
            Self::Gone(msg) => msg.clone(),
            Self::Conflict(msg) => msg.clone(),
            Self::RateLimited => "Too many requests. Please try again later.".to_string(),
            Self::RateLimitedWithRetry(_) => {
                "Too many requests. Please try again later.".to_string()
            }
            Self::Internal(_) => {
                "An internal server error occurred. Please try again later.".to_string()
            }
            Self::Database(_) => {
                "A database error occurred. Please try again later.".to_string()
            }
            Self::Cache(_) => "A cache error occurred. Please try again later.".to_string(),
            Self::Authentication(msg) => msg.clone(),
            Self::Validation(msg) => msg.clone(),
            Self::InvalidInput(msg) => msg.clone(),
            Self::DecryptionError(_) => {
                "Failed to decrypt data. Please check your encryption keys.".to_string()
            }
            Self::EncryptionError(_) => {
                "Failed to encrypt data. Please check your encryption keys.".to_string()
            }
            Self::Crypto(_) => {
                "A cryptographic error occurred. Please try again later.".to_string()
            }
            Self::MissingToken => "Authentication token is required.".to_string(),
            Self::NotJson(msg) => msg.clone(),
            Self::UserDeactivated(msg) => msg.clone(),
            Self::InvalidUsername(msg) => msg.clone(),
            Self::RoomInUse(msg) => msg.clone(),
            Self::UserInUse(msg) => msg.clone(),
            Self::InvalidRoomState(msg) => msg.clone(),
            Self::ThreepidInUse(msg) => msg.clone(),
            Self::ThreepidNotFound(msg) => msg.clone(),
            Self::ThreepidAuthFailed(msg) => msg.clone(),
            Self::ThreepidDenied(msg) => msg.clone(),
            Self::ServerNotTrusted(msg) => msg.clone(),
            Self::UnsupportedRoomVersion(msg) => msg.clone(),
            Self::IncompatibleRoomVersion(msg) => msg.clone(),
            Self::BadState(msg) => msg.clone(),
            Self::GuestAccessForbidden(msg) => msg.clone(),
            Self::CaptchaNeeded(msg) => msg.clone(),
            Self::CaptchaInvalid(msg) => msg.clone(),
            Self::MissingParam(msg) => msg.clone(),
            Self::TooLarge(msg) => msg.clone(),
            Self::Exclusive(msg) => msg.clone(),
            Self::ResourceLimitExceeded(msg) => msg.clone(),
            Self::CannotLeaveServerNoticeRoom(msg) => msg.clone(),
            Self::Unknown(msg) => msg.clone(),
            Self::Unrecognized(msg) => msg.clone(),
            Self::NotImplemented(msg) => msg.clone(),
            Self::RequestTimeout(msg) => msg.clone(),
        }
    }

    fn technical_details(&self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_layer_classification() {
        assert!(matches!(
            ApiError::Database("test".to_string()).layer(),
            ErrorLayer::Infrastructure
        ));
        assert!(matches!(
            ApiError::Cache("test".to_string()).layer(),
            ErrorLayer::Infrastructure
        ));
        assert!(matches!(
            ApiError::Unauthorized("test".to_string()).layer(),
            ErrorLayer::BusinessLogic
        ));
        assert!(matches!(
            ApiError::NotFound("test".to_string()).layer(),
            ErrorLayer::BusinessLogic
        ));
        assert!(matches!(
            ApiError::Internal("test".to_string()).layer(),
            ErrorLayer::Application
        ));
    }

    #[test]
    fn test_error_severity_classification() {
        assert!(matches!(
            ApiError::Internal("test".to_string()).severity(),
            ErrorSeverity::Critical
        ));
        assert!(matches!(
            ApiError::Unauthorized("test".to_string()).severity(),
            ErrorSeverity::High
        ));
        assert!(matches!(
            ApiError::NotFound("test".to_string()).severity(),
            ErrorSeverity::Medium
        ));
        assert!(matches!(
            ApiError::BadRequest("test".to_string()).severity(),
            ErrorSeverity::Low
        ));
    }

    #[test]
    fn test_user_message_generation() {
        let error = ApiError::not_found("User not found");
        assert!(error.user_message().contains("User not found"));

        let error = ApiError::RateLimited;
        assert!(error.user_message().contains("Too many requests"));

        let error = ApiError::internal("Database connection failed");
        assert!(error.user_message().contains("internal server error"));
    }

    #[test]
    fn test_log_level_mapping() {
        assert_eq!(
            ApiError::Internal("test".to_string()).log_level(),
            tracing::Level::ERROR
        );
        assert_eq!(
            ApiError::Unauthorized("test".to_string()).log_level(),
            tracing::Level::ERROR
        );
        assert_eq!(
            ApiError::NotFound("test".to_string()).log_level(),
            tracing::Level::WARN
        );
        assert_eq!(
            ApiError::BadRequest("test".to_string()).log_level(),
            tracing::Level::INFO
        );
    }
}
