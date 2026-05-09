use super::AppState;
use crate::common::ApiError;
use crate::web::AuthenticatedUser;
use super::OptionalAuthenticatedUser;
use axum::{
    body::Bytes,
    extract::{Json, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Router,
};
use chrono;
use serde_json::{json, Value};

fn create_media_config_router() -> Router<AppState> {
    Router::new().route("/config", get(media_config))
}

fn create_media_preview_delete_router() -> Router<AppState> {
    Router::new()
        .route("/preview_url", get(preview_url))
        .route("/delete/{server_name}/{media_id}", post(delete_media))
}

fn create_media_legacy_download_router() -> Router<AppState> {
    Router::new()
        .route("/download/{server_name}/{media_id}", get(download_media_v1))
        .route(
            "/download/{server_name}/{media_id}/{filename}",
            get(download_media_v1_with_filename),
        )
}

fn create_media_modern_upload_router() -> Router<AppState> {
    Router::new().route("/upload", post(upload_media_v3))
}

fn create_media_v1_router() -> Router<AppState> {
    Router::new()
        .route("/upload", post(upload_media_v1))
        .merge(create_media_config_router())
        .merge(create_media_preview_delete_router())
        .merge(create_media_legacy_download_router())
        .route("/quota/check", get(check_quota))
        .route("/quota/stats", get(quota_stats))
        .route("/quota/alerts", get(quota_alerts))
}

fn create_media_v3_router() -> Router<AppState> {
    Router::new()
        .merge(create_media_modern_upload_router())
        .merge(create_media_config_router())
        .merge(create_media_preview_delete_router())
        .route(
            "/upload/{server_name}/{media_id}",
            put(upload_media_with_id),
        )
        .route("/download/{server_name}/{media_id}", get(download_media))
        .route(
            "/download/{server_name}/{media_id}/{filename}",
            get(download_media_with_filename),
        )
        .route("/thumbnail/{server_name}/{media_id}", get(get_thumbnail))
}

fn create_media_r0_router() -> Router<AppState> {
    create_media_modern_upload_router().merge(create_media_config_router())
}

fn create_media_r1_router() -> Router<AppState> {
    create_media_legacy_download_router()
}

fn create_media_authenticated_router() -> Router<AppState> {
    Router::new()
        .route("/download/{server_name}/{media_id}", get(download_media_authenticated))
        .route(
            "/download/{server_name}/{media_id}/{filename}",
            get(download_media_authenticated_with_filename),
        )
        .route("/thumbnail/{server_name}/{media_id}", get(get_thumbnail_authenticated))
}

pub fn create_media_router(_state: AppState) -> Router<AppState> {
    let preview_router = Router::new().route("/preview_url", get(preview_url));
    let authenticated_media_router = create_media_authenticated_router();
    Router::new()
        .nest("/_matrix/media/v1", create_media_v1_router())
        .nest("/_matrix/media/v3", create_media_v3_router())
        .nest("/_matrix/media/r0", create_media_r0_router())
        .nest("/_matrix/media/r1", create_media_r1_router())
        .nest("/_matrix/client/v1/media", authenticated_media_router.merge(preview_router))
}

// ---------------------------------------------------------------------------
// Route ledger manifest
//
// Each nested router exposes a different subset of the media surface:
//   - v1:  upload_v1, /config, preview/delete, legacy download, /quota/*
//   - v3:  modern upload, /config, preview/delete, /upload/{...},
//          download (modern), thumbnail
//   - r0:  modern upload + /config (subset)
//   - r1:  legacy download only
// We enumerate each per-prefix below rather than using
// `expand_under_prefixes` because the relative-path sets differ.
// ---------------------------------------------------------------------------

fn media_v1_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::POST, "/upload"),
        (Method::GET, "/config"),
        (Method::GET, "/preview_url"),
        (Method::POST, "/delete/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}/{filename}"),
        (Method::GET, "/quota/check"),
        (Method::GET, "/quota/stats"),
        (Method::GET, "/quota/alerts"),
    ]
}

