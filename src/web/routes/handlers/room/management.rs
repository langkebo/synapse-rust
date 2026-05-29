use super::{ensure_room_state_write_access, ensure_room_view_access, UpgradeRoomRequest, UpgradeRoomResponse};
use crate::common::ApiError;
use crate::map_internal;
use crate::services::CreateRoomConfig;
use crate::web::routes::{
    ensure_room_member, extract_token_from_headers, validate_room_id, AppState, AuthenticatedUser,
    OptionalAuthenticatedUser,
};
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct RoomSyncQueryDto {
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    timeout: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_optional_bool")]
    full_state: Option<bool>,
    #[serde(default)]
    since: Option<String>,
}

pub(crate) async fn get_room_info(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let user_id = &auth_user.user_id;

    let membership = sqlx::query(
        r"
        SELECT membership
        FROM room_memberships
        WHERE room_id = $1 AND user_id = $2
        ",
    )
    .bind(&room_id)
    .bind(user_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(map_internal!("Failed to check room membership"))?;

    let membership = match membership {
        Some(row) => row.get::<Option<String>, _>("membership"),
        None => None,
    };

    if membership.is_none() {
        return Err(ApiError::not_found("Room not found or not a member".to_string()));
    }

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(map_internal!("Failed to get room"))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let summary = state.services.room_summary_storage.get_summary(&room_id).await.ok().flatten();

    let invited_members_count = sqlx::query_scalar::<_, i64>(
        r"
        SELECT COUNT(*)
        FROM room_memberships
        WHERE room_id = $1 AND membership = 'invite'
        ",
    )
    .bind(&room_id)
    .fetch_one(&*state.services.room_storage.pool)
    .await
    .unwrap_or(0);

    let guest_can_join = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, "m.room.guest_access")
        .await
        .ok()
        .and_then(|events| {
            events.into_iter().find(|event| event.state_key.as_deref() == Some("")).and_then(|event| {
                event.content.get("guest_access").and_then(|value| value.as_str()).map(|value| value == "can_join")
            })
        })
        .unwrap_or_else(|| summary.as_ref().is_some_and(|value| value.guest_access == "can_join"));

    Ok(Json(json!({
        "room_id": room_id,
        "name": room.name,
        "avatar_url": room.avatar_url,
        "topic": room.topic,
        "canonical_alias": room.canonical_alias,
        "joined_members_count": room.member_count,
        "invited_members_count": invited_members_count,
        "world_readable": room.is_public,
        "guest_can_join": guest_can_join,
        "membership": membership
    })))
}

pub(crate) async fn get_room_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let membership = sqlx::query(
        r"
        SELECT 1
        FROM room_memberships
        WHERE room_id = $1 AND user_id = $2
        LIMIT 1
        ",
    )
    .bind(&room_id)
    .bind(&auth_user.user_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(map_internal!("Failed to check room membership"))?;

    if membership.is_none() {
        return Err(ApiError::not_found("Room not found or not a member".to_string()));
    }

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(map_internal!("Failed to get room"))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    Ok(Json(json!({
        "room_id": room_id,
        "room_version": room.room_version
    })))
}

pub(crate) async fn get_joined_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let rooms = sqlx::query(
        r"
        SELECT DISTINCT room_id
        FROM room_memberships
        WHERE user_id = $1 AND membership = 'join'
        ORDER BY room_id
        ",
    )
    .bind(user_id)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(map_internal!("Failed to get joined rooms"))?;

    let room_ids: Vec<String> = rooms.iter().filter_map(|row| row.get::<Option<String>, _>("room_id")).collect();

    Ok(Json(json!({
        "joined_rooms": room_ids
    })))
}

