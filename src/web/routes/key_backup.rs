use super::{AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use validator::Validate;

pub fn create_key_backup_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/r0/room_keys/version",
            get(get_all_backup_versions),
        )
        .route(
            "/_matrix/client/r0/room_keys/version",
            post(create_backup_version),
        )
        .route(
            "/_matrix/client/r0/room_keys/version/{version}",
            get(get_backup_version),
        )
        .route(
            "/_matrix/client/r0/room_keys/version/{version}",
            put(update_backup_version),
        )
        .route(
            "/_matrix/client/r0/room_keys/version/{version}",
            delete(delete_backup_version),
        )
        .route("/_matrix/client/r0/room_keys/keys", get(get_room_keys_all))
        .route("/_matrix/client/r0/room_keys/keys", put(put_room_keys_all))
        .route("/_matrix/client/r0/room_keys/{version}", get(get_room_keys))
        .route("/_matrix/client/r0/room_keys/{version}", put(put_room_keys))
        .route(
            "/_matrix/client/r0/room_keys/{version}/keys",
            post(put_room_keys_multi),
        )
        .route(
            "/_matrix/client/r0/room_keys/{version}/keys/{room_id}",
            get(get_room_key_by_id),
        )
        .route(
            "/_matrix/client/r0/room_keys/{version}/keys/{room_id}/{session_id}",
            get(get_room_key),
        )
        .route("/_matrix/client/r0/room_keys/recover", post(recover_keys))
        .route(
            "/_matrix/client/r0/room_keys/recovery/{version}/progress",
            get(get_recovery_progress),
        )
        .route(
            "/_matrix/client/r0/room_keys/verify/{version}",
            get(verify_backup),
        )
        .route(
            "/_matrix/client/r0/room_keys/batch_recover",
            post(batch_recover_keys),
        )
        .route(
            "/_matrix/client/r0/room_keys/recover/{version}/{room_id}",
            get(recover_room_keys),
        )
        .route(
            "/_matrix/client/r0/room_keys/recover/{version}/{room_id}/{session_id}",
            get(recover_session_key),
        )
        .route(
            "/_matrix/client/v3/room_keys/version",
            get(get_all_backup_versions).post(create_backup_version),
        )
        .route(
            "/_matrix/client/v3/room_keys/version/{version}",
            get(get_backup_version)
                .put(update_backup_version)
                .delete(delete_backup_version),
        )
        .route(
            "/_matrix/client/v3/room_keys/keys",
            get(get_room_keys_all).put(put_room_keys_all),
        )
        .route(
            "/_matrix/client/v3/room_keys/keys/{version}",
            get(get_room_keys).put(put_room_keys),
        )
        // Key Export/Import (E2EE 100%)
        .route(
            "/_matrix/client/r0/room_keys/export",
            get(export_keys),
        )
        .route(
            "/_matrix/client/r0/room_keys/export/{version}",
            get(export_keys_by_version),
        )
        .route(
            "/_matrix/client/r0/room_keys/import",
            post(import_keys),
        )
        .route(
            "/_matrix/client/r0/room_keys/import/{version}",
            post(import_keys_by_version),
        )
        // v3 routes
        .route(
            "/_matrix/client/v3/room_keys/export",
            get(export_keys),
        )
        .route(
            "/_matrix/client/v3/room_keys/export/{version}",
            get(export_keys_by_version),
        )
        .route(
            "/_matrix/client/v3/room_keys/import",
            post(import_keys),
        )
        .route(
            "/matrix/client/v3/room_keys/import/{version}",
            post(import_keys_by_version),
        )
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateBackupVersionBody {
    #[validate(length(max = 255, message = "Algorithm name too long"))]
    pub algorithm: Option<String>,
    pub auth_data: Option<Value>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateBackupVersionBody {
    pub auth_data: Option<Value>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PutRoomKeysBody {
    #[validate(length(min = 1, max = 255))]
    pub room_id: Option<String>,
    pub sessions: Option<Vec<Value>>,
}

#[axum::debug_handler]
async fn create_backup_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateBackupVersionBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(crate::error::ApiError::bad_request(e.to_string()));
    }

    let algorithm = body.algorithm.as_deref().unwrap_or("m.megolm.v1.aes-sha2");
    let auth_data = body.auth_data;

    let version = state
        .services
        .backup_service
        .create_backup(&auth_user.user_id, algorithm, auth_data)
        .await?;

    Ok(Json(serde_json::json!({
        "version": version
    })))
}

