use super::AppState;
use crate::common::ApiError;
use crate::web::AuthenticatedUser;
use axum::{
    body::Bytes,
    extract::{Json, Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

pub fn create_media_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/media/v3/upload/{server_name}/{media_id}",
            post(upload_media),
        )
        .route(
            "/_matrix/media/v3/download/{server_name}/{media_id}",
            get(download_media),
        )
        .route(
            "/_matrix/media/v3/thumbnail/{server_name}/{media_id}",
            get(get_thumbnail),
        )
        .route("/_matrix/media/v1/upload", post(upload_media_v1))
        .route("/_matrix/media/v3/upload", post(upload_media_v3))
        .route("/_matrix/media/v1/config", get(media_config))
        .route(
            "/_matrix/media/v1/download/{server_name}/{media_id}",
            get(download_media_v1),
        )
        .route(
            "/_matrix/media/r1/download/{server_name}/{media_id}",
            get(download_media_v1),
        )
        .route("/_matrix/media/v3/preview_url", get(preview_url))
        .route("/_matrix/media/v1/preview_url", get(preview_url))
        .route("/_matrix/media/v3/config", get(media_config))
        .route(
            "/_matrix/media/v1/delete/{server_name}/{media_id}",
            post(delete_media),
        )
        .route(
            "/_matrix/media/v3/delete/{server_name}/{media_id}",
            post(delete_media),
        )
}

async fn upload_media_v3(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
    headers: axum::http::HeaderMap,
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

    let media_service = state.services.media_service.clone();
    Ok(Json(
        media_service
            .upload_media(&auth_user.user_id, &content_bytes, content_type, filename)
            .await?,
    ))
}

async fn media_config(State(_state): State<AppState>) -> Json<Value> {
    Json(json!({
        "m.upload.size": 50 * 1024 * 1024
    }))
}

async fn upload_media(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
    headers: axum::http::HeaderMap,
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
            .upload_media(&auth_user.user_id, &content_bytes, content_type, filename)
            .await?,
    ))
}

async fn download_media(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    match state
        .services
        .media_service
        .download_media(&server_name, &media_id)
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
    headers: axum::http::HeaderMap,
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
            .upload_media(&auth_user.user_id, &content_bytes, content_type, filename)
            .await?,
    ))
}

async fn download_media_v1(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state
        .services
        .media_service
        .download_media(&server_name, &media_id)
        .await
    {
        Ok(content) => {
            let content_type = guess_content_type(&media_id);
            let headers = [
                ("Content-Type", content_type.to_string()),
                ("Content-Length", content.len().to_string()),
            ];
            (headers, content)
        }
        Err(e) => {
            let error_body = serde_json::to_vec(&json!({
                "errcode": e.code(),
                "error": e.message()
            }))
            .unwrap_or_else(|_| br#"{"errcode":"M_UNKNOWN","error":"Internal error"}"#.to_vec());
            let headers = [
                ("Content-Type", "application/json".to_string()),
                ("Content-Length", error_body.len().to_string()),
            ];
            (headers, error_body)
        }
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