pub(crate) async fn get_my_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let rooms = sqlx::query(
        r"
        SELECT rm.room_id, rm.membership,
               COALESCE(r.name, '') as name,
               COALESCE(r.avatar_url, '') as avatar_url,
               rm.updated_ts
        FROM room_memberships rm
        LEFT JOIN rooms r ON rm.room_id = r.room_id
        WHERE rm.user_id = $1
        ORDER BY rm.updated_ts DESC
        ",
    )
    .bind(user_id)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(map_internal!("Failed to get rooms"))?;

    let mut room_list = Vec::new();
    for row in rooms.iter() {
        let membership: Option<String> = row.get("membership");
        let room_id: Option<String> = row.get("room_id");
        let name: Option<String> = row.get("name");

        if let (Some(m), Some(r_id)) = (membership, room_id) {
            room_list.push(json!({
                "room_id": r_id,
                "membership": m,
                "name": name.unwrap_or_default(),
                "avatar_url": row.get::<Option<String>, _>("avatar_url").unwrap_or_default()
            }));
        }
    }

    Ok(Json(json!({
        "rooms": room_list,
        "total": room_list.len()
    })))
}

pub(crate) async fn create_private_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    body["preset"] = serde_json::Value::String("private_chat".to_string());
    body["visibility"] = serde_json::Value::String("private".to_string());
    create_room(State(state), headers, Json(body)).await
}

pub(crate) async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.auth_service.validate_token(&token).await?;

    let visibility = body.get("visibility").and_then(|v| v.as_str());
    if let Some(v) = visibility {
        if v != "public" && v != "private" {
            return Err(ApiError::bad_request("Visibility must be 'public' or 'private'".to_string()));
        }
    }

    let room_alias = body.get("room_alias_name").and_then(|v| v.as_str());
    if let Some(alias) = room_alias {
        if alias.len() > 255 {
            return Err(ApiError::bad_request("Room alias name too long".to_string()));
        }
        if !alias.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') {
            return Err(ApiError::bad_request("Invalid characters in room alias name".to_string()));
        }
    }

    let name = body.get("name").and_then(|v| v.as_str());
    if let Some(n) = name {
        if n.len() > 255 {
            return Err(ApiError::bad_request("Room name too long".to_string()));
        }
    }

    let topic = body.get("topic").and_then(|v| v.as_str());
    if let Some(t) = topic {
        if t.len() > 4096 {
            return Err(ApiError::bad_request("Room topic too long".to_string()));
        }
    }

    let invite = body
        .get("invite")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<String>>());

    if let Some(ref inv) = invite {
        if inv.len() > 100 {
            return Err(ApiError::bad_request("Too many invites (max 100)".to_string()));
        }
    }

    let preset = body.get("preset").and_then(|v| v.as_str());

    let room_type = body
        .get("room_type")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("creation_content").and_then(|cc| cc.get("type")).and_then(|v| v.as_str()));

    let is_direct = body.get("is_direct").and_then(|v| v.as_bool());
    let room_version = body.get("room_version").and_then(|v| v.as_str()).map(str::to_owned);
    let mut creation_content = body.get("creation_content").cloned();
    if let Some(map) = creation_content.as_mut().and_then(|value| value.as_object_mut()) {
        map.remove("creator");
        map.remove("room_version");
        map.remove("predecessor");
    }
    let initial_state = body.get("initial_state").and_then(|v| v.as_array()).cloned();
    let power_level_content_override = body.get("power_level_content_override").cloned();

    let config = CreateRoomConfig {
        visibility: visibility.map(|s| s.to_string()),
        room_alias_name: room_alias.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
        topic: topic.map(|s| s.to_string()),
        invite_list: invite,
        preset: preset.map(|s| s.to_string()),
        room_type: room_type.map(|s| s.to_string()),
        is_direct,
        room_version,
        creation_content,
        initial_state,
        power_level_content_override,
        ..Default::default()
    };

    let result = state.services.room_service.create_room(&user_id, config.clone()).await?;

    if config.room_type.as_deref() == Some("m.space") {
        let space_request = crate::storage::space::CreateSpaceRequest {
            room_id: result.get("room_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            name: config.name.clone(),
            topic: config.topic.clone(),
            avatar_url: None,
            creator: user_id.to_string(),
            join_rule: config.preset.clone(),
            visibility: config.visibility.clone(),
            is_public: config.visibility.as_ref().map(|v| v == "public"),
            parent_space_id: None,
        };
        if let Err(e) = state.services.space_service.create_space(space_request).await {
            ::tracing::error!("Failed to create space record: {}", e);
        }
    }

    Ok(Json(result))
}

