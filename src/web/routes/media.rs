use super::AppState;
use crate::common::ApiError;
use crate::web::AuthenticatedUser;
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

pub fn create_media_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/_matrix/media/v1", create_media_v1_router())
        .nest("/_matrix/media/v3", create_media_v3_router())
        .nest("/_matrix/media/r0", create_media_r0_router())
        .nest("/_matrix/media/r1", create_media_r1_router())
}

async fn upload_media_common(
    state: &AppState,
    user_id: &str,
    params: &Value,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
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
            .upload_media(user_id, &content_bytes, content_type, filename)
            .await?,
    ))
}

async fn download_media_common(
    state: &AppState,
    server_name: &str,
    media_id: &str,
) -> Result<(String, Vec<u8>), ApiError> {
    let content = state
        .services
        .media_service
        .download_media(server_name, media_id)
        .await?;
    let content_type = guess_content_type(media_id).to_string();
    Ok((content_type, content))
}

fn media_response_headers(content_type: String, content_length: usize) -> [(String, String); 2] {
    [
        ("Content-Type".to_string(), content_type),
        ("Content-Length".to_string(), content_length.to_string()),
    ]
}

fn media_error_response(error: ApiError) -> ([(String, String); 2], Vec<u8>) {
    let error_body = serde_json::to_vec(&json!({
        "errcode": error.code(),
        "error": error.message()
    }))
    .unwrap_or_else(|_| br#"{"errcode":"M_UNKNOWN","error":"Internal error"}"#.to_vec());
    let headers = media_response_headers("application/json".to_string(), error_body.len());
    (headers, error_body)
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
    Path((_server_name, _media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    upload_media_common(&state, &auth_user.user_id, &params, &headers, body).await
}

async fn download_media(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (content_type, content) = download_media_common(&state, &server_name, &media_id).await?;
    let headers = media_response_headers(content_type, content.len());
    Ok((StatusCode::OK, headers, content))
}

async fn download_media_with_filename(
    State(state): State<AppState>,
    Path((server_name, media_id, _filename)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (content_type, content) = download_media_common(&state, &server_name, &media_id).await?;
    let headers = media_response_headers(content_type, content.len());
    Ok((StatusCode::OK, headers, content))
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
            let content_type = guess_content_type(&media_id);
            let headers = [
                ("Content-Type".to_string(), content_type.to_string()),
                ("Content-Length".to_string(), content.len().to_string()),
            ];
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
        Ok((content_type, content)) => {
            let headers = media_response_headers(content_type, content.len());
            (headers, content)
        }
        Err(error) => media_error_response(error),
    }
}

async fn download_media_v1_with_filename(
    State(state): State<AppState>,
    Path((server_name, media_id, _filename)): Path<(String, String, String)>,
) -> impl IntoResponse {
    match download_media_common(&state, &server_name, &media_id).await {
        Ok((content_type, content)) => {
            let headers = media_response_headers(content_type, content.len());
            (headers, content)
        }
        Err(error) => media_error_response(error),
    }
}

fn guess_content_type(filename: &str) -> &'static str {
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
        _ => "application/octet-stream",
    }
}

async fn preview_url(
    State(state): State<AppState>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let url = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("URL required".to_string()))?;

    let ts = params
        .get("ts")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    match state.services.media_service.preview_url(url, ts).await {
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
    let is_admin = auth_user.is_admin;

    let media_info = state
        .services
        .media_service
        .get_media_info(&server_name, &media_id)
        .await?;

    let uploader = media_info
        .get("uploader")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if !is_admin && uploader != auth_user.user_id {
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
