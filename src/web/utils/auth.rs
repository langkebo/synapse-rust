use crate::common::ApiError;
use axum::http::HeaderMap;

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
        .ok_or_else(|| {
            ApiError::unauthorized("Missing or invalid authorization header".to_string())
        })?;

    if token.trim().is_empty() {
        return Err(ApiError::unauthorized(
            "Empty authorization token".to_string(),
        ));
    }

    Ok(token)
}