#[axum::debug_handler]
pub(crate) async fn get_room_visibility(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = state
        .services
        .room_service
        .get_room_visibility(&room_id)
        .await
        .map_err(map_internal!("Failed to get room visibility"))?;

    Ok(Json(json!({
        "visibility": visibility
    })))
}

#[axum::debug_handler]
pub(crate) async fn set_room_visibility(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = body
        .get("visibility")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing visibility field".to_string()))?;

    if visibility != "public" && visibility != "private" {
        return Err(ApiError::bad_request("visibility must be 'public' or 'private'".to_string()));
    }

    ensure_room_member(&state, &auth_user, &room_id, "You must be a member of this room to update room visibility")
        .await?;

    let is_creator = state
        .services
        .room_service
        .is_room_creator(&room_id, &auth_user.user_id)
        .await
        .map_err(map_internal!("Failed to check room creator"))?;

    if !is_creator {
        return Err(ApiError::forbidden("Only the room creator can update room visibility".to_string()));
    }

    let is_public = visibility == "public";

    state.services.room_service.set_room_directory(&room_id, is_public).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "visibility": visibility,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn get_user_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let rooms = state.services.room_service.get_joined_rooms(&user_id).await?;

    Ok(Json(json!({
        "joined_rooms": rooms
    })))
}

pub(crate) async fn upgrade_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<UpgradeRoomRequest>,
) -> Result<Json<UpgradeRoomResponse>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_state_write_access(&state, &auth_user, &room_id, "m.room.tombstone").await?;

    let new_room_id = state.services.room_service.upgrade_room(&room_id, &body.new_version, &auth_user.user_id).await?;

    Ok(Json(UpgradeRoomResponse { replacement_room: new_room_id }))
}

