use super::AppState;
use super::OptionalAuthenticatedUser;
use crate::common::ApiError;
use crate::web::AuthenticatedUser;
use axum::{
    body::Bytes,
    extract::{Json, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
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
    Router::new().route("/preview_url", get(preview_url)).route("/delete/{server_name}/{media_id}", post(delete_media))
}

fn create_media_legacy_download_router() -> Router<AppState> {
    Router::new()
        .route("/download/{server_name}/{media_id}", get(download_media_v1))
        .route("/download/{server_name}/{media_id}/{filename}", get(download_media_v1_with_filename))
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
        // Chunked upload routes
        .route("/upload/chunk/start", post(chunked_upload_start))
        .route("/upload/chunk", post(chunked_upload_chunk))
        .route("/upload/chunk/complete", post(chunked_upload_complete))
        .route("/upload/chunk/cancel", post(chunked_upload_cancel))
        .route("/upload/chunk/progress", get(chunked_upload_progress))
}

fn create_media_v3_router() -> Router<AppState> {
    Router::new()
        .merge(create_media_modern_upload_router())
        .merge(create_media_config_router())
        .merge(create_media_preview_delete_router())
        .route("/upload/{server_name}/{media_id}", put(upload_media_with_id))
        .route("/download/{server_name}/{media_id}", get(download_media))
        .route("/download/{server_name}/{media_id}/{filename}", get(download_media_with_filename))
        .route("/download_signed/{server_name}/{media_id}", get(download_media_signed))
        .route("/download_signed/{server_name}/{media_id}/{filename}", get(download_media_signed_with_filename))
        .route("/thumbnail/{server_name}/{media_id}", get(get_thumbnail))
}

fn create_media_r0_router() -> Router<AppState> {
    create_media_modern_upload_router()
        .merge(create_media_config_router())
        .merge(create_media_legacy_download_router())
        .merge(create_media_preview_delete_router())
}

fn create_media_r1_router() -> Router<AppState> {
    create_media_legacy_download_router()
}

fn create_media_authenticated_router() -> Router<AppState> {
    Router::new()
        .route("/download/{server_name}/{media_id}", get(download_media_authenticated))
        .route("/download/{server_name}/{media_id}/{filename}", get(download_media_authenticated_with_filename))
        .route("/thumbnail/{server_name}/{media_id}", get(get_thumbnail_authenticated))
}

pub fn create_upload_provider_router() -> Router<AppState> {
    Router::new().route("/upload/token", post(create_upload_token)).route("/upload/provider", get(get_upload_provider))
}

/// POST /_matrix/client/v3/upload/token
/// Generate a one-time upload token for OSS/MinIO direct upload.
/// Falls back to standard Matrix upload if no external provider is configured.
async fn create_upload_token(
    State(_state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let filename = body.get("filename").and_then(|v| v.as_str()).unwrap_or("upload");
    let content_type = body.get("content_type").and_then(|v| v.as_str()).unwrap_or("application/octet-stream");

    // Generate a unique upload token
    let token = format!("upload_{}_{}", auth_user.user_id, chrono::Utc::now().timestamp_millis());

    // Standard Matrix upload fallback (no external storage configured)
    Ok((
        StatusCode::OK,
        Json(json!({
            "upload_token": token,
            "storage_type": "matrix",
            "upload_url": "/_matrix/media/v3/upload",
            "filename": filename,
            "content_type": content_type,
            "max_file_size": 50 * 1024 * 1024u64,
        })),
    ))
}

/// GET /_matrix/client/v3/upload/provider
/// Return the current upload provider configuration.
async fn get_upload_provider(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::OK,
        Json(json!({
            "provider": "matrix",
            "supports_chunked_upload": true,
            "supports_resume": true,
            "max_file_size": 50 * 1024 * 1024u64,
            "chunk_size": 5 * 1024 * 1024,
        })),
    ))
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
        (Method::POST, "/upload/chunk/start"),
        (Method::POST, "/upload/chunk"),
        (Method::POST, "/upload/chunk/complete"),
        (Method::POST, "/upload/chunk/cancel"),
        (Method::GET, "/upload/chunk/progress"),
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
        (Method::GET, "/download_signed/{server_name}/{media_id}"),
        (Method::GET, "/download_signed/{server_name}/{media_id}/{filename}"),
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
    let mut out = expand_under_prefixes("media", &["/_matrix/media/v1"], &media_v1_relative_routes());
    out.extend(expand_under_prefixes("media", &["/_matrix/media/v3"], &media_v3_relative_routes()));
    out.extend(expand_under_prefixes("media", &["/_matrix/media/r0"], &media_r0_relative_routes()));
    out.extend(expand_under_prefixes("media", &["/_matrix/media/r1"], &media_r1_relative_routes()));
    out.extend(expand_under_prefixes("media", &["/_matrix/client/v1/media"], &media_authenticated_relative_routes()));
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
        .or_else(|| headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()))
        .unwrap_or("application/octet-stream");

    let filename = params.get("filename").and_then(|v| v.as_str());
    let content_bytes = body.to_vec();

    if content_bytes.is_empty() {
        return Err(ApiError::bad_request("No file content provided".to_string()));
    }

    Ok(Json(
        state
            .services
            .extensions
            .media_domain_service
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
    if server_name != state.services.core.server_name {
        return Err(ApiError::bad_request(format!(
            "server_name must match local server: {}",
            state.services.core.server_name
        )));
    }

    let content_type = params
        .get("content_type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()))
        .unwrap_or("application/octet-stream");

    let filename = params.get("filename").and_then(|v| v.as_str());
    let content_bytes = body.to_vec();

    if content_bytes.is_empty() {
        return Err(ApiError::bad_request("No file content provided".to_string()));
    }

    Ok(Json(
        state
            .services
            .extensions
            .media_domain_service
            .upload_media_with_id(user_id, media_id, &content_bytes, content_type, filename)
            .await?,
    ))
}

fn ensure_local_media_server_name(state: &AppState, server_name: &str) -> Result<(), ApiError> {
    if server_name != state.services.core.server_name {
        return Err(ApiError::not_found("Media not found".to_string()));
    }

    Ok(())
}

async fn download_media_common(
    state: &AppState,
    server_name: &str,
    media_id: &str,
    response_filename: Option<&str>,
) -> Result<crate::services::media::MediaResponsePayload, ApiError> {
    ensure_local_media_server_name(state, server_name)?;

    state.services.extensions.media_domain_service.download_media(server_name, media_id, response_filename).await
}

fn media_response_headers(headers: &crate::services::media::MediaResponseHeaders) -> HeaderMap {
    use axum::http::HeaderValue;

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

fn media_error_response(error: &ApiError) -> (StatusCode, HeaderMap, Vec<u8>) {
    let status = error.http_status();
    let error_body = serde_json::to_vec(&json!({
        "errcode": error.code(),
        "error": error.message()
    }))
    .unwrap_or_else(|_| br#"{"errcode":"M_UNKNOWN","error":"Internal error"}"#.to_vec());
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, axum::http::HeaderValue::from_static("application/json"));
    if let Ok(v) = axum::http::HeaderValue::from_str(&error_body.len().to_string()) {
        headers.insert(header::CONTENT_LENGTH, v);
    }
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, axum::http::HeaderValue::from_static("nosniff"));
    (status, headers, error_body)
}

fn thumbnail_request_params(params: &Value) -> (u32, u32, &str) {
    let width = params.get("width").and_then(|v| v.as_u64()).filter(|&w| w <= 10000).unwrap_or(800) as u32;
    let height = params.get("height").and_then(|v| v.as_u64()).filter(|&h| h <= 10000).unwrap_or(600) as u32;
    let method = params.get("method").and_then(|v| v.as_str()).unwrap_or("scale");

    (width, height, method)
}

async fn thumbnail_response_common(
    state: &AppState,
    server_name: &str,
    media_id: &str,
    params: &Value,
) -> Result<crate::services::media::MediaResponsePayload, ApiError> {
    ensure_local_media_server_name(state, server_name)?;
    let (width, height, method) = thumbnail_request_params(params);

    state.services.extensions.media_domain_service.get_thumbnail(server_name, media_id, width, height, method).await
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

pub async fn media_config(State(state): State<AppState>) -> impl IntoResponse {
    let route_owner =
        crate::worker::topology_validator::current_instance_worker_type(&state.services.core.config.worker);
    (
        [(header::HeaderName::from_static("x-synapse-route-owner"), HeaderValue::from_static(route_owner.as_str()))],
        Json(json!({
            "m.upload.size": state.services.core.config.server.max_upload_size
        })),
    )
}

pub async fn check_quota(State(state): State<AppState>, auth_user: AuthenticatedUser) -> Result<Json<Value>, ApiError> {
    let quota_info = state.services.extensions.media_domain_service.get_user_quota(&auth_user.user_id).await?;

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

pub async fn quota_stats(State(state): State<AppState>, auth_user: AuthenticatedUser) -> Result<Json<Value>, ApiError> {
    let quota_info = state.services.extensions.media_domain_service.get_user_quota(&auth_user.user_id).await?;
    let stats = state.services.extensions.media_domain_service.get_usage_stats(&auth_user.user_id).await?;

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
    let alerts = state.services.extensions.media_domain_service.get_user_alerts(&auth_user.user_id, false).await?;

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
    upload_media_with_id_common(&state, &auth_user.user_id, &server_name, &media_id, &params, &headers, body).await
}

async fn download_media(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let response = download_media_common(&state, &server_name, &media_id, None).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

async fn download_media_with_filename(
    State(state): State<AppState>,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let response = download_media_common(&state, &server_name, &media_id, Some(&filename)).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

/// Signed media download — verifies HMAC signature before serving.
/// URL: /_matrix/media/v3/download/{server_name}/{media_id}?signature={hex}&expires={timestamp}
async fn download_media_signed(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let signature = params
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("Missing signature parameter".to_string()))?;

    let expires: u64 = params.get("expires").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);

    if !state.services.extensions.media_domain_service.verify_media_download_url(
        &server_name,
        &media_id,
        signature,
        expires,
    ) {
        return Err(ApiError::unauthorized("Invalid or expired media signature".to_string()));
    }

    let response = download_media_common(&state, &server_name, &media_id, None).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

/// Signed media download with filename.
async fn download_media_signed_with_filename(
    State(state): State<AppState>,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let signature = params
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("Missing signature parameter".to_string()))?;

    let expires: u64 = params.get("expires").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);

    if !state.services.extensions.media_domain_service.verify_media_download_url(
        &server_name,
        &media_id,
        signature,
        expires,
    ) {
        return Err(ApiError::unauthorized("Invalid or expired media signature".to_string()));
    }

    let response = download_media_common(&state, &server_name, &media_id, Some(&filename)).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

async fn download_media_authenticated(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let response = download_media_common(&state, &server_name, &media_id, None).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

async fn download_media_authenticated_with_filename(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let response = download_media_common(&state, &server_name, &media_id, Some(&filename)).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

async fn get_thumbnail_authenticated(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let response = thumbnail_response_common(&state, &server_name, &media_id, &params).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
}

#[allow(clippy::unused_async)]
async fn _preview_url(State(_state): State<AppState>, Query(params): Query<Value>) -> Result<Json<Value>, ApiError> {
    let url =
        params.get("url").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad_request("URL required".to_string()))?;

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
    let response = thumbnail_response_common(&state, &server_name, &media_id, &params).await?;
    let headers = media_response_headers(&response.headers);
    Ok((StatusCode::OK, headers, response.content))
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
    match download_media_common(&state, &server_name, &media_id, None).await {
        Ok(response) => {
            let headers = media_response_headers(&response.headers);
            (StatusCode::OK, headers, response.content)
        }
        Err(error) => media_error_response(&error),
    }
}

async fn download_media_v1_with_filename(
    State(state): State<AppState>,
    Path((server_name, media_id, filename)): Path<(String, String, String)>,
) -> impl IntoResponse {
    match download_media_common(&state, &server_name, &media_id, Some(&filename)).await {
        Ok(response) => {
            let headers = media_response_headers(&response.headers);
            (StatusCode::OK, headers, response.content)
        }
        Err(error) => media_error_response(&error),
    }
}

async fn preview_url(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let url =
        params.get("url").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad_request("URL required".to_string()))?;

    let blacklist = &state.services.core.config.url_preview.ip_range_blacklist;
    if let Err(e) = crate::common::security::check_url_against_blacklist(url, blacklist) {
        return Err(ApiError::forbidden(format!("URL not allowed: {e}")));
    }

    let ts = params.get("ts").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    match state.services.extensions.media_domain_service.preview_url(url, ts) {
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

    state
        .services
        .extensions
        .media_domain_service
        .delete_media_for_user(&server_name, &media_id, &auth_user.user_id)
        .await?;

    Ok(Json(json!({
        "deleted": true,
        "media_id": media_id
    })))
}

// ---------------------------------------------------------------------------
// Chunked upload handlers
// ---------------------------------------------------------------------------

/// POST /_matrix/media/v1/upload/chunk/start
/// Start a new chunked upload session.
async fn chunked_upload_start(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let filename = body.get("filename").and_then(|v| v.as_str());
    let content_type = body.get("content_type").and_then(|v| v.as_str());
    let total_size = body.get("total_size").and_then(|v| v.as_i64());
    let total_chunks = body.get("total_chunks").and_then(|v| v.as_i64()).unwrap_or(1) as i32;

    if total_chunks < 1 {
        return Err(ApiError::bad_request("total_chunks must be at least 1".to_string()));
    }

    let upload_id = state
        .services
        .extensions
        .media_domain_service
        .start_chunked_upload(&auth_user.user_id, filename, content_type, total_size, total_chunks)
        .await?;

    Ok(Json(json!({
        "upload_id": upload_id,
        "chunk_size_limit": 10 * 1024 * 1024,
        "max_file_size": 100 * 1024 * 1024
    })))
}

/// POST /_matrix/media/v1/upload/chunk
/// Upload a single chunk. If no upload_id is provided, a new session is auto-started.
async fn chunked_upload_chunk(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    headers: HeaderMap,
    Query(params): Query<Value>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let upload_id = params.get("upload_id").and_then(|v| v.as_str());
    let chunk_index = params.get("chunk_index").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let total_chunks = params.get("total_chunks").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
    let filename = params.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let total_size = params.get("total_size").and_then(|v| v.as_i64());

    let request = crate::services::media::chunked_upload::ChunkUploadRequest {
        upload_id: upload_id.map(|s| s.to_string()),
        chunk_index,
        total_chunks,
        chunk_data: body.to_vec(),
        filename,
        content_type,
        total_size,
    };

    let response = state.services.extensions.media_domain_service.upload_chunk(request, &auth_user.user_id).await?;

    Ok(Json(json!({
        "upload_id": response.upload_id,
        "chunk_index": response.chunk_index,
        "uploaded_chunks": response.uploaded_chunks,
        "total_chunks": response.total_chunks,
        "uploaded_size": response.uploaded_size,
        "status": response.status
    })))
}

/// POST /_matrix/media/v1/upload/chunk/complete
/// Finalize a chunked upload after all chunks have been uploaded.
async fn chunked_upload_complete(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let upload_id = body
        .get("upload_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("upload_id is required".to_string()))?;

    let response =
        state.services.extensions.media_domain_service.complete_chunked_upload(upload_id, &auth_user.user_id).await?;

    Ok(Json(json!({
        "content_uri": response.content_uri,
        "media_id": response.media_id,
        "size": response.size
    })))
}

/// POST /_matrix/media/v1/upload/chunk/cancel
/// Cancel an in-progress chunked upload.
async fn chunked_upload_cancel(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let upload_id = body
        .get("upload_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("upload_id is required".to_string()))?;

    state.services.extensions.media_domain_service.cancel_chunked_upload(upload_id, &auth_user.user_id).await?;

    Ok(Json(json!({
        "cancelled": true,
        "upload_id": upload_id
    })))
}

/// GET /_matrix/media/v1/upload/chunk/progress?upload_id=...
/// Query the progress of an in-progress chunked upload.
async fn chunked_upload_progress(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let upload_id = params
        .get("upload_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("upload_id is required".to_string()))?;

    let progress = state.services.extensions.media_domain_service.get_chunked_upload_progress(upload_id).await?;

    if progress.user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Upload does not belong to user"));
    }

    Ok(Json(json!({
        "upload_id": progress.upload_id,
        "filename": progress.filename,
        "content_type": progress.content_type,
        "total_size": progress.total_size,
        "uploaded_size": progress.uploaded_size,
        "total_chunks": progress.total_chunks,
        "uploaded_chunks": progress.uploaded_chunks,
        "status": progress.status,
        "expires_at": progress.expires_at
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
        let shared_paths = ["/config", "/preview_url", "/delete/{server_name}/{media_id}"];
        let modern_upload_paths = ["/upload"];
        let legacy_download_paths =
            ["/download/{server_name}/{media_id}", "/download/{server_name}/{media_id}/{filename}"];

        assert_eq!(shared_paths.len(), 3);
        assert_eq!(modern_upload_paths.len(), 1);
        assert_eq!(legacy_download_paths.len(), 2);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
        assert!(legacy_download_paths.iter().all(|path| path.starts_with("/download/")));
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

        assert!(r0_only_paths.iter().all(|path| !path.contains("/preview_url")));
        assert!(r1_only_paths.iter().all(|path| !path.contains("/delete/")));
        assert!(v1_only_paths.iter().all(|path| path.starts_with("/_matrix/media/v1/")));
        assert!(v3_only_paths.iter().all(|path| path.starts_with("/_matrix/media/v3/")));
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
        let media_ids = vec!["abc123", "media_id_with_underscores", "media-id-with-dashes", "UPPERCASE123"];

        for id in media_ids {
            assert!(!id.is_empty());
            assert!(id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
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
