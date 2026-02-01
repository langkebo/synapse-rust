use super::AppState;
use crate::common::ApiError;
use crate::services::MediaService;
use crate::web::AuthenticatedUser;
use axum::{
    extract::{Json, Path, Query, State},
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
}

async fn upload_media_v3(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let content = body
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("No file provided".to_string()))?;
    let content_type = body
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream");
    let filename = body.get("filename").and_then(|v| v.as_str());

    let content_bytes: Vec<u8> = content
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u8)
        .collect();

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
    State(_state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let content = body
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("No file provided".to_string()))?;
    let content_type = body
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream");
    let filename = body.get("filename").and_then(|v| v.as_str());

    let content_bytes: Vec<u8> = content
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u8)
        .collect();
    let media_service = MediaService::new("media");
    Ok(Json(
        media_service
            .upload_media(&auth_user.user_id, &content_bytes, content_type, filename)
            .await?,
    ))
}

async fn download_media(
    State(_state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let media_service = MediaService::new("media");

    match media_service.download_media(&server_name, &media_id).await {
        Ok(content) => {
            let content_type = guess_content_type(&media_id);
            let headers = [
                ("Content-Type".to_string(), content_type.to_string()),
                ("Content-Length".to_string(), content.len().to_string()),
            ];
            (headers, content)
        }
        Err(e) => {
            let error_body = serde_json::to_vec(&json!({
                "errcode": e.code(),
                "error": e.message()
            }))
            .unwrap();
            let headers = [
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Content-Length".to_string(), error_body.len().to_string()),
            ];
            (headers, error_body)
        }
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
    State(_state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> impl IntoResponse {
    let width = params.get("width").and_then(|v| v.as_u64()).unwrap_or(800) as u32;
    let height = params.get("height").and_then(|v| v.as_u64()).unwrap_or(600) as u32;
    let method = params
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("scale");

    let media_service = MediaService::new("media");

    match media_service
        .get_thumbnail(&server_name, &media_id, width, height, method)
        .await
    {
        Ok(content) => {
            let content_type = guess_content_type(&media_id);
            let headers = [
                ("Content-Type".to_string(), content_type.to_string()),
                ("Content-Length".to_string(), content.len().to_string()),
            ];
            (headers, content)
        }
        Err(e) => {
            let error_body = serde_json::to_vec(&json!({
                "errcode": e.code(),
                "error": e.message()
            }))
            .unwrap();
            let headers = [
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Content-Length".to_string(), error_body.len().to_string()),
            ];
            (headers, error_body)
        }
    }
}

async fn upload_media_v1(
    State(_state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let content = body
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("No file provided".to_string()))?;
    let content_type = body
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream");
    let filename = body.get("filename").and_then(|v| v.as_str());

    let content_bytes: Vec<u8> = content
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u8)
        .collect();
    let media_service = MediaService::new("media");
    Ok(Json(
        media_service
            .upload_media(&auth_user.user_id, &content_bytes, content_type, filename)
            .await?,
    ))
}

async fn download_media_v1(
    State(_state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let media_service = MediaService::new("media");

    match media_service.download_media(&server_name, &media_id).await {
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
            .unwrap();
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