pub(crate) async fn forget_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    state.services.room_service.forget_room(&room_id, &auth_user.user_id).await?;
    Ok(Json(json!({
        "room_id": room_id,
        "is_forgotten": true,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn room_initial_sync(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(map_internal!("Database error"))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let from = params
        .get("from")
        .and_then(|value| crate::common::parse_stream_token(value).or_else(|| value.parse::<i64>().ok()))
        .unwrap_or(0);
    let limit = params.get("limit").and_then(|value| value.parse::<i64>().ok()).unwrap_or(10).clamp(1, 100);

    let state_events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(map_internal!("Failed to get room state"))?
        .into_iter()
        .map(|event| {
            json!({
                "event_id": event.event_id,
                "room_id": event.room_id,
                "sender": event.sender,
                "type": event.event_type,
                "state_key": event.state_key.unwrap_or_default(),
                "content": event.content,
                "origin_server_ts": event.origin_server_ts,
                "unsigned": event.unsigned.unwrap_or_else(|| json!({}))
            })
        })
        .collect::<Vec<Value>>();

    let members = state.services.room_service.get_room_members(&room_id, &auth_user.user_id).await?;
    let messages =
        state.services.room_service.get_room_messages(&room_id, &auth_user.user_id, from, limit, "b").await?;

    let member_events = members.get("chunk").and_then(Value::as_array).cloned().unwrap_or_default();
    let visibility = if room.is_public { "public" } else { "private" };

    Ok(Json(json!({
        "room_id": room.room_id,
        "membership": "join",
        "visibility": visibility,
        "messages": messages,
        "pagination_chunk": messages.get("chunk").cloned().unwrap_or_else(|| json!([])),
        "state": state_events,
        "members": member_events,
        "presence": [],
        "receipts": {},
        "account_data": [],
        "name": room.name,
        "topic": room.topic,
        "canonical_alias": room.canonical_alias,
        "join_rule": room.join_rule
    })))
}

pub(crate) async fn get_room_capabilities(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(map_internal!("Failed to get room"))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_encrypted = sqlx::query(
        r"
        SELECT 1
        FROM events
        WHERE room_id = $1 AND event_type = 'm.room.encryption' AND state_key IS NOT NULL
        LIMIT 1
        ",
    )
    .bind(&room_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(map_internal!("Failed to check room encryption"))?
    .is_some();

    let encryption_content = sqlx::query(
        r"
        SELECT content
        FROM events
        WHERE room_id = $1 AND event_type = 'm.room.encryption' AND state_key IS NOT NULL
        ORDER BY origin_server_ts DESC
        LIMIT 1
        ",
    )
    .bind(&room_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(map_internal!("Failed to get encryption event content"))?;

    let encryption_status = crate::storage::room::RoomEncryptionStatus::from_encryption_event(
        is_encrypted,
        if is_encrypted {
            encryption_content
                .as_ref()
                .and_then(|row| {
                    use sqlx::Row;
                    let content: serde_json::Value = row.get("content");
                    content.get("algorithm").and_then(|v| v.as_str()).map(|s| s.to_string())
                })
                .or_else(|| room.encryption.clone())
        } else {
            None
        },
        encryption_content.as_ref().and_then(|row| {
            use sqlx::Row;
            let content: serde_json::Value = row.get("content");
            content.get("rotation_period_ms").and_then(|v| v.as_i64())
        }),
        encryption_content.as_ref().and_then(|row| {
            use sqlx::Row;
            let content: serde_json::Value = row.get("content");
            content.get("rotation_period_msgs").and_then(|v| v.as_i64())
        }),
    );

    let join_rule = if room.is_public { "public" } else { "invite" };

    Ok(Json(json!({
        "room_id": room_id,
        "room_version": room.room_version,
        "capabilities": {
            "knock": false,
            "restricted": false,
            "threading": true,
            "read_receipts": true,
            "typing_notifications": true
        },
        "features": {
            "encryption": is_encrypted,
            "federation": true,
            "guest_access": false
        },
        "encryption_status": encryption_status,
        "join_rule": join_rule
    })))
}

pub(crate) async fn get_room_sync(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<RoomSyncQueryDto>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let timeout = params.timeout.unwrap_or(30000);
    let full_state = params.full_state.unwrap_or(false);
    let since = params.since.as_deref();

    let result = state
        .services
        .sync_service
        .room_sync_with_timeout(&auth_user.user_id, &room_id, timeout, full_state, since)
        .await?;

    Ok(Json(result))
}

fn parse_u64_query_value(raw: &str) -> Option<u64> {
    raw.parse::<u64>().ok()
}

fn parse_bool_query_value(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.as_deref().and_then(parse_u64_query_value))
}

fn deserialize_optional_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.as_deref().and_then(parse_bool_query_value))
}

#[cfg(test)]
mod tests {
    use super::{parse_bool_query_value, parse_u64_query_value};

    #[test]
    fn test_room_sync_parse_u64_query_value_accepts_decimal_string() {
        assert_eq!(parse_u64_query_value("30000"), Some(30000));
        assert_eq!(parse_u64_query_value("0"), Some(0));
    }

    #[test]
    fn test_room_sync_parse_bool_query_value_accepts_legacy_forms() {
        assert_eq!(parse_bool_query_value("true"), Some(true));
        assert_eq!(parse_bool_query_value("1"), Some(true));
        assert_eq!(parse_bool_query_value("false"), Some(false));
        assert_eq!(parse_bool_query_value("0"), Some(false));
    }
}

pub(crate) async fn get_room_thread_by_id(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, thread_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    crate::web::routes::validate_event_id(&thread_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let request = crate::services::thread_service::GetThreadRequest {
        room_id: room_id.clone(),
        thread_id: thread_id.clone(),
        include_replies: true,
        reply_limit: Some(100),
    };

    let thread_detail = state.services.thread_service.get_thread(request, Some(&auth_user.user_id)).await?;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "thread_id": thread_id,
        "root": thread_detail.root,
        "replies": thread_detail.replies,
        "reply_count": thread_detail.replies.len(),
        "participants": thread_detail.participants,
        "summary": thread_detail.summary,
        "user_receipt": thread_detail.user_receipt,
        "user_subscription": thread_detail.user_subscription
    })))
}

