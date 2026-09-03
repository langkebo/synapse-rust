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

const MSC4108_TTL_MS: i64 = 5 * 60 * 1000; // 5 minutes

pub fn create_msc4108_rendezvous_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/unstable/org.matrix.msc4108/rendezvous", post(create_session))
        .route(
            "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}",
            get(get_session).put(update_session).delete(delete_session),
        )
        .with_state(state)
}

pub fn msc4108_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_matrix/client/unstable/org.matrix.msc4108/rendezvous"),
        (Method::GET, "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}"),
        (Method::PUT, "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}"),
        (Method::DELETE, "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "msc4108_rendezvous"))
    .collect()
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
/// Response: `{"url": "..."}` + `ETag` + `Expires` headers
async fn create_session(State(ctx): State<AuthContext>, body: String) -> Result<Response, ApiError> {
    let (session_id, etag, expires_at) = ctx
        .rendezvous_storage
        .create_msc4108_session(&body, MSC4108_TTL_MS)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to create MSC4108 session", &e))?;

    let url = build_rendezvous_url(&ctx, &session_id);
    let expires_http_date = http_date_from_millis(expires_at);

    let response = (
        StatusCode::OK,
        [
            (header::ETAG, etag.as_str()),
            (header::EXPIRES, expires_http_date.as_str()),
            (header::CONTENT_TYPE, "application/json"),
        ],
        axum::Json(serde_json::json!({ "url": url })),
    );

    Ok(response.into_response())
}

/// GET /rendezvous/{session_id} — Poll for data.
///
/// Supports `If-None-Match` for conditional polling.
/// Returns `text/plain` body + `ETag` header, or 304 if not modified.
async fn get_session(
    State(ctx): State<AuthContext>,
    Path(session_id): Path<SessionId>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let result = ctx
        .rendezvous_storage
        .get_msc4108_data(&session_id)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to get MSC4108 data", &e))?;

    let (data, etag) = match result {
        Some(v) => v,
        None => return Err(ApiError::not_found("Rendezvous session not found or expired".to_string())),
    };

    // Check If-None-Match for conditional polling
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if let Ok(client_etag) = if_none_match.to_str() {
            if client_etag == etag || client_etag == "*" {
                return Ok((StatusCode::NOT_MODIFIED, [(header::ETAG, etag.as_str())], Body::empty()).into_response());
            }
        }
    }

    Ok((StatusCode::OK, [(header::ETAG, etag.as_str()), (header::CONTENT_TYPE, "text/plain")], Body::from(data))
        .into_response())
}

/// PUT /rendezvous/{session_id} — Update session data.
///
/// Supports `If-Match` for conditional updates.
/// Request body: `text/plain` (new encrypted payload)
/// Response: new `ETag` header
async fn update_session(
    State(ctx): State<AuthContext>,
    Path(session_id): Path<SessionId>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, ApiError> {
    let if_match = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()).filter(|s| !s.is_empty());

    let new_etag = ctx
        .rendezvous_storage
        .update_msc4108_data(&session_id, &body, if_match)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to update MSC4108 data", &e))?
        .ok_or_else(|| ApiError::bad_request("ETag mismatch or session expired".to_string()))?;

    Ok((StatusCode::OK, [(header::ETAG, new_etag.as_str()), (header::CONTENT_TYPE, "text/plain")], Body::empty())
        .into_response())
}

/// DELETE /rendezvous/{session_id} — Close session.
async fn delete_session(
    State(ctx): State<AuthContext>,
    Path(session_id): Path<SessionId>,
) -> Result<Response, ApiError> {
    ctx.rendezvous_storage
        .delete_msc4108_session(&session_id)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to delete MSC4108 session", &e))?;

    Ok((StatusCode::OK, Body::empty()).into_response())
}

/// Convert millisecond timestamp to HTTP date format (RFC 7231).
fn http_date_from_millis(millis: i64) -> String {
    let secs = millis / 1000;
    let dt = chrono::DateTime::from_timestamp(secs, 0).unwrap_or_else(chrono::Utc::now);
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}
