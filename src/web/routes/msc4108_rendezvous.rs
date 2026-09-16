//! MSC4108 Rendezvous endpoints for QR code sign-in.
//!
//! Implements the unstable MSC4108 rendezvous transport protocol used by
//! the SDK's `MSC4108RendezvousSession`. Data is exchanged as `text/plain`
//! opaque blobs with ETag-based conditional requests.
//!
//! Endpoints:
//!   POST   /_matrix/client/unstable/org.matrix.msc4108/rendezvous          — create session
//!   GET    /_matrix/client/unstable/org.matrix.msc4108/rendezvous/{id}     — poll for data
//!   PUT    /_matrix/client/unstable/org.matrix.msc4108/rendezvous/{id}     — update data
//!   DELETE /_matrix/client/unstable/org.matrix.msc4108/rendezvous/{id}     — close session

use crate::common::ApiError;
use crate::web::routes::context::AuthContext;
use crate::web::routes::extractors::SessionId;
use crate::web::routes::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use synapse_storage::rendezvous::Msc4108UpdateOutcome;

const MSC4108_TTL_MS: i64 = 5 * 60 * 1000; // 5 minutes
const MSC4108_MAX_PAYLOAD_BYTES: usize = 4 * 1024; // 4 KiB per MSC4108

/// See [`create_msc4108_rendezvous_router`].
pub fn create_msc4108_rendezvous_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/unstable/org.matrix.msc4108/rendezvous", post(create_session))
        .route(
            "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}",
            get(get_session).put(update_session).delete(delete_session),
        )
        .with_state(state)
}

/// Build the full rendezvous URL for a session.
fn build_rendezvous_url(ctx: &AuthContext, session_id: &str) -> String {
    format!(
        "{}/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{}",
        ctx.config.server.get_public_baseurl().trim_end_matches('/'),
        session_id
    )
}