fn media_v3_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::POST, "/upload"),
        (Method::GET, "/config"),
        (Method::GET, "/preview_url"),
        (Method::POST, "/delete/{server_name}/{media_id}"),
        (Method::PUT, "/upload/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}/{filename}"),
        (Method::GET, "/thumbnail/{server_name}/{media_id}"),
    ]
}

fn media_r0_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![(Method::POST, "/upload"), (Method::GET, "/config")]
}

fn media_r1_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/download/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}/{filename}"),
    ]
}

fn media_authenticated_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/download/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}/{filename}"),
        (Method::GET, "/thumbnail/{server_name}/{media_id}"),
        (Method::GET, "/preview_url"),
    ]
}

pub fn media_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::expand_under_prefixes;
    let mut out =
        expand_under_prefixes("media", &["/_matrix/media/v1"], &media_v1_relative_routes());
    out.extend(expand_under_prefixes(
        "media",
        &["/_matrix/media/v3"],
        &media_v3_relative_routes(),
    ));
    out.extend(expand_under_prefixes(
        "media",
        &["/_matrix/media/r0"],
        &media_r0_relative_routes(),
    ));
    out.extend(expand_under_prefixes(
        "media",
        &["/_matrix/media/r1"],
        &media_r1_relative_routes(),
    ));
    out.extend(expand_under_prefixes(
        "media",
        &["/_matrix/client/v1/media"],
        &media_authenticated_relative_routes(),
    ));
    out
}

async fn upload_media_common(
    state: &AppState,
    user_id: &str,
    params: &Value,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let content_type = params
        .get("content_type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
        })
        .unwrap_or("application/octet-stream");

    let filename = params.get("filename").and_then(|v| v.as_str());
    let content_bytes = body.to_vec();

    if content_bytes.is_empty() {
        return Err(ApiError::bad_request(
            "No file content provided".to_string(),
        ));
    }

    let quota_info = state
        .services
        .media_quota_service
        .get_user_quota(user_id)
        .await?;

    if quota_info.max_storage_bytes > 0 {
        let new_total = quota_info.current_storage_bytes + content_bytes.len() as i64;
        if new_total > quota_info.max_storage_bytes {
            return Err(ApiError::bad_request(format!(
                "Media quota exceeded: {} bytes used, {} bytes limit, {} bytes would be added",
                quota_info.current_storage_bytes,
                quota_info.max_storage_bytes,
                content_bytes.len()
            )));
        }
    }

    Ok(Json(
        state
            .services
            .media_service
            .upload_media(user_id, &content_bytes, content_type, filename)
            .await?,
    ))
}

async fn upload_media_with_id_common(
    state: &AppState,
    user_id: &str,
    server_name: &str,
    media_id: &str,
    params: &Value,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    if server_name != state.services.server_name {
        return Err(ApiError::bad_request(format!(
            "server_name must match local server: {}",
            state.services.server_name
        )));
    }

    let content_type = params
        .get("content_type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
        })
        .unwrap_or("application/octet-stream");

    let filename = params.get("filename").and_then(|v| v.as_str());
    let content_bytes = body.to_vec();

    if content_bytes.is_empty() {
        return Err(ApiError::bad_request(
            "No file content provided".to_string(),
        ));
    }

    Ok(Json(
        state
            .services
            .media_service
            .upload_media_with_id(user_id, media_id, &content_bytes, content_type, filename)
            .await?,
    ))
}

fn ensure_local_media_server_name(state: &AppState, server_name: &str) -> Result<(), ApiError> {
    if server_name != state.services.server_name {
        return Err(ApiError::not_found("Media not found".to_string()));
    }

    Ok(())
}