pub(crate) async fn set_room_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, data_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r"
        INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $5, $5)
        ON CONFLICT (user_id, room_id, data_type)
        DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
        ",
    )
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind(&data_type)
    .bind(&body)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(map_internal!("Database error"))?;

    Ok(Json(json!({
        "room_id": room_id,
        "data_type": data_type,
        "updated_ts": now
    })))
}

pub(crate) async fn get_room_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, data_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let result =
        sqlx::query("SELECT data FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3")
            .bind(&auth_user.user_id)
            .bind(&room_id)
            .bind(&data_type)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(map_internal!("Database error"))?;

    match result {
        Some(row) => Ok(Json(row.get::<Option<Value>, _>("data").unwrap_or_else(|| json!({})))),
        None => Err(ApiError::not_found("Room account data not found".to_string())),
    }
}

pub(crate) async fn get_room_turn_server(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let voip_service = &state.services.rtc_domain_service.infra;

    if !voip_service.is_enabled() {
        return Ok(Json(json!({
            "uris": [],
            "username": "",
            "password": "",
            "ttl": 0
        })));
    }

    let creds = voip_service.generate_turn_credentials(&auth_user.user_id)?;

    Ok(Json(json!({
        "uris": creds.uris,
        "username": creds.username,
        "password": creds.password,
        "ttl": creds.ttl
    })))
}

pub(crate) async fn get_room_metadata(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(map_internal!("Failed to get room"))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let mut response = json!({
        "room_id": room.room_id,
        "name": room.name,
        "topic": room.topic,
        "avatar_url": room.avatar_url,
        "canonical_alias": room.canonical_alias,
        "join_rule": room.join_rule,
        "history_visibility": room.history_visibility,
        "creator": room.creator_user_id,
        "room_version": room.room_version,
        "encryption": room.encryption,
        "is_public": room.is_public,
        "member_count": room.member_count,
        "created_ts": room.created_ts
    });

    if let Some(obj) = response.as_object_mut() {
        if obj.get("name").is_some_and(|v| v.is_null()) {
            obj.remove("name");
        }
        if obj.get("topic").is_some_and(|v| v.is_null()) {
            obj.remove("topic");
        }
        if obj.get("avatar_url").is_some_and(|v| v.is_null()) {
            obj.remove("avatar_url");
        }
        if obj.get("canonical_alias").is_some_and(|v| v.is_null()) {
            obj.remove("canonical_alias");
        }
        if obj.get("creator").is_some_and(|v| v.is_null()) {
            obj.remove("creator");
        }
        if obj.get("encryption").is_some_and(|v| v.is_null()) {
            obj.remove("encryption");
        }
    }

    Ok(Json(response))
}

pub(crate) async fn get_room_vault_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let result = sqlx::query(
        "SELECT data, updated_ts FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3",
    )
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind("m.room.vault_data")
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(map_internal!("Database error"))?;

    match result {
        Some(row) => Ok(Json(json!({
            "room_id": room_id,
            "vault_data": row
                .get::<Option<Value>, _>("data")
                .unwrap_or_else(|| json!({})),
            "updated_ts": row.get::<Option<i64>, _>("updated_ts")
        }))),
        None => Ok(Json(json!({
            "room_id": room_id,
            "vault_data": {},
            "updated_ts": Value::Null
        }))),
    }
}

