use crate::common::ApiError;
use crate::web::routes::context::MediaContext;
use crate::web::AuthenticatedUser;
use axum::{
    body::Bytes,
    extract::{Json, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;

// ---------------------------------------------------------------------------
// Shared upload helpers
// ---------------------------------------------------------------------------

/// Extract an upload filename from query params or the Content-Disposition header.
pub(crate) fn parse_upload_filename(headers: &HeaderMap, query_params: &Value) -> Option<String> {
    if let Some(filename) = query_params.get("filename").and_then(|v| v.as_str()) {
        if !filename.is_empty() {
            return Some(filename.to_string());
        }
    }
    headers.get(header::CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()).and_then(|v| {
        v.split(';').map(|part| part.trim()).find(|part| part.starts_with("filename=")).and_then(|part| {
            let name = part.trim_start_matches("filename=").trim_matches('"');
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
    })
}

pub(crate) fn ensure_local_media_server_name(ctx: &MediaContext, server_name: &str) -> Result<(), ApiError> {
    if server_name != ctx.server_name {
        return Err(ApiError::not_found("Media not found".to_string()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Upload common helpers
// ---------------------------------------------------------------------------

pub(crate) async fn upload_media_common(
    ctx: &MediaContext,
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

    let filename = parse_upload_filename(headers, params);
    let content_bytes = body.to_vec();

    if content_bytes.is_empty() {
        return Err(ApiError::bad_request("No file content provided".to_string()));
    }

    Ok(Json(ctx.media_domain_service.upload_media(user_id, &content_bytes, content_type, filename.as_deref()).await?))
}

pub(crate) async fn upload_media_with_id_common(
    ctx: &MediaContext,
    user_id: &str,
    server_name: &str,
    media_id: &str,
    params: &Value,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    if server_name != ctx.server_name {
        return Err(ApiError::bad_request(format!("server_name must match local server: {}", ctx.server_name)));
    }

    let content_type = params
        .get("content_type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()))
        .unwrap_or("application/octet-stream");

    let filename = parse_upload_filename(headers, params);
    let content_bytes = body.to_vec();

    if content_bytes.is_empty() {
        return Err(ApiError::bad_request("No file content provided".to_string()));
    }

    Ok(Json(
        ctx.media_domain_service
            .upload_media_with_id(user_id, media_id, &content_bytes, content_type, filename.as_deref())
            .await?,
    ))
}

// ---------------------------------------------------------------------------
// Upload handlers
// ---------------------------------------------------------------------------

pub(crate) async fn upload_media_v3(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    upload_media_common(&ctx, &auth_user.user_id, &params, &headers, body).await
}

pub(crate) async fn upload_media_v1(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    upload_media_common(&ctx, &auth_user.user_id, &params, &headers, body).await
}

pub(crate) async fn upload_media_with_id(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    upload_media_with_id_common(&ctx, &auth_user.user_id, &server_name, &media_id, &params, &headers, body).await
}

// ---------------------------------------------------------------------------
// Chunked upload handlers
// ---------------------------------------------------------------------------

/// POST /_matrix/media/v1/upload/chunk/start
pub(crate) async fn chunked_upload_start(
    State(ctx): State<MediaContext>,
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

    let upload_id = ctx
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
pub(crate) async fn chunked_upload_chunk(
    State(ctx): State<MediaContext>,
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

    let request = synapse_services::media::chunked_upload::ChunkUploadRequest {
        upload_id: upload_id.map(|s| s.to_string()),
        chunk_index,
        total_chunks,
        chunk_data: body.to_vec(),
        filename,
        content_type,
        total_size,
    };

    let response = ctx.media_domain_service.upload_chunk(request, &auth_user.user_id).await?;

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
pub(crate) async fn chunked_upload_complete(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let upload_id = body
        .get("upload_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("upload_id is required".to_string()))?;

    let response = ctx.media_domain_service.complete_chunked_upload(upload_id, &auth_user.user_id).await?;

    Ok(Json(json!({
        "content_uri": response.content_uri,
        "media_id": response.media_id,
        "size": response.size
    })))
}

/// POST /_matrix/media/v1/upload/chunk/cancel
pub(crate) async fn chunked_upload_cancel(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let upload_id = body
        .get("upload_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("upload_id is required".to_string()))?;

    ctx.media_domain_service.cancel_chunked_upload(upload_id, &auth_user.user_id).await?;

    Ok(Json(json!({
        "cancelled": true,
        "upload_id": upload_id
    })))
}

/// GET /_matrix/media/v1/upload/chunk/progress
pub(crate) async fn chunked_upload_progress(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let upload_id = params
        .get("upload_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("upload_id is required".to_string()))?;

    let progress = ctx.media_domain_service.get_chunked_upload_progress(upload_id).await?;

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

// ---------------------------------------------------------------------------
// Upload provider handlers
// ---------------------------------------------------------------------------

/// POST /_matrix/client/v3/upload/token
pub(crate) async fn create_upload_token(
    State(_ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let filename = body.get("filename").and_then(|v| v.as_str()).unwrap_or("upload");
    let content_type = body.get("content_type").and_then(|v| v.as_str()).unwrap_or("application/octet-stream");

    let token = format!("upload_{}_{}", auth_user.user_id, current_timestamp_millis());

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
pub(crate) async fn get_upload_provider(
    State(_ctx): State<MediaContext>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_parse_upload_filename_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Disposition", "attachment; filename=\"test.jpg\"".parse().unwrap());
        let result = parse_upload_filename(&headers, &serde_json::Value::Null);
        assert_eq!(result, Some("test.jpg".to_string()));
    }

    #[test]
    fn test_parse_upload_filename_from_query() {
        let headers = HeaderMap::new();
        let params = serde_json::json!({"filename": "photo.png"});
        let result = parse_upload_filename(&headers, &params);
        assert_eq!(result, Some("photo.png".to_string()));
    }

    #[test]
    fn test_parse_upload_filename_query_takes_priority() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Disposition", "attachment; filename=\"header.jpg\"".parse().unwrap());
        let params = serde_json::json!({"filename": "query.png"});
        let result = parse_upload_filename(&headers, &params);
        assert_eq!(result, Some("query.png".to_string()));
    }

    #[test]
    fn test_parse_upload_filename_none_when_missing() {
        let headers = HeaderMap::new();
        let result = parse_upload_filename(&headers, &serde_json::Value::Null);
        assert_eq!(result, None);
    }
}
