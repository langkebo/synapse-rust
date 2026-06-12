pub use synapse_common::error::{
    init_error_metrics, ApiError, ApiErrorKind, ApiResponse, ApiResult, ErrorContext, ErrorSource,
    MatrixErrorCode,
};

// Re-export macros
pub use synapse_common::{
    bail, dbg_context, ensure, ensure_forbidden, ensure_not_found, ensure_unauthorized,
    safe_unwrap, safe_unwrap_ctx, wrap_result,
};

// ---------------------------------------------------------------------------
// Conversion helpers (avoiding Orphan Rule issues)
// ---------------------------------------------------------------------------

pub fn crypto_error_to_api_error(err: crate::e2ee::crypto::CryptoError) -> ApiError {
    match err {
        crate::e2ee::crypto::CryptoError::EncryptionError(msg) => ApiError::encryption_error(msg),
        crate::e2ee::crypto::CryptoError::DecryptionError(msg) => ApiError::decryption_error(msg),
        _ => ApiError::crypto(err.to_string()),
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn ed25519_error_to_api_error(err: ed25519_dalek::ed25519::Error) -> ApiError {
    ApiError::crypto(format!("Ed25519 error: {err}"))
}
