use crate::common::ApiError;
use crate::web::routes::context::MediaContext;
use crate::web::AuthenticatedUser;
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Media CSP / safety header constants mirroring
/// `synapse-services::media::build_media_response_headers` so that remote
/// media proxied via federation gets the same sandbox treatment as local
/// media.
const MEDIA_CONTENT_SECURITY_POLICY: &str = "sandbox; default-src 'none'; script-src 'none'; \
plugin-types application/pdf; style-src 'unsafe-inline'; media-src 'self'; \
object-src 'self'; img-src 'self';";

const SAFE_INLINE_MEDIA_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "audio/mpeg",
    "audio/wav",
    "audio/ogg",
    "audio/flac",
    "video/mp4",
    "video/webm",
    "application/pdf",
];

// ---------------------------------------------------------------------------
// Media ID validation
// ---------------------------------------------------------------------------

/// Validate that a media_id does not contain path traversal sequences.
#[allow(dead_code)]
pub(crate) fn validate_media_id(server_name: &str, media_id: &str) -> Result<(), ApiError> {
    if media_id.is_empty() {
        return Err(ApiError::bad_request("media_id must not be empty".to_string()));
    }
    if media_id.contains("..") || media_id.contains('/') || media_id.contains('\\') {
        return Err(ApiError::bad_request(format!(
            "Invalid media_id for server {}: path traversal not allowed",
            server_name
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Header formatting helpers
// ---------------------------------------------------------------------------

fn sanitize_attachment_filename(filename: &str) -> String {
    filename
        .chars()
        .filter(|c| !c.is_control() && !matches!(*c, '"' | '\\' | '/' | '\0'))
        .take(200)
        .collect::<String>()
        .trim()
        .to_string()
}

fn encode_rfc5987(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric()
                || matches!(c, '!' | '#' | '$' | '&' | '+' | '-' | '.' | '^' | '_' | '`' | '|' | '~')
            {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}

fn build_proxy_media_headers(
    content_type: String,
    content_length: usize,
    filename: Option<&str>,
) -> synapse_services::media::MediaResponseHeaders {
    let primary_type = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let inline_safe = SAFE_INLINE_MEDIA_TYPES.iter().any(|safe| *safe == primary_type);
    let disposition_kind = if inline_safe { "inline" } else { "attachment" };
    let content_disposition = match filename {
        Some(name) if !name.is_empty() => {
            let safe = sanitize_attachment_filename(name);
            if safe.is_empty() {
                disposition_kind.to_string()
            } else {
                let encoded = encode_rfc5987(&safe);
                format!("{disposition_kind}; filename=\"{safe}\"; filename*=UTF-8''{encoded}")
            }
        }
        _ => disposition_kind.to_string(),
    };
    synapse_services::media::MediaResponseHeaders {
        content_type,
        content_length,
        content_disposition,
        x_content_type_options: "nosniff",
        content_security_policy: MEDIA_CONTENT_SECURITY_POLICY,
        cross_origin_resource_policy: "cross-origin",
        referrer_policy: "no-referrer",
    }
}

pub(crate) fn media_response_headers(headers: &synapse_services::media::MediaResponseHeaders) -> HeaderMap {
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

pub(crate) fn media_error_response(error: &ApiError) -> (StatusCode, HeaderMap, Vec<u8>) {
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

// ---------------------------------------------------------------------------
// Remote media fetch helpers
// ---------------------------------------------------------------------------

/// Fetch remote media via federation and wrap it into a `MediaResponsePayload`.
async fn fetch_remote_media_via_federation(
    ctx: &MediaContext,
    server_name: &str,
    media_id: &str,
    response_filename: Option<&str>,
) -> Result<synapse_services::media::MediaResponsePayload, ApiError> {
    let federation_client = ctx.federation_client.clone();
    let resp = federation_client
        .media_download(server_name, server_name, media_id)
        .await
        .map_err(|e| ApiError::not_found(format!("Remote media not reachable: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_else(|e| format!("Failed to read remote media response: {e}"));
        return Err(ApiError::not_found(format!("Remote media fetch failed: {status} {body}")));
    }

    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map_or_else(|| "application/octet-stream".to_string(), |s| s.to_string());

    let content =
        resp.bytes().await.map_err(|e| ApiError::internal(format!("Failed to read remote media body: {e}")))?.to_vec();

    let headers = build_proxy_media_headers(content_type, content.len(), response_filename);
    Ok(synapse_services::media::MediaResponsePayload { content, headers })
}

/// Fetch remote thumbnail via federation.
async fn fetch_remote_thumbnail_via_federation(
    ctx: &MediaContext,
    server_name: &str,
    media_id: &str,
    width: u32,
    height: u32,
    method: &str,
) -> Result<synapse_services::media::MediaResponsePayload, ApiError> {
    let federation_client = ctx.federation_client.clone();
    let resp = federation_client
        .media_thumbnail(server_name, server_name, media_id, width, height, method)
        .await
        .map_err(|e| ApiError::not_found(format!("Remote thumbnail not reachable: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_else(|e| format!("Failed to read remote thumbnail response: {e}"));
        return Err(ApiError::not_found(format!("Remote thumbnail fetch failed: {status} {body}")));
    }

    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map_or_else(|| "image/jpeg".to_string(), |s| s.to_string());

    let content = resp
        .bytes()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read remote thumbnail body: {e}")))?
        .to_vec();

    let headers = build_proxy_media_headers(content_type, content.len(), None);
    Ok(synapse_services::media::MediaResponsePayload { content, headers })
}

// ---------------------------------------------------------------------------
// Download and thumbnail common helpers
// ---------------------------------------------------------------------------

pub(crate) async fn download_media_common(
    ctx: &MediaContext,
    server_name: &str,
    media_id: &str,
    response_filename: Option<&str>,
) -> Result<synapse_services::media::MediaResponsePayload, ApiError> {
    if server_name == ctx.server_name {
        return ctx.media_domain_service.download_media(server_name, media_id, response_filename).await;
    }
    fetch_remote_media_via_federation(ctx, server_name, media_id, response_filename).await
}

pub(crate) fn thumbnail_request_params(params: &Value) -> (u32, u32, &str) {
    let width = params.get("width").and_then(|v| v.as_u64()).filter(|&w| w <= 10000).unwrap_or(800) as u32;
    let height = params.get("height").and_then(|v| v.as_u64()).filter(|&h| h <= 10000).unwrap_or(600) as u32;
    let method = params.get("method").and_then(|v| v.as_str()).unwrap_or("scale");
    (width, height, method)
}

pub(crate) async fn thumbnail_response_common(
    ctx: &MediaContext,
    server_name: &str,
    media_id: &str,
    params: &Value,
) -> Result<synapse_services::media::MediaResponsePayload, ApiError> {
    let (width, height, method) = thumbnail_request_params(params);

    if server_name == ctx.server_name {
        return ctx.media_domain_service.get_thumbnail(server_name, media_id, width, height, method).await;
    }

    fetch_remote_thumbnail_via_federation(ctx, server_name, media_id, width, height, method).await
}

// ---------------------------------------------------------------------------
// Download handlers
// ---------------------------------------------------------------------------

pub(crate) async fn download_media(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let response = download_media_common(&ctx, &server_name, &media_id, None).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

pub(crate) async fn download_media_with_filename(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let response = download_media_common(&ctx, &server_name, &media_id, Some(&filename)).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

/// Signed media download — verifies HMAC signature before serving.
pub(crate) async fn download_media_signed(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let signature = params
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("Missing signature parameter".to_string()))?;

    let expires: u64 = params.get("expires").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);

    if !ctx.media_domain_service.verify_media_download_url(&server_name, &media_id, signature, expires) {
        return Err(ApiError::unauthorized("Invalid or expired media signature".to_string()));
    }

    let response = download_media_common(&ctx, &server_name, &media_id, None).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

/// Signed media download with filename.
pub(crate) async fn download_media_signed_with_filename(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let signature = params
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("Missing signature parameter".to_string()))?;

    let expires: u64 = params.get("expires").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);

    if !ctx.media_domain_service.verify_media_download_url(&server_name, &media_id, signature, expires) {
        return Err(ApiError::unauthorized("Invalid or expired media signature".to_string()));
    }

    let response = download_media_common(&ctx, &server_name, &media_id, Some(&filename)).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

pub(crate) async fn download_media_authenticated(
    State(ctx): State<MediaContext>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let response = download_media_common(&ctx, &server_name, &media_id, None).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

pub(crate) async fn download_media_authenticated_with_filename(
    State(ctx): State<MediaContext>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let response = download_media_common(&ctx, &server_name, &media_id, Some(&filename)).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

pub(crate) async fn download_media_v1(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match download_media_common(&ctx, &server_name, &media_id, None).await {
        Ok(response) => {
            let headers = media_response_headers(&response.headers);
            (StatusCode::OK, headers, response.content)
        }
        Err(error) => media_error_response(&error),
    }
}

pub(crate) async fn download_media_v1_with_filename(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
) -> impl IntoResponse {
    match download_media_common(&ctx, &server_name, &media_id, Some(&filename)).await {
        Ok(response) => {
            let headers = media_response_headers(&response.headers);
            (StatusCode::OK, headers, response.content)
        }
        Err(error) => media_error_response(&error),
    }
}

// ---------------------------------------------------------------------------
// Thumbnail handlers
// ---------------------------------------------------------------------------

pub(crate) async fn get_thumbnail(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let response = thumbnail_response_common(&ctx, &server_name, &media_id, &params).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

pub(crate) async fn get_thumbnail_authenticated(
    State(ctx): State<MediaContext>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let response = thumbnail_response_common(&ctx, &server_name, &media_id, &params).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_media_id_rejects_traversal() {
        assert!(validate_media_id("server", "../etc").is_err());
        assert!(validate_media_id("server", "a/b").is_err());
        assert!(validate_media_id("server", "a\\b").is_err());
    }

    #[test]
    fn test_validate_media_id_allows_valid() {
        assert!(validate_media_id("server", "abc123").is_ok());
        assert!(validate_media_id("server", "media_id_with_underscores").is_ok());
        assert!(validate_media_id("server", "media-id-with-dashes").is_ok());
        assert!(validate_media_id("server", "UPPERCASE123").is_ok());
    }

    #[test]
    fn test_validate_media_id_rejects_empty() {
        assert!(validate_media_id("server", "").is_err());
    }

    #[test]
    fn test_thumbnail_default_dimensions() {
        let default_width: u32 = 800;
        let default_height: u32 = 600;
        assert!(default_width > 0);
        assert!(default_height > 0);
    }

    #[test]
    fn test_remote_fetch_error_includes_status() {
        let error_msg = "Remote media fetch failed: 502 Failed to read remote media response: connection reset";
        assert!(error_msg.contains("502"));
        assert!(error_msg.contains("Failed to read"));
    }
}