pub(crate) async fn set_room_vault_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r"
        INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $5, $5)
        ON CONFLICT (user_id, room_id, data_type)
        DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
        ",
    )
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind("m.room.vault_data")
    .bind(&body)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(map_internal!("Database error"))?;

    Ok(Json(json!({
        "room_id": room_id,
        "vault_data": body,
        "updated_ts": now
    })))
}

pub(crate) async fn get_room_rendered(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(map_internal!("Failed to get room"))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, 20)
        .await
        .map_err(map_internal!("Failed to get room events"))?;

    let lines: Vec<Value> = events
        .iter()
        .rev()
        .map(|event| {
            let body = event.content.get("body").and_then(|value| value.as_str()).unwrap_or("");
            json!({
                "event_id": event.event_id,
                "sender": event.user_id,
                "type": event.event_type,
                "text": body,
                "origin_server_ts": event.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "room_id": room_id,
        "rendered": {
            "title": room.name.unwrap_or_else(|| room.room_id.clone()),
            "topic": room.topic,
            "canonical_alias": room.canonical_alias,
            "member_count": room.member_count,
            "lines": lines
        }
    })))
}

pub(crate) async fn get_room_external_ids(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let aliases = state
        .services
        .room_storage
        .get_room_aliases(&room_id)
        .await
        .map_err(map_internal!("Failed to get room aliases"))?;

    let external_ids: Vec<Value> =
        aliases.into_iter().map(|alias| json!({"type": "room_alias", "value": alias})).collect();

    Ok(Json(json!({
        "room_id": room_id,
        "external_ids": external_ids
    })))
}

pub(crate) async fn get_room_service_types(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(map_internal!("Failed to get room"))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let mut service_types = vec!["messaging".to_string()];
    if room.encryption.is_some() {
        service_types.push("encryption".to_string());
    }
    if room.is_public {
        service_types.push("directory".to_string());
    }

    Ok(Json(json!({
        "room_id": room_id,
        "service_types": service_types
    })))
}

pub(crate) async fn get_room_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, device_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let device = state
        .services
        .device_storage
        .get_device(&device_id)
        .await
        .map_err(map_internal!("Failed to get device"))?
        .ok_or_else(|| ApiError::not_found("Device not found".to_string()))?;

    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &device.user_id)
        .await
        .map_err(map_internal!("Failed to verify device membership"))?;

    if !is_member {
        return Err(ApiError::not_found("Device not found in this room".to_string()));
    }

    Ok(Json(json!({
        "room_id": room_id,
        "device": {
            "device_id": device.device_id,
            "user_id": device.user_id,
            "display_name": device.display_name,
            "last_seen_ts": device.last_seen_ts,
            "last_seen_ip": device.last_seen_ip,
            "created_ts": device.created_ts
        }
    })))
}

pub(crate) async fn get_room_resolve(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(map_internal!("Failed to get room"))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let aliases = state
        .services
        .room_storage
        .get_room_aliases(&room_id)
        .await
        .map_err(map_internal!("Failed to get room aliases"))?;

    Ok(Json(json!({
        "room_id": room_id,
        "aliases": aliases,
        "name": room.name,
        "canonical_alias": room.canonical_alias
    })))
}

pub(crate) async fn get_room_spaces(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let rooms = if let Some(space) = state.services.space_service.get_space_by_room(&room_id).await? {
        let children = state.services.space_service.get_space_children(&space.space_id).await?;

        children
            .into_iter()
            .map(|child| {
                json!({
                    "room_id": child.room_id,
                    "via": child.via_servers,
                    "suggested": child.is_suggested,
                    "order": child.order
                })
            })
            .collect::<Vec<Value>>()
    } else {
        Vec::new()
    };

    let spaces = state
        .services
        .space_service
        .get_parent_spaces(&room_id)
        .await?
        .into_iter()
        .map(|space| {
            json!({
                "room_id": space.room_id,
                "name": space.name,
                "topic": space.topic,
                "avatar_url": space.avatar_url,
                "join_rule": space.join_rule,
                "world_readable": space.is_public,
                "guest_can_join": space.is_public,
                "room_type": space.room_type
            })
        })
        .collect::<Vec<Value>>();

    Ok(Json(json!({
        "rooms": rooms,
        "spaces": spaces
    })))
}

