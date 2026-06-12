use axum::http::HeaderMap;
use synapse_common::ApiError;

pub(crate) fn generate_request_id() -> String {
    format!("req-{}", uuid::Uuid::new_v4())
}

pub(crate) fn resolve_request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .unwrap_or_else(generate_request_id)
}

pub(crate) fn bearer_token_opt(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Result<String, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::unauthorized("Missing or invalid authorization header".to_string()))?;

    if token.trim().is_empty() {
        return Err(ApiError::unauthorized("Empty authorization token".to_string()));
    }

    Ok(token)
}
