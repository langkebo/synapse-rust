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
            ApiError::Database(_) | ApiError::Cache(_) => ErrorLayer::Infrastructure,
            ApiError::Unauthorized(_)
            | ApiError::Forbidden(_)
            | ApiError::NotFound(_)
            | ApiError::Conflict(_)
            | ApiError::RateLimited => ErrorLayer::BusinessLogic,
            ApiError::Internal(_)
            | ApiError::Authentication(_)
            | ApiError::Validation(_)
            | ApiError::InvalidInput(_)
            | ApiError::DecryptionError(_)
            | ApiError::EncryptionError(_)
            | ApiError::Crypto(_)
            | ApiError::BadRequest(_) => ErrorLayer::Application,
        }
    }

    fn severity(&self) -> ErrorSeverity {
        match self {
            ApiError::Internal(_) | ApiError::Database(_) | ApiError::Cache(_) => {
                ErrorSeverity::Critical
            }
            ApiError::Unauthorized(_) | ApiError::Forbidden(_) | ApiError::RateLimited => {
                ErrorSeverity::High
            }
            ApiError::NotFound(_) | ApiError::Conflict(_) | ApiError::Validation(_) => {
                ErrorSeverity::Medium
            }
            ApiError::BadRequest(_) | ApiError::InvalidInput(_) => ErrorSeverity::Low,
            ApiError::Authentication(_)
            | ApiError::DecryptionError(_)
            | ApiError::EncryptionError(_)
            | ApiError::Crypto(_) => ErrorSeverity::High,
        }
    }

    fn user_message(&self) -> String {
        match self {
            ApiError::BadRequest(msg) => msg.clone(),
            ApiError::Unauthorized(msg) => msg.clone(),
            ApiError::Forbidden(msg) => msg.clone(),
            ApiError::NotFound(msg) => msg.clone(),
            ApiError::Conflict(msg) => msg.clone(),
            ApiError::RateLimited => "Too many requests. Please try again later.".to_string(),
            ApiError::Internal(_) => {
                "An internal server error occurred. Please try again later.".to_string()
            }
            ApiError::Database(_) => {
                "A database error occurred. Please try again later.".to_string()
            }
            ApiError::Cache(_) => "A cache error occurred. Please try again later.".to_string(),
            ApiError::Authentication(msg) => msg.clone(),
            ApiError::Validation(msg) => msg.clone(),
            ApiError::InvalidInput(msg) => msg.clone(),
            ApiError::DecryptionError(_) => {
                "Failed to decrypt data. Please check your encryption keys.".to_string()
            }
            ApiError::EncryptionError(_) => {
                "Failed to encrypt data. Please check your encryption keys.".to_string()
            }
            ApiError::Crypto(_) => {
                "A cryptographic error occurred. Please try again later.".to_string()
            }
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
