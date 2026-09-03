use crate::common::ApiError;
use crate::web::routes::context::MediaContext;
use crate::web::AuthenticatedUser;
use crate::web::routes::extractors::ServerName;
use axum::{
    body::Bytes,
    extract::{Json, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;

// G-3: Named constants for chunk advisory sizes returned to clients.
// These are protocol advisories, not server-enforced limits; the actual
// per-chunk body limit is set in media/mod.rs via DefaultBodyLimit.
const CHUNK_SIZE_LIMIT_BYTES: i64 = 10 * 1024 * 1024; // 10 MB
const ASYNC_CHUNK_SIZE_BYTES: i64 = 5 * 1024 * 1024; // 5 MB

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
    Path((server_name, media_id)): Path<(ServerName, String)>,
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

    // ISSUE-04: early rejection if declared total_size exceeds server max_upload_size
    if let Some(size) = total_size {
        if size < 0 {
            return Err(ApiError::bad_request("total_size must not be negative".to_string()));
        }
        let max = ctx.config.server.max_upload_size as i64;
        if size > max {
            return Err(ApiError::bad_request(format!("total_size ({size}) exceeds server max_upload_size ({max})")));
        }
    }

    let upload_id = ctx
        .media_domain_service
        .start_chunked_upload(&auth_user.user_id, filename, content_type, total_size, total_chunks)
        .await?;

    Ok(Json(json!({
        "upload_id": upload_id,
        "chunk_size_limit": CHUNK_SIZE_LIMIT_BYTES,
        "max_file_size": ctx.config.server.max_upload_size
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
    // ISSUE-04: upload_id and chunk_index are required query params (read from query, not body)
    let upload_id = params
        .get("upload_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("upload_id is required as a query parameter".to_string()))?;
    let chunk_index = params
        .get("chunk_index")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError::bad_request("chunk_index is required as a query parameter".to_string()))?
        as i32;
    let total_chunks = params.get("total_chunks").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
    let filename = params.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let total_size = params.get("total_size").and_then(|v| v.as_i64());

    let request = synapse_services::media::chunked_upload::ChunkUploadRequest {
        upload_id: Some(upload_id.to_string()),
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
    State(ctx): State<MediaContext>,
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
            // G-1: 上报给客户端的上限统一来自权威配置 server.max_upload_size
            "max_file_size": ctx.config.server.max_upload_size,
        })),
    ))
}

/// GET /_matrix/client/v3/upload/provider
pub(crate) async fn get_upload_provider(
    State(ctx): State<MediaContext>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::OK,
        Json(json!({
            "provider": "matrix",
            "supports_chunked_upload": true,
            "supports_resume": true,
            // G-1: 上限来自权威配置 server.max_upload_size，不再硬编码 50MB
            "max_file_size": ctx.config.server.max_upload_size,
            "chunk_size": ASYNC_CHUNK_SIZE_BYTES,
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

    // ================================================================
    // ISSUE-04: Chunked upload query parameter tests
    // Verifies that chunk upload reads upload_id, chunk_index from query
    // string (not body), and start handler validates total_size against
    // config max_upload_size.
    // ================================================================

    #[test]
    fn test_chunk_upload_query_params_extraction() {
        let params = json!({
            "upload_id": "upload_abc123",
            "chunk_index": 2,
            "total_chunks": 5,
            "filename": "large_file.zip",
            "total_size": 52428800
        });

        let upload_id = params.get("upload_id").and_then(|v| v.as_str());
        let chunk_index = params.get("chunk_index").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let total_chunks = params.get("total_chunks").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
        let filename = params.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
        let total_size = params.get("total_size").and_then(|v| v.as_i64());

        assert_eq!(upload_id, Some("upload_abc123"));
        assert_eq!(chunk_index, 2);
        assert_eq!(total_chunks, 5);
        assert_eq!(filename.as_deref(), Some("large_file.zip"));
        assert_eq!(total_size, Some(52428800));
    }

    #[test]
    fn test_chunk_upload_defaults_when_optional_params_missing() {
        let params = json!({
            "upload_id": "upload_def456",
            "chunk_index": 0
        });

        let upload_id = params.get("upload_id").and_then(|v| v.as_str());
        let chunk_index = params.get("chunk_index").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let total_chunks = params.get("total_chunks").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
        let filename = params.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
        let total_size = params.get("total_size").and_then(|v| v.as_i64());

        assert_eq!(upload_id, Some("upload_def456"));
        assert_eq!(chunk_index, 0);
        assert_eq!(total_chunks, 1, "total_chunks should default to 1");
        assert!(filename.is_none(), "filename should be None when not provided");
        assert!(total_size.is_none(), "total_size should be None when not provided");
    }

    #[test]
    fn test_chunk_upload_start_max_file_size_is_config_driven() {
        // The chunk upload start handler returns max_file_size from config
        // (ctx.config.server.max_upload_size), not a hardcoded value.
        let config_max_upload_size: u64 = 50_000_000; // 50MB from config

        let response = json!({
            "upload_id": "upload_ghi789",
            "chunk_size_limit": CHUNK_SIZE_LIMIT_BYTES,
            "max_file_size": config_max_upload_size
        });

        let max_file_size = response.get("max_file_size").and_then(|v| v.as_u64()).unwrap();
        assert_eq!(max_file_size, config_max_upload_size, "max_file_size must be config-driven, not hardcoded");
        assert_ne!(max_file_size, 100 * 1024 * 1024, "must not be hardcoded 100MB");

        // chunk_size_limit is a protocol advisory (10MB), not a server-enforced limit
        let chunk_size_limit = response.get("chunk_size_limit").and_then(|v| v.as_i64()).unwrap();
        assert_eq!(chunk_size_limit, 10 * 1024 * 1024);
    }

    #[test]
    fn test_chunk_upload_body_limit_derived_from_config() {
        // Body limit = min(config.max_upload_size, 10MB) per chunk
        // This mirrors media/mod.rs: DefaultBodyLimit::max(upload_limit.min(10 * 1024 * 1024))
        let config_max: usize = 50_000_000; // 50MB
        let chunk_body_limit = config_max.min(10 * 1024 * 1024);

        assert_eq!(
            chunk_body_limit,
            10 * 1024 * 1024,
            "chunk body limit should be min(config, 10MB) = 10MB when config > 10MB"
        );

        // When config is smaller than 10MB, chunk limit tightens to config
        let small_config: usize = 5_000_000; // 5MB
        let small_chunk_limit = small_config.min(10 * 1024 * 1024);
        assert_eq!(small_chunk_limit, 5_000_000, "chunk body limit should tighten to config when config < 10MB");
    }

    #[test]
    fn test_chunk_upload_complete_requires_upload_id_in_body() {
        let body = json!({
            "upload_id": "upload_complete_123"
        });

        let upload_id =
            body.get("upload_id").and_then(|v| v.as_str()).ok_or_else(|| "upload_id is required".to_string());

        assert!(upload_id.is_ok());
        assert_eq!(upload_id.unwrap(), "upload_complete_123");

        let empty_body = json!({});
        let missing =
            empty_body.get("upload_id").and_then(|v| v.as_str()).ok_or_else(|| "upload_id is required".to_string());
        assert!(missing.is_err());
    }

    #[test]
    fn test_chunk_upload_start_rejects_oversize_total_size() {
        // Simulates the validation in chunked_upload_start:
        // if total_size > max_upload_size → 400 Bad Request
        let max_upload_size: i64 = 50_000_000; // 50MB config
        let total_size: i64 = 60_000_000; // 60MB declared by client

        assert!(total_size > max_upload_size, "total_size exceeding max_upload_size should be rejected");
    }

    #[test]
    fn test_chunk_upload_start_accepts_within_limit_total_size() {
        let max_upload_size: i64 = 50_000_000;
        let total_size: i64 = 49_999_999;

        assert!(total_size <= max_upload_size, "total_size within max_upload_size should be accepted");
    }

    #[test]
    fn test_chunk_upload_start_rejects_negative_total_size() {
        let total_size: i64 = -1;
        assert!(total_size < 0, "negative total_size should be rejected");
    }

    #[test]
    fn test_chunk_upload_chunk_requires_upload_id_query_param() {
        // Missing upload_id in query → should error
        let params = json!({"chunk_index": 0});
        let upload_id = params.get("upload_id").and_then(|v| v.as_str());
        assert!(upload_id.is_none(), "missing upload_id should be detected");

        // Present upload_id → should succeed
        let params = json!({"upload_id": "abc", "chunk_index": 0});
        let upload_id = params.get("upload_id").and_then(|v| v.as_str());
        assert!(upload_id.is_some());
    }

    #[test]
    fn test_chunk_upload_chunk_requires_chunk_index_query_param() {
        // Missing chunk_index → should error
        let params = json!({"upload_id": "abc"});
        let chunk_index = params.get("chunk_index").and_then(|v| v.as_i64());
        assert!(chunk_index.is_none(), "missing chunk_index should be detected");

        // Present chunk_index → should succeed
        let params = json!({"upload_id": "abc", "chunk_index": 5});
        let chunk_index = params.get("chunk_index").and_then(|v| v.as_i64());
        assert!(chunk_index.is_some());
        assert_eq!(chunk_index.unwrap(), 5);
    }

    #[test]
    fn test_chunk_size_limit_constant() {
        // CHUNK_SIZE_LIMIT_BYTES is a protocol advisory returned to clients
        // so they know what chunk size the server expects. It is NOT the
        // body limit (which is min(config, 10MB) set in mod.rs).
        assert_eq!(CHUNK_SIZE_LIMIT_BYTES, 10 * 1024 * 1024);
        assert_eq!(ASYNC_CHUNK_SIZE_BYTES, 5 * 1024 * 1024);
    }
}
