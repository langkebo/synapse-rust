use axum::{
    extract::{State, Path},
    Json,
};
use super::super::backup::{BackupKeyService, BackupUploadRequest};
use std::sync::Arc;
use crate::error::ApiError;

pub async fn create_backup(
    State(service): State<Arc<BackupKeyService>>,
    Path(user_id): Path<String>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let algorithm = request["algorithm"].as_str().unwrap();
    
    let version = service.create_backup(&user_id, algorithm).await?;
    
    Ok(Json(serde_json::json!({
        "version": version.version,
        "algorithm": version.algorithm,
        "auth_data": version.auth_data,
    })))
}

pub async fn get_backup(
    State(service): State<Arc<BackupKeyService>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let backup = service.get_backup(&user_id).await?;
    
    Ok(Json(serde_json::json!({
        "version": backup.as_ref().map(|b| b.version.clone()),
        "algorithm": backup.as_ref().map(|b| b.algorithm.clone()),
        "auth_data": backup.as_ref().map(|b| b.auth_data.clone()),
        "count": backup.as_ref().map(|b| b.count).unwrap_or(0),
        "etag": backup.as_ref().map(|b| b.etag.clone()),
    })))
}

pub async fn delete_backup(
    State(service): State<Arc<BackupKeyService>>,
    Path((user_id, version)): Path<(String, String)>,
) -> Result<Json<()>, ApiError> {
    service.delete_backup(&user_id, &version).await?;
    Ok(Json(()))
}

pub async fn upload_backup_keys(
    State(service): State<Arc<BackupKeyService>>,
    Path((user_id, room_id, session_id)): Path<(String, String, String)>,
    Json(request): Json<BackupUploadRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let response = service.upload_backup(&user_id, &room_id, &session_id, request).await?;
    
    Ok(Json(serde_json::json!({
        "etag": response.etag,
        "count": response.count,
    })))
}

pub async fn download_backup_keys(
    State(service): State<Arc<BackupKeyService>>,
    Path((user_id, room_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<BackupUploadRequest>, ApiError> {
    let backup = service.download_backup(&user_id, &room_id, &session_id).await?
        .ok_or_else(|| ApiError::NotFound("Backup data not found".to_string()))?;
    
    Ok(Json(backup))
}