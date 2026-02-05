use super::{AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::Value;

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
}

#[axum::debug_handler]
async fn create_backup_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let algorithm = body["algorithm"].as_str().unwrap_or("m.megolm.v1.aes-sha2");
    let auth_data = body.get("auth_data").cloned();

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
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let auth_data = body.get("auth_data").cloned();

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
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let room_id = body["room_id"].as_str().unwrap_or("");

    state
        .services
        .backup_service
        .upload_room_keys_for_room(
            &auth_user.user_id,
            room_id,
            &version,
            body["sessions"].as_array().cloned().unwrap_or_default(),
        )
        .await?;

    Ok(Json(serde_json::json!({
        "count": body["sessions"].as_array().map(|s| s.len() as i64).unwrap_or(0),
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