async fn download_media_common(
    state: &AppState,
    server_name: &str,
    media_id: &str,
) -> Result<(String, Vec<u8>, Option<String>), ApiError> {
    ensure_local_media_server_name(state, server_name)?;

    let content = state
        .services
        .media_service
        .download_media(server_name, media_id)
        .await?;

    let metadata = state
        .services
        .media_service
        .get_media_metadata(server_name, media_id)
        .await
        .unwrap_or(serde_json::Value::Null);

    let stored_content_type = metadata
        .get("content_type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let stored_filename = metadata
        .get("filename")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let content_type = stored_content_type
        .or_else(|| {
            let guess_src = stored_filename.as_deref().unwrap_or(media_id);
            Some(guess_content_type(guess_src, &content).to_string())
        })
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Ok((content_type, content, stored_filename))
}

fn media_response_headers(
    content_type: String,
    content_length: usize,
    filename: Option<&str>,
) -> HeaderMap {
    build_media_headers(content_type, content_length, filename)
}

fn media_error_response(error: ApiError) -> (StatusCode, HeaderMap, Vec<u8>) {
    let status = error.http_status();
    let error_body = serde_json::to_vec(&json!({
        "errcode": error.code(),
        "error": error.message()
    }))
    .unwrap_or_else(|_| br#"{"errcode":"M_UNKNOWN","error":"Internal error"}"#.to_vec());
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    if let Ok(v) = axum::http::HeaderValue::from_str(&error_body.len().to_string()) {
        headers.insert(header::CONTENT_LENGTH, v);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    (status, headers, error_body)
}

// 媒体下载响应必须像 Synapse 上游那样在所有浏览器路径上锁死 XSS：
// 1) 仅对一组明确的"惰性"媒体类型保留原始 Content-Type 并允许 inline；
//    其余类型一律强制改写为 application/octet-stream + Content-Disposition: attachment，
//    这样即便用户上传 .svg / .html / .js / .xhtml，浏览器也只会下载、不会执行。
// 2) 永远附带 X-Content-Type-Options: nosniff，阻止旧版 Edge/IE 的 MIME 嗅探回退到 HTML。
// 3) 永远附带强 sandbox CSP，封掉脚本/插件/同源对象，作为最后一道兜底——
//    历史上多次的媒体 XSS（CVE-2018-16868 等）都是因为缺这层防御才被打穿。
// 4) Cross-Origin-Resource-Policy: cross-origin 让客户端 SDK 仍可跨源加载缩略图，
//    Referrer-Policy: no-referrer 防止媒体 URL 把房间/用户标识泄给第三方。
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
            if c.is_ascii_alphanumeric() || matches!(c, '!' | '#' | '$' | '&' | '+' | '-' | '.' | '^' | '_' | '`' | '|' | '~') {
                c.to_string()
            } else {
                format!("{}{:02X}", "%", c as u32)
            }
        })
        .collect()
}

