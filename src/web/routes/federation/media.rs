use crate::common::*;
use crate::web::routes::AppState;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde_json::Value;

fn validate_federation_media_server_name(
    state: &AppState,
    server_name: &str,
) -> Result<(), ApiError> {
    if server_name != state.services.server_name {
        return Err(ApiError::not_found(
            "Media is not hosted on this server".to_string(),
        ));
    }

    Ok(())
}

fn parse_federation_query_i64(params: &Value, key: &str, default: i64) -> Result<i64, ApiError> {
    match params.get(key) {
        Some(Value::Number(value)) => value
            .as_i64()
            .ok_or_else(|| ApiError::bad_request(format!("Invalid '{key}' parameter"))),
        Some(Value::String(value)) => value
            .parse::<i64>()
            .map_err(|_| ApiError::bad_request(format!("Invalid '{key}' parameter"))),
        Some(_) => Err(ApiError::bad_request(format!(
            "Invalid '{key}' parameter"
        ))),
        None => Ok(default),
    }
}

pub(super) async fn media_download(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    validate_federation_media_server_name(&state, &server_name)?;

    if media_id.is_empty() {
        return Err(ApiError::bad_request("Missing media_id"));
    }

    let content = state
        .services
        .media_service
        .download_media(&server_name, &media_id)
        .await?;
    let content_type = federation_guess_content_type(&media_id, &content).to_string();
    let headers = federation_media_response_headers(content_type, content.len());

    Ok((headers, content))
}

pub(super) async fn media_thumbnail(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    validate_federation_media_server_name(&state, &server_name)?;

    let width = parse_federation_query_i64(&params, "width", 100)?;
    let height = parse_federation_query_i64(&params, "height", 100)?;
    let method = params
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("scale");

    const MAX_FEDERATION_THUMBNAIL_DIMENSION: i64 = 4096;
    if width < 1
        || height < 1
        || width > MAX_FEDERATION_THUMBNAIL_DIMENSION
        || height > MAX_FEDERATION_THUMBNAIL_DIMENSION
    {
        return Err(ApiError::bad_request(format!(
            "Thumbnail dimensions must be between 1 and {MAX_FEDERATION_THUMBNAIL_DIMENSION}"
        )));
    }

    let content = state
        .services
        .media_service
        .get_thumbnail(&server_name, &media_id, width as u32, height as u32, method)
        .await?;
    let content_type = federation_guess_content_type(&media_id, &content).to_string();
    let headers = federation_media_response_headers(content_type, content.len());

    Ok((headers, content))
}

fn federation_media_response_headers(
    content_type: String,
    content_length: usize,
) -> [(String, String); 2] {
    [
        ("Content-Type".to_string(), content_type),
        ("Content-Length".to_string(), content_length.to_string()),
    ]
}

fn federation_guess_content_type(filename: &str, data: &[u8]) -> &'static str {
    if let Some(kind) = infer::get(data) {
        return kind.mime_type();
    }

    let lower = filename.to_ascii_lowercase();

    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".webm") {
        "video/webm"
    } else if lower.ends_with(".ogg") {
        "audio/ogg"
    } else if lower.ends_with(".mp3") {
        "audio/mpeg"
    } else if lower.ends_with(".wav") {
        "audio/wav"
    } else {
        "application/octet-stream"
    }
}