pub(crate) async fn search_room_messages(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let search_term = body
        .get("search_term")
        .and_then(Value::as_str)
        .or_else(|| {
            body.get("search_categories")
                .and_then(|value| value.get("room_events"))
                .and_then(|value| value.get("search_term"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            body.get("search_categories")
                .and_then(|value| value.get("room_events"))
                .and_then(|value| value.get("search"))
                .and_then(|value| value.get("term"))
                .and_then(Value::as_str)
        })
        .or_else(|| body.get("search").and_then(|value| value.get("term")).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("Search term cannot be empty"))?;

    let limit = body
        .get("limit")
        .and_then(Value::as_u64)
        .or_else(|| {
            body.get("search_categories")
                .and_then(|value| value.get("room_events"))
                .and_then(|value| value.get("filter"))
                .and_then(|value| value.get("limit"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(10)
        .min(100) as i64;

    let search_pattern = format!("%{}%", search_term.to_lowercase());
    let rows = sqlx::query(
        r"
        SELECT event_id, room_id, sender, event_type, content, origin_server_ts
        FROM events
        WHERE room_id = $1
          AND event_type = 'm.room.message'
          AND LOWER(content::text) LIKE $2
        ORDER BY origin_server_ts DESC
        LIMIT $3
        ",
    )
    .bind(&room_id)
    .bind(&search_pattern)
    .bind(limit)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(map_internal!("Database error"))?;

    let results: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            json!({
                "rank": 1.0,
                "result": {
                    "event_id": row.get::<Option<String>, _>("event_id"),
                    "room_id": row.get::<Option<String>, _>("room_id"),
                    "sender": row.get::<Option<String>, _>("sender"),
                    "type": row.get::<Option<String>, _>("event_type"),
                    "content": row.get::<Option<Value>, _>("content").unwrap_or(Value::Null),
                    "origin_server_ts": row.get::<Option<i64>, _>("origin_server_ts").unwrap_or(0)
                }
            })
        })
        .collect();

    Ok(Json(json!({
        "search_categories": {
            "room_events": {
                "count": results.len(),
                "results": results,
                "highlights": [search_term]
            }
        }
    })))
}

pub(crate) async fn get_retention_policy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room_policy = state
        .services
        .retention_storage
        .get_room_policy(&room_id)
        .await
        .map_err(map_internal!("Failed to get retention policy"))?;

    let server_policy = state.services.retention_storage.get_server_policy().await.ok();

    match room_policy {
        Some(policy) => Ok(Json(serde_json::json!({
            "room_id": room_id,
            "max_lifetime": policy.max_lifetime,
            "min_lifetime": policy.min_lifetime,
            "expire_on_clients": policy.expire_on_clients,
            "is_server_default": policy.is_server_default,
            "created_ts": policy.created_ts,
            "updated_ts": policy.updated_ts
        }))),
        None => {
            let default = server_policy.unwrap_or(crate::storage::retention::ServerRetentionPolicy {
                id: 0,
                max_lifetime: None,
                min_lifetime: 0,
                expire_on_clients: false,
                created_ts: 0,
                updated_ts: 0,
            });
            Ok(Json(serde_json::json!({
                "room_id": room_id,
                "max_lifetime": default.max_lifetime,
                "min_lifetime": default.min_lifetime,
                "expire_on_clients": default.expire_on_clients,
                "is_server_default": true,
                "created_ts": default.created_ts,
                "updated_ts": default.updated_ts
            })))
        }
    }
}