/// POST /rendezvous — Create a new MSC4108 rendezvous session.
///
/// Request body: `text/plain` (initial encrypted payload from the SDK)
/// Response: `{"url": "..."}` + **ETag/Expires/Last-Modified/Cache-Control/Pragma** headers
async fn create_session(
    State(ctx): State<AuthContext>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, ApiError> {
    // Validate Content-Type per MSC4108 (required, must be text/plain).
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    if content_type.is_empty() {
        return Err(ApiError::missing_param("Content-Type header is required".to_string()));
    }
    if !content_type.starts_with("text/plain") {
        return Err(ApiError::invalid_param("Content-Type must be text/plain".to_string()));
    }

    // Enforce the 4KB maximum payload size.
    if body.len() > MSC4108_MAX_PAYLOAD_BYTES {
        return Err(ApiError::too_large("Payload exceeds maximum size of 4KB".to_string()));
    }

    let (session_id, etag, created_ts, expires_at) = ctx
        .rendezvous_storage
        .create_msc4108_session(&body, MSC4108_TTL_MS)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create MSC4108 session", e))?;

    let url = build_rendezvous_url(&ctx, &session_id);
    let expires_http_date = http_date_from_millis(expires_at);
    let last_modified_http = http_date_from_millis(created_ts);

    let response = (
        StatusCode::OK,
        [
            (header::ETAG, etag.as_str()),
            (header::EXPIRES, expires_http_date.as_str()),
            (header::LAST_MODIFIED, last_modified_http.as_str()),
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
            (header::ACCESS_CONTROL_EXPOSE_HEADERS, "ETag"),
            (header::CONTENT_TYPE, "application/json"),
        ],
        axum::Json(serde_json::json!({ "url": url })),
    );

    Ok(response.into_response())
}

/// GET /rendezvous/{session_id} — Poll for data.
///
/// Supports `If-None-Match` for conditional polling.
/// Returns `text/plain` body + standard MSC4108 caching headers, or 304 if not modified.
async fn get_session(
    State(ctx): State<AuthContext>,
    Path(session_id): Path<SessionId>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let result = ctx
        .rendezvous_storage
        .get_msc4108_data(&session_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get MSC4108 data", e))?;

    let (data, etag, updated_ts, expires_at) = match result {
        Some(v) => v,
        None => return Err(ApiError::not_found("Rendezvous session not found or expired".to_string())),
    };

    let expires_http_date = http_date_from_millis(expires_at);
    let last_modified_http = http_date_from_millis(updated_ts);

    // Check If-None-Match for conditional polling
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if let Ok(client_etag) = if_none_match.to_str() {
            if client_etag == etag || client_etag == "*" {
                // 304 with full common headers, no body
                return Ok((
                    StatusCode::NOT_MODIFIED,
                    [
                        (header::ETAG, etag.as_str()),
                        (header::EXPIRES, expires_http_date.as_str()),
                        (header::LAST_MODIFIED, last_modified_http.as_str()),
                        (header::CACHE_CONTROL, "no-store"),
                        (header::PRAGMA, "no-cache"),
                    ],
                    Body::empty(),
                )
                    .into_response());
            }
        }
    }

    Ok((
        StatusCode::OK,
        [
            (header::ETAG, etag.as_str()),
            (header::EXPIRES, expires_http_date.as_str()),
            (header::LAST_MODIFIED, last_modified_http.as_str()),
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
            (header::CONTENT_TYPE, "text/plain"),
        ],
        Body::from(data),
    )
        .into_response())
}

/// PUT /rendezvous/{session_id} — Update session data.
///
/// Supports `If-Match` for conditional updates.
/// Request body: `text/plain` (new encrypted payload)
/// Response: `202 Accepted` + new `ETag` header + common caching headers.
/// A mismatched `If-Match` produces `412 Precondition Failed` with the unstable
/// `org.matrix.msc4108.errcode: M_CONCURRENT_WRITE` field.
async fn update_session(
    State(ctx): State<AuthContext>,
    Path(session_id): Path<SessionId>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, ApiError> {
    // Validate Content-Type per MSC4108 (required, must be text/plain).
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    if content_type.is_empty() {
        return Err(ApiError::missing_param("Content-Type header is required".to_string()));
    }
    if !content_type.starts_with("text/plain") {
        return Err(ApiError::invalid_param("Content-Type must be text/plain".to_string()));
    }

    // Enforce the 4KB maximum payload size.
    if body.len() > MSC4108_MAX_PAYLOAD_BYTES {
        return Err(ApiError::too_large("Payload exceeds maximum size of 4KB".to_string()));
    }

    // If-Match is required per MSC4108 for PUT, but the SDK preceding versions may omit it.
    // We preserve backward compatibility: treat a missing/empty If-Match as an unconditional update.
    let if_match = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()).filter(|s| !s.is_empty());

    let outcome = ctx
        .rendezvous_storage
        .update_msc4108_data(&session_id, &body, if_match)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to update MSC4108 data", e))?;

    match outcome {
        Msc4108UpdateOutcome::Updated { new_etag, updated_ts, expires_at } => {
            let expires_http_date = http_date_from_millis(expires_at);
            let last_modified_http = http_date_from_millis(updated_ts);
            Ok((
                StatusCode::ACCEPTED,
                [
                    (header::ETAG, new_etag.as_str()),
                    (header::EXPIRES, expires_http_date.as_str()),
                    (header::LAST_MODIFIED, last_modified_http.as_str()),
                    (header::CACHE_CONTROL, "no-store"),
                    (header::PRAGMA, "no-cache"),
                    (header::CONTENT_TYPE, "text/plain"),
                ],
                Body::empty(),
            )
                .into_response())
        }
        Msc4108UpdateOutcome::PreconditionFailed { current_etag, updated_ts, expires_at } => {
            // 412 with unstable errcode prefix per MSC4108 §Unstable prefix.
            let expires_http_date = http_date_from_millis(expires_at);
            let last_modified_http = http_date_from_millis(updated_ts);
            let body = axum::Json(serde_json::json!({
                "errcode": "M_UNKNOWN",
                "org.matrix.msc4108.errcode": "M_CONCURRENT_WRITE",
                "error": "ETag mismatch - data was modified"
            }));
            Ok((
                StatusCode::PRECONDITION_FAILED,
                [
                    (header::ETAG, current_etag.as_str()),
                    (header::EXPIRES, expires_http_date.as_str()),
                    (header::LAST_MODIFIED, last_modified_http.as_str()),
                    (header::CACHE_CONTROL, "no-store"),
                    (header::PRAGMA, "no-cache"),
                ],
                body,
            )
                .into_response())
        }
        Msc4108UpdateOutcome::NotFound => {
            Err(ApiError::not_found("Rendezvous session not found or expired".to_string()))
        }
    }
}

/// DELETE /rendezvous/{session_id} — Close session.
///
/// Returns `204 No Content` on success. Per MSC4108, unknown or expired
/// session ids respond with `404 Not Found`.
async fn delete_session(
    State(ctx): State<AuthContext>,
    Path(session_id): Path<SessionId>,
) -> Result<Response, ApiError> {
    let deleted = ctx
        .rendezvous_storage
        .delete_msc4108_session(&session_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to delete MSC4108 session", e))?;

    if !deleted {
        return Err(ApiError::not_found("Rendezvous session not found or expired".to_string()));
    }

    Ok(delete_success_response())
}

/// Build the `204 No Content` response for a successful DELETE.
///
/// MSC4108 requires the common caching headers on every rendezvous response,
/// including the DELETE confirmation: without `Cache-Control: no-store` /
/// `Pragma: no-cache` an intermediary may retain a deleted session's metadata;
/// `Last-Modified` marks the moment the resource ceased to exist. Extracted as
/// a free function so the header contract is testable without a fully-wired
/// `AuthContext` (S-15: the previous test asserted a locally-constructed array
/// and passed even with zero headers emitted).
pub fn delete_success_response() -> Response {
    let last_modified = http_date_from_millis(chrono::Utc::now().timestamp_millis());
    (
        StatusCode::NO_CONTENT,
        [
            (header::LAST_MODIFIED, last_modified.as_str()),
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Body::empty(),
    )
        .into_response()
}

/// Convert millisecond timestamp to HTTP date format (RFC 7231).
fn http_date_from_millis(millis: i64) -> String {
    let secs = millis / 1000;
    let dt = chrono::DateTime::from_timestamp(secs, 0).unwrap_or_else(chrono::Utc::now);
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}
