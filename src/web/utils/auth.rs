use crate::common::ApiError;
use axum::http::HeaderMap;

pub(crate) fn generate_request_id() -> String {
    format!("req-{}", uuid::Uuid::new_v4())
}

pub(crate) fn resolve_request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map_or_else(generate_request_id, |v| v.to_string())
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

/// Extract bearer token from Authorization header, falling back to
/// `access_token=` query parameter if the header is missing or invalid.
pub(crate) fn extract_token(headers: &HeaderMap, uri: &str) -> Result<String, ApiError> {
    match bearer_token(headers) {
        Ok(token) => Ok(token),
        Err(header_err) => {
            if let Some(query) = uri.split('?').nth(1) {
                for pair in query.split('&') {
                    if let Some(value) = pair.strip_prefix("access_token=") {
                        return Ok(value.to_string());
                    }
                }
            }
            Err(header_err)
        }
    }
}

/// Like `extract_token` but returns `None` instead of an error when
/// no token is found.
pub(crate) fn extract_token_opt(headers: &HeaderMap, uri: &str) -> Option<String> {
    if let Some(token) = bearer_token_opt(headers) {
        return Some(token);
    }
    if let Some(query) = uri.split('?').nth(1) {
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("access_token=") {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use synapse_common::MatrixErrorCode;

    #[test]
    fn test_generate_request_id_format() {
        let id = generate_request_id();
        assert!(id.starts_with("req-"));
        assert_eq!(id.len(), "req-".len() + 36); // UUID v4 is 36 chars
    }

    #[test]
    fn test_generate_request_id_unique() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_resolve_request_id_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "test-123".parse().unwrap());
        assert_eq!(resolve_request_id(&headers), "test-123");
    }

    #[test]
    fn test_resolve_request_id_missing_header() {
        let headers = HeaderMap::new();
        let id = resolve_request_id(&headers);
        assert!(id.starts_with("req-"));
    }

    #[test]
    fn test_resolve_request_id_empty_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "".parse().unwrap());
        let id = resolve_request_id(&headers);
        assert!(id.starts_with("req-"));
    }

    #[test]
    fn test_resolve_request_id_whitespace_only() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "   ".parse().unwrap());
        let id = resolve_request_id(&headers);
        assert!(id.starts_with("req-"));
    }

    #[test]
    fn test_bearer_token_opt_valid() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer my-token-123".parse().unwrap());
        assert_eq!(bearer_token_opt(&headers), Some("my-token-123".to_string()));
    }

    #[test]
    fn test_bearer_token_opt_missing_header() {
        let headers = HeaderMap::new();
        assert_eq!(bearer_token_opt(&headers), None);
    }

    #[test]
    fn test_bearer_token_opt_no_bearer_prefix() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Basic abc123".parse().unwrap());
        assert_eq!(bearer_token_opt(&headers), None);
    }

    #[test]
    fn test_bearer_token_opt_empty_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer ".parse().unwrap());
        assert_eq!(bearer_token_opt(&headers), None);
    }

    #[test]
    fn test_bearer_token_opt_whitespace_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer    ".parse().unwrap());
        assert_eq!(bearer_token_opt(&headers), None);
    }

    #[test]
    fn test_bearer_token_valid() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer my-token-123".parse().unwrap());
        assert_eq!(bearer_token(&headers).unwrap(), "my-token-123");
    }

    #[test]
    fn test_bearer_token_missing_header() {
        let headers = HeaderMap::new();
        assert!(bearer_token(&headers).is_err());
    }

    #[test]
    fn test_bearer_token_no_bearer_prefix() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Basic abc123".parse().unwrap());
        let err = bearer_token(&headers).unwrap_err();
        assert!(err.code_is(MatrixErrorCode::Unauthorized));
    }

    #[test]
    fn test_bearer_token_empty() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer ".parse().unwrap());
        let err = bearer_token(&headers).unwrap_err();
        assert!(err.code_is(MatrixErrorCode::Unauthorized));
    }

    // === extract_token tests ===

    #[test]
    fn test_extract_token_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer header-token".parse().unwrap());
        assert_eq!(extract_token(&headers, "/test").unwrap(), "header-token");
    }

    #[test]
    fn test_extract_token_from_query_param() {
        let headers = HeaderMap::new();
        let uri = "/_matrix/client/v3/sync?access_token=query-token&other=value";
        assert_eq!(extract_token(&headers, uri).unwrap(), "query-token");
    }

    #[test]
    fn test_extract_token_header_takes_priority() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer header-token".parse().unwrap());
        let uri = "/test?access_token=query-token";
        assert_eq!(extract_token(&headers, uri).unwrap(), "header-token");
    }

    #[test]
    fn test_extract_token_query_only() {
        let headers = HeaderMap::new();
        let uri = "/test?access_token=abc123";
        assert_eq!(extract_token(&headers, uri).unwrap(), "abc123");
    }

    #[test]
    fn test_extract_token_no_token_at_all() {
        let headers = HeaderMap::new();
        let uri = "/test";
        assert!(extract_token(&headers, uri).is_err());
    }

    #[test]
    fn test_extract_token_query_no_access_token_param() {
        let headers = HeaderMap::new();
        let uri = "/test?other_param=value";
        assert!(extract_token(&headers, uri).is_err());
    }

    // === extract_token_opt tests ===

    #[test]
    fn test_extract_token_opt_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer my-token".parse().unwrap());
        assert_eq!(extract_token_opt(&headers, "/test"), Some("my-token".to_string()));
    }

    #[test]
    fn test_extract_token_opt_from_query() {
        let headers = HeaderMap::new();
        let uri = "/test?access_token=q-token";
        assert_eq!(extract_token_opt(&headers, uri), Some("q-token".to_string()));
    }

    #[test]
    fn test_extract_token_opt_none() {
        let headers = HeaderMap::new();
        assert_eq!(extract_token_opt(&headers, "/test"), None);
    }

    #[test]
    fn test_extract_token_opt_empty_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer ".parse().unwrap());
        let uri = "/test?access_token=fallback";
        assert_eq!(extract_token_opt(&headers, uri), Some("fallback".to_string()));
    }
}