#[axum::debug_handler]
async fn get_all_backup_versions(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backups = state
        .services
        .backup_service
        .get_all_backups(&auth_user.user_id)
        .await?;

    let versions: Vec<Value> = backups
        .into_iter()
        .map(|b| {
            serde_json::json!({
                "algorithm": b.algorithm,
                "auth_data": b.backup_data,
                "version": b.version.to_string()
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "versions": versions
    })))
}

#[axum::debug_handler]
async fn get_backup_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backup = state
        .services
        .backup_service
        .get_backup(&auth_user.user_id, &version)
        .await?;

    match backup {
        Some(b) => Ok(Json(serde_json::json!({
            "algorithm": b.algorithm,
            "auth_data": b.backup_data,
            "version": b.version.to_string()
        }))),
        None => Err(crate::error::ApiError::not_found(format!(
            "Backup version '{}' not found",
            version
        ))),
    }
}

#[axum::debug_handler]
async fn update_backup_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
    Json(body): Json<UpdateBackupVersionBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(crate::error::ApiError::bad_request(e.to_string()));
    }

    let auth_data = body.auth_data;

    state
        .services
        .backup_service
        .update_backup_auth_data(&auth_user.user_id, &version, auth_data)
        .await?;

    Ok(Json(serde_json::json!({
        "version": version
    })))
}

#[axum::debug_handler]
async fn delete_backup_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backup = state
        .services
        .backup_service
        .get_backup(&auth_user.user_id, &version)
        .await?;

    if backup.is_none() {
        return Err(crate::error::ApiError::not_found(format!(
            "Backup version '{}' not found",
            version
        )));
    }

    state
        .services
        .backup_service
        .delete_backup(&auth_user.user_id, &version)
        .await?;

    Ok(Json(serde_json::json!({
        "deleted": true,
        "version": version
    })))
}

#[axum::debug_handler]
async fn get_room_keys_all(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backups = state
        .services
        .backup_service
        .get_all_backups(&auth_user.user_id)
        .await?;

    let latest_backup = backups.into_iter().max_by_key(|b| b.version);

    if let Some(backup) = latest_backup {
        let rooms = state
            .services
            .backup_service
            .get_backup_count_per_room(&auth_user.user_id, &backup.version.to_string())
            .await?;

        Ok(Json(serde_json::json!({
            "rooms": rooms,
            "etag": backup.etag.clone().unwrap_or_else(|| backup.version.to_string())
        })))
    } else {
        Ok(Json(serde_json::json!({
            "rooms": {},
            "etag": "0"
        })))
    }
}

#[axum::debug_handler]
async fn put_room_keys_all(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<PutRoomKeysBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(crate::error::ApiError::bad_request(e.to_string()));
    }

    let backups = state
        .services
        .backup_service
        .get_all_backups(&auth_user.user_id)
        .await?;

    let latest_backup = backups.into_iter().max_by_key(|b| b.version);

    let version = if let Some(backup) = latest_backup {
        backup.version.to_string()
    } else {
        return Err(crate::error::ApiError::not_found("No backup version found"));
    };

    let room_id = body.room_id.as_deref().unwrap_or("");
    let sessions = body.sessions.unwrap_or_default();

    state
        .services
        .backup_service
        .upload_room_keys_for_room(&auth_user.user_id, room_id, &version, sessions.clone())
        .await?;

    Ok(Json(serde_json::json!({
        "count": sessions.len(),
        "etag": version
    })))
}

#[axum::debug_handler]
async fn get_room_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backup = state
        .services
        .backup_service
        .get_backup(&auth_user.user_id, &version)
        .await?
        .ok_or_else(|| {
            crate::error::ApiError::not_found(format!("Backup version '{}' not found", version))
        })?;

    let rooms = state
        .services
        .backup_service
        .get_backup_count_per_room(&auth_user.user_id, &version)
        .await?;

    Ok(Json(serde_json::json!({
        "rooms": rooms,
        "etag": backup.etag.clone().unwrap_or_else(|| backup.version.to_string())
    })))
}

#[axum::debug_handler]
async fn put_room_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
    Json(body): Json<PutRoomKeysBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(crate::error::ApiError::bad_request(e.to_string()));
    }

    let room_id = body.room_id.as_deref().unwrap_or("");
    let sessions = body.sessions.unwrap_or_default();

    state
        .services
        .backup_service
        .upload_room_keys_for_room(&auth_user.user_id, room_id, &version, sessions.clone())
        .await?;

    Ok(Json(serde_json::json!({
        "count": sessions.len(),
        "etag": format!("{}_{}", version, chrono::Utc::now().timestamp())
    })))
}

