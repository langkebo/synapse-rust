use super::super::backup::{BackupKeyUploadParams, BackupKeyUploadRequest, KeyBackupService};
use crate::error::ApiError;
use crate::web::routes::extractors::auth::AuthenticatedUser;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn create_backup(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<KeyBackupService>>,
    Path(user_id): Path<String>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Cannot create backup for another user"));
    }

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

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn get_backup(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<KeyBackupService>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Cannot access backup for another user"));
    }

    let backup = service.get_backup_version(&user_id).await?;

    Ok(Json(serde_json::json!({
        "version": backup.as_ref().map(|b| b.version.to_string()),
        "algorithm": backup.as_ref().map(|b| b.algorithm.clone()),
        "auth_data": backup.as_ref().map(|b| b.backup_data.clone()),
    })))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn delete_backup(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<KeyBackupService>>,
    Path((user_id, version)): Path<(String, String)>,
) -> Result<Json<()>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Cannot delete backup for another user"));
    }

    service.delete_backup(&user_id, &version).await?;
    Ok(Json(()))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn upload_backup_keys(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<KeyBackupService>>,
    Path((user_id, room_id, session_id)): Path<(String, String, String)>,
    Json(request): Json<BackupKeyUploadRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Cannot upload keys for another user"));
    }

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

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn download_backup_keys(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<KeyBackupService>>,
    Path((user_id, room_id, session_id, version)): Path<(String, String, String, String)>,
) -> Result<Json<BackupKeyUploadRequest>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Cannot download keys for another user"));
    }

    let backup = service
        .get_backup_key(&user_id, &room_id, &session_id, &version)
        .await?
        .ok_or_else(|| ApiError::not_found("Backup data not found".to_string()))?;

    Ok(Json(BackupKeyUploadRequest {
        first_message_index: backup.first_message_index,
        forwarded_count: backup.forwarded_count,
        is_verified: backup.is_verified,
        session_data: backup.session_data["session_data"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    }))
}