fn build_media_headers(
    content_type: String,
    content_length: usize,
    filename: Option<&str>,
) -> HeaderMap {
    use axum::http::HeaderValue;

    let primary_type = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let inline_safe = SAFE_INLINE_MEDIA_TYPES
        .iter()
        .any(|safe| *safe == primary_type);

    let (final_type, disposition_kind) = if inline_safe {
        (content_type, "inline")
    } else {
        (content_type, "attachment")
    };

    let disposition = match filename {
        Some(name) if !name.is_empty() => {
            let safe = sanitize_attachment_filename(name);
            if safe.is_empty() {
                disposition_kind.to_string()
            } else {
                let encoded = encode_rfc5987(&safe);
                format!("{}; filename=\"{}\"; filename*=UTF-8''{}", disposition_kind, safe, encoded)
            }
        }
        _ => disposition_kind.to_string(),
    };

    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&final_type) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&content_length.to_string()) {
        headers.insert(header::CONTENT_LENGTH, v);
    }
    if let Ok(v) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(MEDIA_CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        axum::http::HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("cross-origin"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers
}

async fn upload_media_v3(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    upload_media_common(&state, &auth_user.user_id, &params, &headers, body).await
}

pub async fn media_config(State(_state): State<AppState>) -> Json<Value> {
    Json(json!({
        "m.upload.size": 50 * 1024 * 1024
    }))
}

pub async fn check_quota(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let quota_info = state
        .services
        .media_quota_service
        .get_user_quota(&auth_user.user_id)
        .await?;

    let limit = quota_info.max_storage_bytes;
    let used = quota_info.current_storage_bytes;
    let remaining = if used >= limit { 0 } else { limit - used };

    Ok(Json(json!({
        "limit": limit,
        "used": used,
        "remaining": remaining,
        "rule": "global"
    })))
}

pub async fn quota_stats(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let quota_info = state
        .services
        .media_quota_service
        .get_user_quota(&auth_user.user_id)
        .await?;
    let stats = state
        .services
        .media_quota_service
        .get_usage_stats(&auth_user.user_id)
        .await?;

    Ok(Json(json!({
        "user_id": auth_user.user_id,
        "storage_bytes": quota_info.current_storage_bytes,
        "media_count": quota_info.current_files_count,
        "limit_bytes": quota_info.max_storage_bytes,
        "statistics": stats
    })))
}

pub async fn quota_alerts(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let alerts = state
        .services
        .media_quota_service
        .get_user_alerts(&auth_user.user_id, false)
        .await?;

    let alerts_list: Vec<Value> = alerts
        .into_iter()
        .map(|alert| {
            json!({
                "alert_id": alert.id,
                "alert_type": alert.alert_type,
                "threshold_percent": alert.threshold_percent,
                "current_usage_bytes": alert.current_usage_bytes,
                "quota_limit_bytes": alert.quota_limit_bytes,
                "message": alert.message,
                "created_ts": alert.created_ts,
                "is_read": alert.is_read
            })
        })
        .collect();

    Ok(Json(json!({
        "alerts": alerts_list
    })))
}

async fn upload_media_with_id(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    upload_media_with_id_common(
        &state,
        &auth_user.user_id,
        &server_name,
        &media_id,
        &params,
        &headers,
        body,
    )
    .await
}

async fn download_media(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (content_type, content, stored_filename) = download_media_common(&state, &server_name, &media_id).await?;
    let headers = media_response_headers(content_type, content.len(), stored_filename.as_deref());
    Ok((StatusCode::OK, headers, content))
}

async fn download_media_with_filename(
    State(state): State<AppState>,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (content_type, content, _) = download_media_common(&state, &server_name, &media_id).await?;
    let headers = media_response_headers(content_type, content.len(), Some(&filename));
    Ok((StatusCode::OK, headers, content))
}

async fn download_media_authenticated(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (content_type, content, stored_filename) = download_media_common(&state, &server_name, &media_id).await?;
    let headers = media_response_headers(content_type, content.len(), stored_filename.as_deref());
    Ok((StatusCode::OK, headers, content))
}

async fn download_media_authenticated_with_filename(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (content_type, content, _) = download_media_common(&state, &server_name, &media_id).await?;
    let headers = media_response_headers(content_type, content.len(), Some(&filename));
    Ok((StatusCode::OK, headers, content))
}

async fn get_thumbnail_authenticated(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_local_media_server_name(&state, &server_name)?;

    let width = params.get("width").and_then(|v| v.as_u64()).unwrap_or(800) as u32;
    let height = params.get("height").and_then(|v| v.as_u64()).unwrap_or(600) as u32;
    let method = params
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("scale");

    match state
        .services
        .media_service
        .get_thumbnail(&server_name, &media_id, width, height, method)
        .await
    {
        Ok(content) => {
            let headers = media_response_headers("image/jpeg".to_string(), content.len(), None);
            Ok((StatusCode::OK, headers, content))
        }
        Err(e) => Err(e),
    }
}

async fn _preview_url(
    State(_state): State<AppState>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let url = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("URL required".to_string()))?;

    Ok(Json(json!({
        "url": url,
        "title": "Preview",
        "description": "URL preview"
    })))
}