#[axum::debug_handler]
async fn put_room_keys_multi(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let mut total_count = 0;

    if let Some(rooms) = body.as_object() {
        for (room_id, room_data) in rooms {
            if let Some(sessions_obj) = room_data.get("sessions") {
                if let Some(sessions) = sessions_obj.as_array() {
                    for session in sessions {
                        if let Err(e) = state
                            .services
                            .backup_service
                            .upload_room_key(
                                &auth_user.user_id,
                                room_id,
                                session["session_id"].as_str().unwrap_or(""),
                                session,
                            )
                            .await
                        {
                            tracing::warn!(
                                target: "key_backup",
                                "Failed to upload room key: {}",
                                e
                            );
                        }
                    }
                    total_count += sessions.len();
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "count": total_count,
        "etag": format!("{}_{}", version, chrono::Utc::now().timestamp())
    })))
}

#[axum::debug_handler]
async fn get_room_key_by_id(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| crate::error::ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(crate::error::ApiError::not_found(format!(
            "Room '{}' not found",
            room_id
        )));
    }

    let keys = state
        .services
        .backup_service
        .get_room_backup_keys(&auth_user.user_id, &room_id, &version)
        .await?;

    let mut sessions = serde_json::Map::new();
    for key in keys {
        let session_data = key
            .backup_data
            .get("session_data")
            .cloned()
            .unwrap_or_else(|| key.backup_data.clone());

        sessions.insert(
            key.session_id,
            serde_json::json!({
                "first_message_index": key.first_message_index,
                "forwarded_count": key.forwarded_count,
                "is_verified": key.is_verified,
                "session_data": session_data
            }),
        );
    }

    Ok(Json(serde_json::json!({
        "rooms": {
            room_id: {
                "sessions": sessions
            }
        }
    })))
}

#[axum::debug_handler]
async fn get_room_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let key = state
        .services
        .backup_service
        .get_backup_key(&auth_user.user_id, &room_id, &session_id, &version)
        .await?;

    match key {
        Some(k) => Ok(Json(serde_json::json!({
            "room_id": room_id,
            "session_id": session_id,
            "first_message_index": k.first_message_index,
            "forwarded_count": k.forwarded_count,
            "is_verified": k.is_verified,
            "session_data": k.backup_data.get("session_data").cloned().unwrap_or(k.backup_data)
        }))),
        None => Err(crate::error::ApiError::not_found(format!(
            "Session '{}' not found in room '{}'",
            session_id, room_id
        ))),
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct RecoverKeysBody {
    pub version: String,
    pub rooms: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct BatchRecoverBody {
    pub version: String,
    pub room_ids: Vec<String>,
    pub session_limit: Option<i32>,
}

#[axum::debug_handler]
async fn recover_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<RecoverKeysBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(crate::error::ApiError::bad_request(e.to_string()));
    }

    let response = state
        .services
        .backup_service
        .recover_keys(&auth_user.user_id, &body.version, body.rooms)
        .await?;

    Ok(Json(serde_json::to_value(response)?))
}

#[axum::debug_handler]
async fn get_recovery_progress(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let progress = state
        .services
        .backup_service
        .get_recovery_progress(&auth_user.user_id, &version)
        .await?;

    Ok(Json(serde_json::to_value(progress)?))
}

#[axum::debug_handler]
async fn verify_backup(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let verification = state
        .services
        .backup_service
        .verify_backup(&auth_user.user_id, &version)
        .await?;

    Ok(Json(serde_json::to_value(verification)?))
}

#[axum::debug_handler]
async fn batch_recover_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<BatchRecoverBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(crate::error::ApiError::bad_request(e.to_string()));
    }

    let response = state
        .services
        .backup_service
        .batch_recover_keys(
            &auth_user.user_id,
            crate::e2ee::backup::models::BatchRecoveryRequest {
                version: body.version,
                room_ids: body.room_ids,
                session_limit: body.session_limit,
            },
        )
        .await?;

    Ok(Json(serde_json::to_value(response)?))
}

#[axum::debug_handler]
async fn recover_room_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let keys = state
        .services
        .backup_service
        .recover_room_keys(&auth_user.user_id, &version, &room_id)
        .await?;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "sessions": keys
    })))
}

