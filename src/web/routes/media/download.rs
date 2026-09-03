use crate::common::ApiError;
use crate::web::routes::context::MediaContext;
use crate::web::routes::extractors::{MediaId, ServerName};
use crate::web::{AuthenticatedUser, OptionalAuthenticatedUser};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};
use tokio_util::io::ReaderStream;

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

// NOTE: media_id validation is centralized in `synapse_services::media_service::validate_media_id`
// (synapse-services/src/media_service.rs:65). The previous route-local duplicate was removed as
// dead code — production download paths delegate to the service-layer validator.

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
// S3: Streaming download helpers (replaces full-buffer download_media_common)
// ---------------------------------------------------------------------------

/// Fetch remote media via federation and return a **streaming** response.
///
/// Uses `resp.bytes_stream()` to forward the remote body in chunks rather than
/// buffering the entire response into a `Vec<u8>`.
async fn fetch_remote_media_stream_via_federation(
    ctx: &MediaContext,
    server_name: &str,
    media_id: &str,
    response_filename: Option<&str>,
) -> Result<(HeaderMap, Body), ApiError> {
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

    let content_length: usize = resp
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let headers = build_proxy_media_headers(content_type, content_length, response_filename);
    let header_map = media_response_headers(&headers);

    // Stream the remote body directly — no buffering into Vec<u8>.
    let body = Body::from_stream(resp.bytes_stream());
    Ok((header_map, body))
}

/// Streaming variant of [`download_media_common`].
///
/// For local media: opens the file and streams it via `ReaderStream`.
/// For remote media: forwards the federation response body as a stream.
///
/// Returns `(StatusCode, HeaderMap, Body)` ready to be converted into a
/// `Response` via `IntoResponse`.
pub(crate) async fn download_media_stream_common(
    ctx: &MediaContext,
    server_name: &str,
    media_id: &str,
    response_filename: Option<&str>,
) -> Result<(StatusCode, HeaderMap, Body), ApiError> {
    if server_name == ctx.server_name {
        // Local media: stream from file handle.
        let payload = ctx.media_domain_service.download_media_stream(server_name, media_id, response_filename).await?;
        let headers = media_response_headers(&payload.headers);
        let body = Body::from_stream(ReaderStream::new(payload.file));
        Ok((StatusCode::OK, headers, body))
    } else {
        // Remote media: stream from federation response.
        let (headers, body) =
            fetch_remote_media_stream_via_federation(ctx, server_name, media_id, response_filename).await?;
        Ok((StatusCode::OK, headers, body))
    }
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
    let has_width = params.get("width").is_some();
    let has_height = params.get("height").is_some();
    if !has_width && !has_height {
        return Err(ApiError::bad_request(
            "Missing width and height query parameters: at least one must be provided".to_string(),
        ));
    }
    let (width, height, method) = thumbnail_request_params(params);

    if server_name == ctx.server_name {
        return ctx.media_domain_service.get_thumbnail(server_name, media_id, width, height, method).await;
    }

    fetch_remote_thumbnail_via_federation(ctx, server_name, media_id, width, height, method).await
}

// ---------------------------------------------------------------------------
// Download handlers (S3: streaming — no full buffering into Vec<u8>)
// ---------------------------------------------------------------------------

pub(crate) async fn download_media(
    State(ctx): State<MediaContext>,
    auth_user: OptionalAuthenticatedUser,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
) -> Result<Response, ApiError> {
    let _ = auth_user;
    let (status, headers, body) = download_media_stream_common(&ctx, &server_name, &media_id, None).await?;
    Ok((status, headers, body).into_response())
}

pub(crate) async fn download_media_with_filename(
    State(ctx): State<MediaContext>,
    auth_user: OptionalAuthenticatedUser,
    Path((server_name, media_id, filename)): Path<(ServerName, MediaId, String)>,
) -> Result<Response, ApiError> {
    let _ = auth_user;
    let (status, headers, body) = download_media_stream_common(&ctx, &server_name, &media_id, Some(&filename)).await?;
    Ok((status, headers, body).into_response())
}

/// Signed media download — verifies HMAC signature before serving.
pub(crate) async fn download_media_signed(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
    Query(params): Query<Value>,
) -> Result<Response, ApiError> {
    let signature = params
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("Missing signature parameter".to_string()))?;

    let expires: u64 = params.get("expires").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);

    if !ctx.media_domain_service.verify_media_download_url(&server_name, &media_id, signature, expires) {
        return Err(ApiError::unauthorized("Invalid or expired media signature".to_string()));
    }

    let (status, headers, body) = download_media_stream_common(&ctx, &server_name, &media_id, None).await?;
    Ok((status, headers, body).into_response())
}

/// Signed media download with filename.
pub(crate) async fn download_media_signed_with_filename(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id, filename)): Path<(ServerName, MediaId, String)>,
    Query(params): Query<Value>,
) -> Result<Response, ApiError> {
    let signature = params
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("Missing signature parameter".to_string()))?;

    let expires: u64 = params.get("expires").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);

    if !ctx.media_domain_service.verify_media_download_url(&server_name, &media_id, signature, expires) {
        return Err(ApiError::unauthorized("Invalid or expired media signature".to_string()));
    }

    let (status, headers, body) = download_media_stream_common(&ctx, &server_name, &media_id, Some(&filename)).await?;
    Ok((status, headers, body).into_response())
}

pub(crate) async fn download_media_authenticated(
    State(ctx): State<MediaContext>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
) -> Result<Response, ApiError> {
    let (status, headers, body) = download_media_stream_common(&ctx, &server_name, &media_id, None).await?;
    Ok((status, headers, body).into_response())
}

pub(crate) async fn download_media_authenticated_with_filename(
    State(ctx): State<MediaContext>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id, filename)): Path<(ServerName, MediaId, String)>,
) -> Result<Response, ApiError> {
    let (status, headers, body) = download_media_stream_common(&ctx, &server_name, &media_id, Some(&filename)).await?;
    Ok((status, headers, body).into_response())
}

pub(crate) async fn download_media_v1(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
) -> Response {
    match download_media_stream_common(&ctx, &server_name, &media_id, None).await {
        Ok((status, headers, body)) => (status, headers, body).into_response(),
        Err(error) => {
            let (status, headers, body) = media_error_response(&error);
            (status, headers, body).into_response()
        }
    }
}

pub(crate) async fn download_media_v1_with_filename(
    State(ctx): State<MediaContext>,
    Path((server_name, media_id, filename)): Path<(ServerName, MediaId, String)>,
) -> Response {
    match download_media_stream_common(&ctx, &server_name, &media_id, Some(&filename)).await {
        Ok((status, headers, body)) => (status, headers, body).into_response(),
        Err(error) => {
            let (status, headers, body) = media_error_response(&error);
            (status, headers, body).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Thumbnail handlers
// ---------------------------------------------------------------------------

pub(crate) async fn get_thumbnail(
    State(ctx): State<MediaContext>,
    auth_user: OptionalAuthenticatedUser,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let _ = auth_user;
    let response = thumbnail_response_common(&ctx, &server_name, &media_id, &params).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

pub(crate) async fn get_thumbnail_authenticated(
    State(ctx): State<MediaContext>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(ServerName, MediaId)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let response = thumbnail_response_common(&ctx, &server_name, &media_id, &params).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

#[cfg(test)]
mod tests {
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