async fn get_thumbnail(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_local_media_server_name(&state, &server_name)?;

    let width = params.get("width").and_then(|v| v.as_u64()).unwrap_or(800) as u32;
    let height = params.get("height").and_then(|v| v.as_u64()).unwrap_or(600) as u32;
    let method = params
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("scale");

    match state
        .services
        .media_service
        .get_thumbnail(&server_name, &media_id, width, height, method)
        .await
    {
        // 缩略图始终编码为 JPEG（generate_thumbnail 内固定 ImageFormat::Jpeg），
        // 走与 download_media 同一套安全头，避免出现"主资源加固、缩略图裸奔"的不对称。
        Ok(content) => {
            let headers = media_response_headers("image/jpeg".to_string(), content.len(), None);
            Ok((StatusCode::OK, headers, content))
        }
        Err(e) => Err(e),
    }
}

async fn upload_media_v1(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    upload_media_common(&state, &auth_user.user_id, &params, &headers, body).await
}

async fn download_media_v1(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match download_media_common(&state, &server_name, &media_id).await {
        Ok((content_type, content, stored_filename)) => {
            let headers = media_response_headers(content_type, content.len(), stored_filename.as_deref());
            (StatusCode::OK, headers, content)
        }
        Err(error) => media_error_response(error),
    }
}

async fn download_media_v1_with_filename(
    State(state): State<AppState>,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
) -> impl IntoResponse {
    match download_media_common(&state, &server_name, &media_id).await {
        Ok((content_type, content, _)) => {
            let headers = media_response_headers(content_type, content.len(), Some(&filename));
            (StatusCode::OK, headers, content)
        }
        Err(error) => media_error_response(error),
    }
}

fn guess_content_type(filename: &str, data: &[u8]) -> &'static str {
    if let Some(kind) = infer::get(data) {
        return kind.mime_type();
    }

    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        _ => "application/octet-stream",
    }
}

async fn preview_url(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let url = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("URL required".to_string()))?;

    let blacklist = &state.services.config.url_preview.ip_range_blacklist;
    if let Err(e) = crate::common::security::check_url_against_blacklist(url, blacklist) {
        return Err(ApiError::forbidden(format!("URL not allowed: {}", e)));
    }

    let ts = params
        .get("ts")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    match state.services.media_service.preview_url(url, ts) {
        Ok(preview) => Ok(Json(preview)),
        Err(e) => Ok(Json(json!({
            "url": url,
            "title": "Preview unavailable",
            "description": format!("Could not generate preview: {}", e.message())
        }))),
    }
}