#[axum::debug_handler]
async fn recover_session_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let key = state
        .services
        .backup_service
        .recover_session_key(&auth_user.user_id, &version, &room_id, &session_id)
        .await?;

    match key {
        Some(k) => Ok(Json(serde_json::json!({
            "room_id": room_id,
            "session_id": session_id,
            "session_data": k
        }))),
        None => Err(crate::error::ApiError::not_found(format!(
            "Session '{}' not found in room '{}'",
            session_id, room_id
        ))),
    }
}

// ============================================================================
// Key Export/Import (E2EE 100%)
// ============================================================================

/// Export all keys
/// GET /_matrix/client/r0/room_keys/export
#[axum::debug_handler]
async fn export_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backup_keys = state.services.backup_service.get_all_backup_keys(&auth_user.user_id).await?;

    let mut room_keys = Vec::new();
    for key in backup_keys {
        room_keys.push(serde_json::json!({
            "room_id": key.room_id,
            "session_id": key.session_id,
            "session_data": key.backup_data,
            "first_message_index": key.first_message_index,
            "forwarded_count": key.forwarded_count,
            "is_verified": key.is_verified
        }));
    }

    let export_data = serde_json::json!({
        "room_keys": room_keys,
        "version": "1"
    });

    Ok(Json(export_data))
}

/// Export keys by version
/// GET /_matrix/client/r0/room_keys/export/{version}
#[axum::debug_handler]
async fn export_keys_by_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backup_keys = state.services.backup_service.get_all_backup_keys(&auth_user.user_id).await?;

    let mut room_keys = Vec::new();
    for key in backup_keys {
        room_keys.push(serde_json::json!({
            "room_id": key.room_id,
            "session_id": key.session_id,
            "session_data": key.backup_data,
            "first_message_index": key.first_message_index,
            "forwarded_count": key.forwarded_count,
            "is_verified": key.is_verified
        }));
    }

    let export_data = serde_json::json!({
        "room_keys": room_keys,
        "version": version
    });

    Ok(Json(export_data))
}

/// Import keys
/// POST /_matrix/client/r0/room_keys/import
#[axum::debug_handler]
async fn import_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let room_keys = body.get("room_keys")
        .and_then(|v| v.as_array())
        .ok_or_else(|| crate::error::ApiError::bad_request("Missing room_keys".to_string()))?;
    
    let version = body.get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("1");

    let mut imported_count = 0;
    let mut failed_count = 0;

    for key_data in room_keys.iter() {
        let room_id = key_data.get("room_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let session_id = key_data.get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let session_data = key_data.get("session_data")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !room_id.is_empty() && !session_id.is_empty() {
            let params = crate::e2ee::backup::BackupKeyUploadParams {
                user_id: auth_user.user_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                session_data: session_data.to_string(),
                version: version.to_string(),
                is_verified: true,
                first_message_index: key_data.get("first_message_index")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                forwarded_count: key_data.get("forwarded_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
            };

            if state.services.backup_service.upload_backup_key(params).await.is_ok() {
                imported_count += 1;
            } else {
                failed_count += 1;
            }
        } else {
            failed_count += 1;
        }
    }

    Ok(Json(serde_json::json!({
        "count": imported_count,
        "failed": failed_count,
        "total": room_keys.len()
    })))
}

/// Import keys by version
/// POST /_matrix/client/r0/room_keys/import/{version}
#[axum::debug_handler]
async fn import_keys_by_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let room_keys = body.get("room_keys")
        .and_then(|v| v.as_array())
        .ok_or_else(|| crate::error::ApiError::bad_request("Missing room_keys".to_string()))?;

    let mut imported_count = 0;
    let mut failed_count = 0;

    for key_data in room_keys.iter() {
        let room_id = key_data.get("room_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let session_id = key_data.get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let session_data = key_data.get("session_data")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !room_id.is_empty() && !session_id.is_empty() {
            let params = crate::e2ee::backup::BackupKeyUploadParams {
                user_id: auth_user.user_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                session_data: session_data.to_string(),
                version: version.clone(),
                is_verified: true,
                first_message_index: key_data.get("first_message_index")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                forwarded_count: key_data.get("forwarded_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
            };

            if state.services.backup_service.upload_backup_key(params).await.is_ok() {
                imported_count += 1;
            } else {
                failed_count += 1;
            }
        } else {
            failed_count += 1;
        }
    }

    Ok(Json(serde_json::json!({
        "count": imported_count,
        "failed": failed_count,
        "total": room_keys.len()
    })))
}
