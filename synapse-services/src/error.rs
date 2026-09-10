//! Service-layer unified error types for A/A+ quality compliance.
//!
//! This module defines `ServiceError` — a domain-grouped error enum that replaces
//! ad-hoc `Result<_, String>` returns throughout synapse-services. Each variant
//! maps to a proper `ApiError` with the correct HTTP status code.
//!
//! # Design Principles
//!
//! 1. **Domain Grouping** — Errors grouped by functional domain (auth, membership,
//!    sync, federation, e2ee, media, policy) for clear error surfaces.
//! 2. **Status Code Preservation** — Every variant carries an explicit HTTP status code.
//! 3. **No `unwrap` Antipattern** — All error paths return `Err`, never panic.

use std::fmt;

use synapse_common::ApiError;

/// Root service error type. All service methods should return `Result<T, ServiceError>`.
#[derive(Debug, Clone)]
pub enum ServiceError {
    // ===== Auth Domain =====
    /// Login or token validation failed with an unexpected error.
    AuthFailed {
        /// Human-readable error message.
        message: String,
    },
    /// Token has been revoked or blacklisted.
    TokenRevoked {
        /// Token type identifier.
        token_type: String,
    },
    /// Invalid or expired password reset token.
    InvalidResetToken,
    /// OIDC token verification failed (signature, key, claims, etc.).
    OidcVerificationFailed {
        /// Human-readable error message; typically includes reason and subject.
        message: String,
    },

    // ===== Application Service Domain =====
    /// Failed to list active application services.
    ApplicationServiceListFailed {
        /// Human-readable error message.
        message: String,
    },
    /// Failed to get application service statistics.
    ApplicationServiceStatsFailed {
        /// Human-readable error message.
        message: String,
    },
    /// Failed to count pending events for application service.
    ApplicationServiceEventCountFailed {
        /// Application service identifier.
        as_id: String,
        /// Human-readable error message.
        message: String,
    },
    /// Failed to count pending transactions for application service.
    ApplicationServiceTransactionCountFailed {
        /// Application service identifier.
        as_id: String,
        /// Human-readable error message.
        message: String,
    },

    // ===== Push Domain =====
    /// Push provider error (configuration, request, warning).
    PushProviderError {
        /// Provider name identifier.
        provider: String,
        /// Human-readable error message.
        message: String,
    },

    // ===== Membership Domain =====
    /// Room is encrypted and requires E2EE key provision.
    EncryptedRoom {
        /// The encrypted room identifier.
        room_id: String,
        /// The reason encryption is required.
        reason: String,
    },
    /// Cannot leave room due to policy or state constraints.
    CannotLeave {
        /// The room the user cannot leave.
        room_id: String,
        /// The reason leaving is blocked.
        reason: String,
    },
    /// User profile update failed.
    ProfileUpdateFailed {
        /// The affected user identifier.
        user_id: String,
        /// The profile field that failed to update.
        field: String,
    },

    // ===== Sync Domain =====
    /// Sliding sync request timed out or produced inconsistent state.
    SyncTimeout,
    /// Event fetch or pagination failed.
    EventFetch {
        /// Human-readable error message.
        message: String,
    },
    /// Device list update failed.
    DeviceListUpdate {
        /// Human-readable error message.
        message: String,
    },

    // ===== Federation Domain =====
    /// Outbound federation request failed.
    FederationRequestFailed {
        /// The federation destination server name.
        destination: String,
        /// The underlying failure reason.
        reason: String,
    },
    /// Invalid federation event signature or key.
    InvalidSignature {
        /// The event identifier with the invalid signature.
        event_id: String,
    },
    /// Persistent key request failed.
    KeyRequestFailed {
        /// The key identifier that could not be fetched.
        key_id: String,
    },

    // ===== E2EE Domain =====
    /// E2EE configuration or key management failed.
    E2eeError {
        /// Human-readable error message.
        message: String,
    },
    /// Device or session verification failed.
    VerificationFailed {
        /// The reason verification failed.
        reason: String,
    },
    /// Backup decryption or upload failed.
    BackupFailed {
        /// Human-readable error message.
        message: String,
    },

    // ===== Media Domain =====
    /// Media access or processing error.
    MediaError {
        /// Human-readable error message.
        message: String,
    },
    /// Content scanner error.
    ContentScan {
        /// Human-readable error message.
        message: String,
    },
    /// Policy server check failed or unreachable.
    PolicyServerUnavailable {
        /// The reason the policy server was unreachable.
        reason: String,
    },
    /// Action denied by policy server.
    PolicyDenied {
        /// The reason the action was denied.
        reason: String,
        /// Human-readable error message.
        message: String,
    },

    // ===== SAML Domain =====
    /// SAML assertion or response verification failed.
    SamlError {
        /// Human-readable error message.
        message: String,
    },