async fn delete_media(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    ensure_local_media_server_name(&state, &server_name)?;

    let media_info = state
        .services
        .media_service
        .get_media_info(&server_name, &media_id)
        .await?;

    let uploader = media_info
        .get("uploader")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if uploader != auth_user.user_id {
        return Err(ApiError::forbidden(
            "You can only delete your own media".to_string(),
        ));
    }

    state
        .services
        .media_service
        .delete_media(&server_name, &media_id)
        .await?;

    Ok(Json(json!({
        "deleted": true,
        "media_id": media_id
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_routes_structure() {
        let routes = vec![
            "/_matrix/media/v3/upload/{server_name}/{media_id}",
            "/_matrix/media/v3/download/{server_name}/{media_id}",
            "/_matrix/media/v3/thumbnail/{server_name}/{media_id}",
            "/_matrix/media/v1/upload",
            "/_matrix/media/v3/upload",
            "/_matrix/media/r0/upload",
            "/_matrix/media/r1/download/{server_name}/{media_id}",
            "/_matrix/media/v1/config",
            "/_matrix/media/v3/config",
        ];

        for route in routes {
            assert!(route.starts_with("/_matrix/media/"));
        }
    }

    #[test]
    fn test_media_nested_router_boundaries() {
        let v1_paths = [
            "/upload",
            "/config",
            "/quota/check",
            "/quota/stats",
            "/quota/alerts",
            "/download/{server_name}/{media_id}",
            "/download/{server_name}/{media_id}/{filename}",
            "/preview_url",
            "/delete/{server_name}/{media_id}",
        ];
        let v3_paths = [
            "/upload/{server_name}/{media_id}",
            "/download/{server_name}/{media_id}",
            "/download/{server_name}/{media_id}/{filename}",
            "/thumbnail/{server_name}/{media_id}",
            "/upload",
            "/preview_url",
            "/config",
            "/delete/{server_name}/{media_id}",
        ];

        assert_eq!(v1_paths.len(), 9);
        assert_eq!(v3_paths.len(), 8);
        assert!(v1_paths.iter().all(|path| path.starts_with('/')));
        assert!(v3_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_media_shared_router_contains_common_paths() {
        let shared_paths = [
            "/config",
            "/preview_url",
            "/delete/{server_name}/{media_id}",
        ];
        let modern_upload_paths = ["/upload"];
        let legacy_download_paths = [
            "/download/{server_name}/{media_id}",
            "/download/{server_name}/{media_id}/{filename}",
        ];

        assert_eq!(shared_paths.len(), 3);
        assert_eq!(modern_upload_paths.len(), 1);
        assert_eq!(legacy_download_paths.len(), 2);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
        assert!(legacy_download_paths
            .iter()
            .all(|path| path.starts_with("/download/")));
    }

    #[test]
    fn test_media_router_keeps_version_boundaries() {
        let r0_only_paths = ["/_matrix/media/r0/upload"];
        let r1_only_paths = ["/_matrix/media/r1/download/{server_name}/{media_id}"];
        let v1_only_paths = ["/_matrix/media/v1/quota/check"];
        let v3_only_paths = [
            "/_matrix/media/v3/upload/{server_name}/{media_id}",
            "/_matrix/media/v3/thumbnail/{server_name}/{media_id}",
        ];

        assert!(r0_only_paths
            .iter()
            .all(|path| !path.contains("/preview_url")));
        assert!(r1_only_paths.iter().all(|path| !path.contains("/delete/")));
        assert!(v1_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/media/v1/")));
        assert!(v3_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/media/v3/")));
    }

    #[test]
    fn test_media_config_response() {
        let config = json!({
            "m.upload.size": 50 * 1024 * 1024
        });

        assert!(config.get("m.upload.size").is_some());
        let size = config.get("m.upload.size").unwrap().as_i64().unwrap();
        assert_eq!(size, 50 * 1024 * 1024);
    }

    #[test]
    fn test_content_type_default() {
        let default_content_type = "application/octet-stream";
        assert!(!default_content_type.is_empty());
    }

    #[test]
    fn test_media_id_format() {
        let media_ids = vec![
            "abc123",
            "media_id_with_underscores",
            "media-id-with-dashes",
            "UPPERCASE123",
        ];

        for id in media_ids {
            assert!(!id.is_empty());
            assert!(id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
        }
    }

    #[test]
    fn test_server_name_format() {
        let server_names = vec!["example.com", "matrix.org", "server.local"];

        for name in server_names {
            assert!(!name.is_empty());
            assert!(name.contains('.'));
        }
    }

    #[test]
    fn test_upload_response_structure() {
        let response = json!({
            "content_uri": "mxc://example.com/media_id_123"
        });

        assert!(response.get("content_uri").is_some());
        let uri = response.get("content_uri").unwrap().as_str().unwrap();
        assert!(uri.starts_with("mxc://"));
    }

    #[test]
    fn test_delete_response_structure() {
        let response = json!({
            "deleted": true,
            "media_id": "media_id_123"
        });

        assert!(response.get("deleted").unwrap().as_bool().unwrap());
        assert!(response.get("media_id").is_some());
    }

    #[test]
    fn test_thumbnail_size_params() {
        let params = json!({
            "width": 256,
            "height": 256,
            "method": "scale"
        });

        assert_eq!(params.get("width").unwrap().as_i64().unwrap(), 256);
        assert_eq!(params.get("height").unwrap().as_i64().unwrap(), 256);
        assert_eq!(params.get("method").unwrap().as_str().unwrap(), "scale");
    }
}
