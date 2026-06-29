use crate::common::ApiError;
use crate::e2ee::backup::models::BackupKeyInfo;
use crate::web::routes::context::RoomContext;
use crate::web::routes::{validate_room_id, AuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};
use std::collections::HashSet;

async fn latest_room_key_backup_version(ctx: &RoomContext, user_id: &str) -> Result<Option<String>, ApiError> {
    let backups = ctx.e2ee_backup_service.get_all_backups(user_id).await?;

    Ok(backups.into_iter().max_by_key(|backup| backup.version).map(|backup| backup.version.to_string()))
}

async fn ensure_room_key_backup_version(ctx: &RoomContext, user_id: &str) -> Result<String, ApiError> {
    if let Some(version) = latest_room_key_backup_version(ctx, user_id).await? {
        return Ok(version);
    }

    ctx.e2ee_backup_service.create_backup(user_id, "m.megolm.v1.aes-sha2", Some(json!({}))).await
}

fn room_key_to_json(key: &BackupKeyInfo) -> Value {
    json!({
        "session_id": key.session_id,
        "first_message_index": key.first_message_index,
        "forwarded_count": key.forwarded_count,
        "is_verified": key.is_verified,
        "session_data": key.session_data
    })
}

fn normalize_forwarded_room_keys(body: &Value, room_id: &str) -> Vec<Value> {
    let mut keys = Vec::new();

    if let Some(room_value) = body.get("rooms").and_then(|rooms| rooms.get(room_id)) {
        keys.extend(extract_forwarded_sessions(room_value));
    }

    if keys.is_empty() {
        keys.extend(extract_forwarded_sessions(body));
    }

    keys
}

fn extract_forwarded_sessions(value: &Value) -> Vec<Value> {
    let Some(sessions) = value.get("sessions") else {
        return Vec::new();
    };

    match sessions {
        Value::Array(items) => items.clone(),
        Value::Object(items) => items
            .iter()
            .map(|(session_id, session_value)| {
                if session_value.get("session_id").is_some() {
                    session_value.clone()
                } else {
                    let mut normalized = session_value.clone();
                    if let Some(map) = normalized.as_object_mut() {
                        map.insert("session_id".to_string(), Value::String(session_id.clone()));
                        if !map.contains_key("session_data") {
                            map.insert("session_data".to_string(), session_value.clone());
                        }
                    }
                    normalized
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn requested_room_key_session_ids(body: &Value, room_id: &str) -> Option<HashSet<String>> {
    let mut session_ids = HashSet::new();

    if let Some(Value::Array(values)) = body.get("session_ids") {
        for value in values {
            if let Some(session_id) = value.as_str() {
                session_ids.insert(session_id.to_string());
            }
        }
    }

    let sessions_source = body
        .get("rooms")
        .and_then(|rooms| rooms.get(room_id))
        .and_then(|room| room.get("sessions"))
        .or_else(|| body.get("sessions"));

    if let Some(sessions) = sessions_source {
        match sessions {
            Value::Array(values) => {
                for value in values {
                    if let Some(session_id) = value.as_str() {
                        session_ids.insert(session_id.to_string());
                    } else if let Some(session_id) = value.get("session_id").and_then(Value::as_str) {
                        session_ids.insert(session_id.to_string());
                    }
                }
            }
            Value::Object(values) => {
                session_ids.extend(values.keys().cloned());
            }
            _ => {}
        }
    }

    if session_ids.is_empty() {
        None
    } else {
        Some(session_ids)
    }
}

pub(crate) async fn get_room_keys(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let version = latest_room_key_backup_version(&ctx, &auth_user.user_id).await?;
    let keys = if let Some(version) = version.clone() {
        ctx.e2ee_backup_service.get_room_backup_keys(&auth_user.user_id, &room_id, &version).await?
    } else {
        Vec::new()
    };

    Ok(Json(json!({
        "room_id": room_id,
        "version": version.unwrap_or_else(|| "0".to_string()),
        "keys": keys.iter().map(room_key_to_json).collect::<Vec<_>>()
    })))
}

pub(crate) async fn get_room_key_count(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let version = latest_room_key_backup_version(&ctx, &auth_user.user_id).await?;
    let count = if let Some(version) = version {
        ctx.e2ee_backup_service.get_room_backup_keys(&auth_user.user_id, &room_id, &version).await?.len()
    } else {
        0
    };

    Ok(Json(json!({
        "count": count
    })))
}

pub(crate) async fn claim_room_keys(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let version = latest_room_key_backup_version(&ctx, &auth_user.user_id).await?;
    let requested_sessions = requested_room_key_session_ids(&body, &room_id);
    let keys = if let Some(version) = version {
        ctx.e2ee_backup_service.get_room_backup_keys(&auth_user.user_id, &room_id, &version).await?
    } else {
        Vec::new()
    };

    let one_time_keys = keys
        .into_iter()
        .filter(|key| requested_sessions.as_ref().is_none_or(|session_ids| session_ids.contains(&key.session_id)))
        .map(|key| (key.session_id.clone(), room_key_to_json(&key)))
        .collect::<serde_json::Map<_, _>>();

    Ok(Json(json!({
        "failures": {},
        "one_time_keys": {
            room_id: one_time_keys
        }
    })))
}

pub(crate) async fn get_room_keys_version(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let version = latest_room_key_backup_version(&ctx, &auth_user.user_id).await?.unwrap_or_else(|| "0".to_string());

    Ok(Json(json!({
        "version": version
    })))
}

pub(crate) async fn forward_room_keys(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let keys = normalize_forwarded_room_keys(&body, &room_id);
    let version = ensure_room_key_backup_version(&ctx, &auth_user.user_id).await?;

    if !keys.is_empty() {
        ctx.e2ee_backup_service.upload_room_keys_for_room(&auth_user.user_id, &room_id, &version, keys.clone()).await?;
    }

    Ok(Json(json!({
        "count": keys.len(),
        "etag": version,
        "version": version
    })))
}
