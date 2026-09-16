use crate::routes::context::E2eeRoomContext;
use crate::routes::extractors::RoomId;
use crate::routes::{validate_room_id, AuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};
use std::collections::HashSet;
use synapse_common::ApiError;
use synapse_e2ee::backup::models::BackupKeyInfo;

async fn latest_room_key_backup_version(ctx: &E2eeRoomContext, user_id: &str) -> Result<Option<String>, ApiError> {
    let backups = ctx.e2ee_backup_service.get_all_backups(user_id).await?;

    Ok(backups.into_iter().max_by_key(|backup| backup.version).map(|backup| backup.version.to_string()))
}

async fn ensure_room_key_backup_version(ctx: &E2eeRoomContext, user_id: &str) -> Result<String, ApiError> {
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

/// See [`get_room_keys`].
pub(crate) async fn get_room_keys(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
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

/// See [`get_room_key_count`].
pub(crate) async fn get_room_key_count(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
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

/// See [`claim_room_keys`].
pub(crate) async fn claim_room_keys(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
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
            room_id.to_string(): one_time_keys
        }
    })))
}

/// See [`get_room_keys_version`].
pub(crate) async fn get_room_keys_version(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let version = latest_room_key_backup_version(&ctx, &auth_user.user_id).await?.unwrap_or_else(|| "0".to_string());

    Ok(Json(json!({
        "version": version
    })))
}

/// See [`forward_room_keys`].
pub(crate) async fn forward_room_keys(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key() -> BackupKeyInfo {
        BackupKeyInfo {
            user_id: "@alice:test".to_string(),
            backup_id: "backup1".to_string(),
            room_id: "!room:test".to_string(),
            session_id: "session1".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: json!({"ciphertext": "abc"}),
        }
    }

    #[test]
    fn room_key_to_json_includes_all_fields() {
        let json = room_key_to_json(&make_key());
        assert_eq!(json["session_id"], "session1");
        assert_eq!(json["first_message_index"], 0);
        assert_eq!(json["forwarded_count"], 0);
        assert_eq!(json["is_verified"], true);
        assert_eq!(json["session_data"]["ciphertext"], "abc");
    }

    #[test]
    fn extract_forwarded_sessions_array_returns_as_is() {
        let value = json!({"sessions": [{"session_id": "a", "session_data": {}}, {"session_id": "b"}]});
        let sessions = extract_forwarded_sessions(&value);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0]["session_id"], "a");
        assert_eq!(sessions[1]["session_id"], "b");
    }

    #[test]
    fn extract_forwarded_sessions_object_normalizes_session_id() {
        let value = json!({"sessions": {"s1": {"ciphertext": "x"}}});
        let sessions = extract_forwarded_sessions(&value);
        assert_eq!(sessions.len(), 1);
        // Object 形态：key 是 session_id，无 session_id 字段时补齐。
        assert_eq!(sessions[0]["session_id"], "s1");
        assert!(sessions[0]["session_data"].is_object());
    }

    #[test]
    fn extract_forwarded_sessions_missing_returns_empty() {
        assert!(extract_forwarded_sessions(&json!({})).is_empty());
        assert!(extract_forwarded_sessions(&json!({"sessions": "not_array_or_object"})).is_empty());
    }

    #[test]
    fn normalize_forwarded_room_keys_uses_room_specific() {
        let body = json!({
            "rooms": {"!room:test": {"sessions": [{"session_id": "a"}]}},
            "sessions": [{"session_id": "fallback"}]
        });
        let keys = normalize_forwarded_room_keys(&body, "!room:test");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0]["session_id"], "a");
    }

    #[test]
    fn normalize_forwarded_room_keys_falls_back_to_body_sessions() {
        let body = json!({"sessions": [{"session_id": "fallback"}]});
        let keys = normalize_forwarded_room_keys(&body, "!room:test");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0]["session_id"], "fallback");
    }

    #[test]
    fn normalize_forwarded_room_keys_empty_body_returns_empty() {
        assert!(normalize_forwarded_room_keys(&json!({}), "!room:test").is_empty());
    }

    #[test]
    fn requested_room_key_session_ids_from_top_level_array() {
        let body = json!({"session_ids": ["s1", "s2"]});
        let ids = requested_room_key_session_ids(&body, "!room:test").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("s1"));
        assert!(ids.contains("s2"));
    }

    #[test]
    fn requested_room_key_session_ids_from_room_sessions() {
        let body = json!({"rooms": {"!room:test": {"sessions": ["s3"]}}});
        let ids = requested_room_key_session_ids(&body, "!room:test").unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.contains("s3"));
    }

    #[test]
    fn requested_room_key_session_ids_empty_returns_none() {
        assert!(requested_room_key_session_ids(&json!({}), "!room:test").is_none());
        assert!(requested_room_key_session_ids(&json!({"session_ids": []}), "!room:test").is_none());
    }

    #[test]
    fn requested_room_key_session_ids_object_uses_keys() {
        let body = json!({"sessions": {"s4": {}, "s5": {}}});
        let ids = requested_room_key_session_ids(&body, "!room:test").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("s4"));
        assert!(ids.contains("s5"));
    }
}
