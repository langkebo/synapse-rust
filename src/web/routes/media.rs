use super::AppState;
use crate::cache::*;
use crate::common::*;
use crate::services::*;
use axum::{
    extract::{Json, Path, Query, State},
    response::IntoResponse,
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn create_media_router(state: Arc<AppState>, media_path: std::path::PathBuf) -> Router {
    Router::new()
        .route("/_matrix/media/r0/upload", post(upload_media))
        .route(
            "/_matrix/media/r0/download/:server_name/:media_id",
            get(download_media),
        )
        .route("/_matrix/media/r0/preview_url", get(preview_url))
        .route(
            "/_matrix/media/r0/thumbnail/:server_name/:media_id",
            get(get_thumbnail),
        )
        .route("/_matrix/media/v1/config", get(media_config))
        .route("/_matrix/media/r1/upload", post(upload_media_v1))
        .route(
            "/_matrix/media/r1/download/:server_name/:media_id",
            get(download_media_v1),
        )
        .with_state((state, media_path))
}

async fn upload_media(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("Missing access token".to_string()))?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let content = body
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("No file provided".to_string()))?;
    let content_type = body
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream");
    let filename = body.get("filename").and_then(|v| v.as_str());

    let media_service = MediaService::new(&state.services, std::path::PathBuf::from("media"));
    media_service
        .upload_media(
            &user_id,
            &content
                .iter()
                .map(|v| v.as_u64().unwrap_or(0) as u8)
                .collect(),
            content_type,
            filename,
        )
        .await
}

async fn download_media(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let media_service = MediaService::new(&state.services, std::path::PathBuf::from("media"));

    match media_service.download_media(&server_name, &media_id).await {
        Ok(content) => {
            let content_type = guess_content_type(&media_id);
            (
                [
                    ("Content-Type", content_type),
                    ("Content-Length", content.len().to_string()),
                ],
                content,
            )
        }
        Err(e) => (
            [("Content-Type", "application/json")],
            serde_json::to_vec(&json!({
                "errcode": e.code,
                "error": e.message
            }))
            .unwrap(),
        ),
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
) -> impl IntoResponse {
    let width = params.get("width").and_then(|v| v.as_u64()).unwrap_or(800);
    let height = params.get("height").and_then(|v| v.as_u64()).unwrap_or(600);
    let method = params
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("scale");

    let media_service = MediaService::new(&state.services, std::path::PathBuf::from("media"));

    match media_service.get_thumbnail(&server_name, &media_id, width, height, method).await {
        Ok(content) => {
            let content_type = guess_content_type(&media_id);
            (
                [
                    ("Content-Type", content_type),
                    ("Content-Length", content.len().to_string()),
                ],
                content,
            )
        }
        Err(e) => (
            [("Content-Type", "application/json")],
            serde_json::to_vec(&json!({
                "errcode": e.code,
                "error": e.message
            }))
            .unwrap(),
        ),
    }
}

async fn media_config() -> Json<Value> {
    Json(json!({
        "m.upload.size": 50 * 1024 * 1024
    }))
}

async fn upload_media_v1(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("Missing access token".to_string()))?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let content = body
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("No file provided".to_string()))?;
    let content_type = body
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream");
    let filename = body.get("filename").and_then(|v| v.as_str());

    let media_service = MediaService::new(&state.services, std::path::PathBuf::from("media"));
    media_service
        .upload_media(
            &user_id,
            &content
                .iter()
                .map(|v| v.as_u64().unwrap_or(0) as u8)
                .collect(),
            content_type,
            filename,
        )
        .await
}

async fn download_media_v1(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let media_service = MediaService::new(&state.services, std::path::PathBuf::from("media"));

    match media_service.download_media(&server_name, &media_id).await {
        Ok(content) => {
            let content_type = guess_content_type(&media_id);
            (
                [
                    ("Content-Type", content_type),
                    ("Content-Length", content.len().to_string()),
                ],
                content,
            )
        }
        Err(e) => (
            [("Content-Type", "application/json")],
            serde_json::to_vec(&json!({
                "errcode": e.code,
                "error": e.message
            }))
            .unwrap(),
        ),
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
