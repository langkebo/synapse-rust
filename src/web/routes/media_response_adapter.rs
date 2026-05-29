use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use serde_json::json;

use crate::common::ApiError;
use crate::services::media::{MediaBinaryPayload, MediaResponseHeaders, MediaResponsePayload};

pub(crate) type MediaHttpResponse = (StatusCode, HeaderMap, Vec<u8>);

pub(crate) fn media_response_headers(headers: &MediaResponseHeaders) -> HeaderMap {
    let mut out = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&headers.content_type) {
        out.insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&headers.content_length.to_string()) {
        out.insert(header::CONTENT_LENGTH, v);
    }
    if let Ok(v) = HeaderValue::from_str(&headers.content_disposition) {
        out.insert(header::CONTENT_DISPOSITION, v);
    }
    out.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static(headers.x_content_type_options));
    out.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(headers.content_security_policy));
    out.insert(
        axum::http::HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static(headers.cross_origin_resource_policy),
    );
    out.insert(header::REFERRER_POLICY, HeaderValue::from_static(headers.referrer_policy));
    out
}

pub(crate) fn media_success_response(response: MediaResponsePayload) -> MediaHttpResponse {
    let headers = media_response_headers(&response.headers);
    (StatusCode::OK, headers, response.content)
}

pub(crate) fn binary_success_response(content_type: &str, content: Vec<u8>) -> MediaHttpResponse {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(content_type) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&content.len().to_string()) {
        headers.insert(header::CONTENT_LENGTH, v);
    }
    (StatusCode::OK, headers, content)
}

pub(crate) fn binary_payload_success_response(payload: MediaBinaryPayload) -> MediaHttpResponse {
    binary_success_response(&payload.content_type, payload.content)
}

pub(crate) fn media_result_response(
    result: Result<MediaResponsePayload, ApiError>,
) -> Result<MediaHttpResponse, ApiError> {
    result.map(media_success_response)
}

pub(crate) fn media_legacy_result_response(result: Result<MediaResponsePayload, ApiError>) -> MediaHttpResponse {
    match result {
        Ok(response) => media_success_response(response),
        Err(error) => api_error_response(&error),
    }
}

pub(crate) fn api_error_response(error: &ApiError) -> MediaHttpResponse {
    let status = error.http_status();
    let error_body = serde_json::to_vec(&json!({
        "errcode": error.code(),
        "error": error.message()
    }))
    .unwrap_or_else(|_| br#"{"errcode":"M_UNKNOWN","error":"Internal error"}"#.to_vec());
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Ok(v) = HeaderValue::from_str(&error_body.len().to_string()) {
        headers.insert(header::CONTENT_LENGTH, v);
    }
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    (status, headers, error_body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_response_returns_json_body_and_headers() {
        let (status, headers, body) = api_error_response(&ApiError::forbidden("forbidden".to_string()));

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()), Some("application/json"));
        assert_eq!(headers.get(header::X_CONTENT_TYPE_OPTIONS).and_then(|v| v.to_str().ok()), Some("nosniff"));
        let value: serde_json::Value = serde_json::from_slice(&body).expect("error body should be valid json");
        assert_eq!(value["errcode"], "M_FORBIDDEN");
        assert_eq!(value["error"], "forbidden");
    }

    #[test]
    fn test_binary_success_response_sets_content_headers() {
        let (status, headers, body) = binary_success_response("application/octet-stream", b"abc".to_vec());

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"abc");
        assert_eq!(headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()), Some("application/octet-stream"));
        assert_eq!(headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()), Some("3"));
    }
}