    // ===== General =====
    /// Resource not found.
    NotFound {
        /// The identifier of the missing resource.
        resource: String,
    },
    /// Permission denied.
    Forbidden {
        /// The reason access was denied.
        reason: String,
    },
    /// Invalid input or request format.
    BadRequest {
        /// Human-readable error message.
        message: String,
    },
    /// Upstream service unavailable.
    ServiceUnavailable {
        /// The unavailable upstream service name.
        service: String,
    },
    /// Internal error — should map to 500.
    Internal {
        /// Human-readable error message.
        message: String,
    },
}

impl ServiceError {
    /// Convert to appropriate `ApiError` with correct HTTP status and Matrix error code.
    pub fn into_api_error(self) -> ApiError {
        match self {
            // Auth -> 401/400
            ServiceError::AuthFailed { message } => ApiError::bad_request(format!("authentication failed: {message}")),
            ServiceError::TokenRevoked { .. } => ApiError::invalid_param("token has been revoked"),
            ServiceError::InvalidResetToken => ApiError::invalid_param("invalid or expired reset token"),
            ServiceError::OidcVerificationFailed { message } => ApiError::unauthorized(message),

            // Application Service -> 500/503
            ServiceError::ApplicationServiceListFailed { message } => {
                ApiError::internal(format!("failed to list active AS: {message}"))
            }
            ServiceError::ApplicationServiceStatsFailed { message } => {
                ApiError::internal(format!("failed to get appservice stats: {message}"))
            }
            ServiceError::ApplicationServiceEventCountFailed { as_id, message } => {
                ApiError::internal(format!("failed to count pending events for {}: {}", as_id, message))
            }
            ServiceError::ApplicationServiceTransactionCountFailed { as_id, message } => {
                ApiError::internal(format!("failed to count pending transactions for {}: {}", as_id, message))
            }

            // Push -> 500
            ServiceError::PushProviderError { provider, message } => {
                ApiError::internal(format!("push provider {} error: {}", provider, message))
            }

            // SAML -> 500
            ServiceError::SamlError { message } => {
                ApiError::internal(format!("SAML error: {message}"))
            }

            // Membership -> 400/403/404
            ServiceError::EncryptedRoom { room_id: _, reason } => {
                ApiError::conflict(format!("room encryption required: {reason}"))
            }
            ServiceError::CannotLeave { room_id, reason } => {
                ApiError::bad_request(format!("cannot leave room {room_id}: {reason}"))
            }
            ServiceError::ProfileUpdateFailed { user_id, field } => {
                ApiError::bad_request(format!("cannot update {field} for {user_id}"))
            }

            // Sync -> 400/503
            ServiceError::SyncTimeout => ApiError::service_unavailable("sync timeout"),
            ServiceError::EventFetch { message } => ApiError::bad_request(format!("event fetch failed: {message}")),
            ServiceError::DeviceListUpdate { message } => {
                ApiError::bad_request(format!("device list update failed: {message}"))
            }

            // Federation -> 502/503
            // Note: synapse-common lacks `bad_gateway`. Using `service_unavailable` (503)
            // for federation failures, and `internal_error` (500) for content scanner.
            ServiceError::FederationRequestFailed { destination, reason } => {
                ApiError::service_unavailable(format!("federation request to {destination} failed: {reason}"))
            }
            ServiceError::InvalidSignature { event_id } => {
                ApiError::forbidden(format!("invalid signature for event {event_id}"))
            }
            ServiceError::KeyRequestFailed { key_id } => ApiError::not_found(format!("key {key_id} not found")),

            // E2EE -> 400/403/500
            ServiceError::E2eeError { message } => ApiError::bad_request(format!("E2EE error: {message}")),
            ServiceError::VerificationFailed { reason } => {
                ApiError::forbidden(format!("verification failed: {reason}"))
            }
            ServiceError::BackupFailed { message } => ApiError::internal(format!("backup failed: {message}")),

            // Media -> 400/500
            ServiceError::MediaError { message } => ApiError::bad_request(message),
            ServiceError::ContentScan { message } => ApiError::internal(format!("content scanner error: {message}")),

            // Policy -> 403/503
            ServiceError::PolicyServerUnavailable { reason } => {
                ApiError::service_unavailable(format!("policy server unavailable: {reason}"))
            }
            ServiceError::PolicyDenied { reason, message } => {
                ApiError::forbidden(format!("policy denied: {reason}, {message}"))
            }

            // General
            ServiceError::NotFound { resource } => ApiError::not_found(format!("{resource} not found")),
            ServiceError::Forbidden { reason } => ApiError::forbidden(reason),
            ServiceError::BadRequest { message } => ApiError::invalid_param(message),
            ServiceError::ServiceUnavailable { service } => {
                ApiError::service_unavailable(format!("{service} unavailable"))
            }
            ServiceError::Internal { message } => ApiError::internal(message),
        }
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::AuthFailed { message } => write!(f, "auth failed: {message}"),
            ServiceError::TokenRevoked { token_type } => {
                write!(f, "token revoked: {token_type}")
            }
            ServiceError::InvalidResetToken => write!(f, "invalid reset token"),
            ServiceError::OidcVerificationFailed { message } => write!(f, "oidc verification failed: {message}"),
            ServiceError::ApplicationServiceListFailed { message } => {
                write!(f, "failed to list active application services: {message}")
            }
            ServiceError::ApplicationServiceStatsFailed { message } => {
                write!(f, "failed to get application service statistics: {message}")
            }
            ServiceError::ApplicationServiceEventCountFailed { as_id, message } => {
                write!(f, "failed to count pending appservice events for '{as_id}': {message}")
            }
            ServiceError::ApplicationServiceTransactionCountFailed { as_id, message } => {
                write!(f, "failed to count pending appservice transactions for '{as_id}': {message}")
            }
            ServiceError::EncryptedRoom { room_id, reason } => {
                write!(f, "encrypted room {room_id}: {reason}")
            }
            ServiceError::CannotLeave { room_id, reason } => {
                write!(f, "cannot leave {room_id}: {reason}")
            }
            ServiceError::ProfileUpdateFailed { user_id, field } => {
                write!(f, "profile update failed for {field} of {user_id}")
            }
            ServiceError::SyncTimeout => write!(f, "sync timeout"),
            ServiceError::EventFetch { message } => write!(f, "event fetch failed: {message}"),
            ServiceError::DeviceListUpdate { message } => {
                write!(f, "device list update failed: {message}")
            }
            ServiceError::FederationRequestFailed { destination, reason } => {
                write!(f, "federation request to {destination} failed: {reason}")
            }
            ServiceError::InvalidSignature { event_id } => {
                write!(f, "invalid signature for event {event_id}")
            }
            ServiceError::KeyRequestFailed { key_id } => {
                write!(f, "key request failed: {key_id}")
            }
            ServiceError::E2eeError { message } => write!(f, "E2EE error: {message}"),
            ServiceError::VerificationFailed { reason } => {
                write!(f, "verification failed: {reason}")
            }
            ServiceError::BackupFailed { message } => write!(f, "backup failed: {message}"),
            ServiceError::MediaError { message } => write!(f, "media error: {message}"),
            ServiceError::ContentScan { message } => {
                write!(f, "content scan error: {message}")
            }
            ServiceError::PolicyServerUnavailable { reason } => {
                write!(f, "policy server unavailable: {reason}")
            }
            ServiceError::PolicyDenied { reason, message } => {
                write!(f, "policy denied: {reason}, {message}")
            }
            ServiceError::PushProviderError { provider, message } => {
                write!(f, "push provider {provider} error: {message}")
            }
            ServiceError::SamlError { message } => write!(f, "SAML error: {message}"),
            ServiceError::NotFound { resource } => write!(f, "{resource} not found"),
            ServiceError::Forbidden { reason } => write!(f, "forbidden: {reason}"),
            ServiceError::BadRequest { message } => write!(f, "bad request: {message}"),
            ServiceError::ServiceUnavailable { service } => {
                write!(f, "service unavailable: {service}")
            }
            ServiceError::Internal { message } => write!(f, "internal error: {message}"),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<ServiceError> for synapse_common::ApiError {
    fn from(e: ServiceError) -> Self {
        e.into_api_error()
    }
}

/// Type alias for results returning `ServiceError`.
pub type ServiceResult<T> = Result<T, ServiceError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_error_to_api_error_status_codes() {
        // Auth -> 400
        let e = ServiceError::AuthFailed { message: "test".into() }.into_api_error();
        assert_eq!(e.http_status(), StatusCode::BAD_REQUEST);

        // Forbidden -> 403
        let e = ServiceError::Forbidden { reason: "access denied".into() }.into_api_error();
        assert_eq!(e.http_status(), StatusCode::FORBIDDEN);

        // NotFound -> 404
        let e = ServiceError::NotFound { resource: "room".into() }.into_api_error();
        assert_eq!(e.http_status(), StatusCode::NOT_FOUND);

        // Internal -> 500
        let e = ServiceError::Internal { message: "panic avoided".into() }.into_api_error();
        assert_eq!(e.http_status(), StatusCode::INTERNAL_SERVER_ERROR);

        // ServiceUnavailable -> 503
        let e = ServiceError::ServiceUnavailable { service: "policy_server".into() }.into_api_error();
        assert_eq!(e.http_status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_federation_and_backup_map_to_available_constructors() {
        // FederationRequestFailed maps to 503 (no bad_gateway in ApiError).
        let e = ServiceError::FederationRequestFailed { destination: "hs.example".into(), reason: "timeout".into() }
            .into_api_error();
        assert_eq!(e.http_status(), StatusCode::SERVICE_UNAVAILABLE);

        // BackupFailed maps to 500.
        let e = ServiceError::BackupFailed { message: "decrypt".into() }.into_api_error();
        assert_eq!(e.http_status(), StatusCode::INTERNAL_SERVER_ERROR);

        // TokenRevoked maps to 400 (invalid_param).
        let e = ServiceError::TokenRevoked { token_type: "refresh".into() }.into_api_error();
        assert_eq!(e.http_status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_display_renders_message() {
        let s = ServiceError::NotFound { resource: "room".into() };
        assert_eq!(s.to_string(), "room not found");
    }
}
