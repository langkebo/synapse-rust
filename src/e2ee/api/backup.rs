use super::super::backup::{BackupKeyUploadParams, BackupKeyUploadRequest, KeyBackupService};
use crate::error::ApiError;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

pub async fn create_backup(
    State(service): State<Arc<KeyBackupService>>,
    Path(user_id): Path<String>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let algorithm = request["algorithm"].as_str().ok_or_else(|| {
        ApiError::bad_request("Missing or invalid 'algorithm' field in request body".to_string())
    })?;
    let auth_data = request.get("auth_data").cloned();

    let version = service
        .create_backup(&user_id, algorithm, auth_data)
        .await?;

    Ok(Json(serde_json::json!({
        "version": version,
    })))
}

pub async fn get_backup(
    State(service): State<Arc<KeyBackupService>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let backup = service.get_backup_version(&user_id).await?;

    Ok(Json(serde_json::json!({
        "version": backup.as_ref().map(|b| b.version.to_string()),
        "algorithm": backup.as_ref().map(|b| b.algorithm.clone()),
        "auth_data": backup.as_ref().map(|b| b.backup_data.clone()),
    })))
}

pub async fn delete_backup(
    State(service): State<Arc<KeyBackupService>>,
    Path((user_id, version)): Path<(String, String)>,
) -> Result<Json<()>, ApiError> {
    service.delete_backup(&user_id, &version).await?;
    Ok(Json(()))
}

pub async fn upload_backup_keys(
    State(service): State<Arc<KeyBackupService>>,
    Path((user_id, room_id, session_id)): Path<(String, String, String)>,
    Json(request): Json<BackupKeyUploadRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let backup = service
        .get_backup_version(&user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

    service
        .upload_backup_key(BackupKeyUploadParams {
            user_id,
            version: backup.version.to_string(),
            room_id,
            session_id,
            first_message_index: request.first_message_index,
            forwarded_count: request.forwarded_count,
            is_verified: request.is_verified,
            session_data: request.session_data,
        })
        .await?;

    Ok(Json(serde_json::json!({
        "etag": format!("{:x}", chrono::Utc::now().timestamp()),
        "count": 1,
    })))
}

pub async fn download_backup_keys(
    State(service): State<Arc<KeyBackupService>>,
    Path((user_id, room_id, session_id, version)): Path<(String, String, String, String)>,
) -> Result<Json<BackupKeyUploadRequest>, ApiError> {
    let backup = service
        .get_backup_key(&user_id, &room_id, &session_id, &version)
        .await?
        .ok_or_else(|| ApiError::not_found("Backup data not found".to_string()))?;

    Ok(Json(BackupKeyUploadRequest {
        first_message_index: backup.first_message_index,
        forwarded_count: backup.forwarded_count,
        is_verified: backup.is_verified,
        session_data: backup.backup_data["session_data"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    }))
}
