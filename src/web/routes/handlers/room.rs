use crate::common::{parse_stream_token, ApiError};
use crate::e2ee::backup::models::BackupKeyInfo;
use crate::services::CreateRoomConfig;
use crate::storage::event::{EventStorage, RoomEvent};
use crate::storage::room::{Receipt, RoomStorage};
use crate::storage::CreateEventParams;
use crate::web::routes::{
    ensure_room_member, extract_token_from_headers, is_joined_room_member,
    is_joined_room_member_or_creator, validate_event_id, validate_receipt_type, validate_room_id,
    validate_user_id, AppState, AuthenticatedUser, OptionalAuthenticatedUser,
};
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{types::JsonValue, Row};
use std::collections::{HashMap, HashSet};

fn parse_room_messages_from_token(params: &Value) -> i64 {
    params
        .get("from")
        .and_then(|v| v.as_str())
        .and_then(|token| parse_stream_token(token).or_else(|| token.parse().ok()))
        .unwrap_or(0)
}

async fn ensure_room_view_access(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member(
        state,
        auth_user,
        room_id,
        "You must be a member of this room to view events",
    )
    .await?;

    Ok(())
}

fn normalize_room_event_type(event_type: &str) -> String {
    if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.to_string()
    } else {
        format!("m.room.{}", event_type)
    }
}

async fn ensure_room_state_write_access(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    event_type: &str,
) -> Result<(), ApiError> {
    ensure_room_member(
        state,
        auth_user,
        room_id,
        "You must be a member of this room to send state events",
    )
    .await?;

    state
        .services
        .auth_service
        .verify_state_event_write(room_id, &auth_user.user_id, event_type)
        .await?;

    Ok(())
}

async fn get_room_event(
    event_storage: &EventStorage,
    room_id: &str,
    event_id: &str,
) -> Result<RoomEvent, ApiError> {
    let event = event_storage
        .get_event(event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::not_found(
            "Event not found in this room".to_string(),
        ));
    }

    Ok(event)
}

pub(crate) async fn get_single_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::not_found(
            "Event not found in this room".to_string(),
        ));
    }

    Ok(Json(json!({
        "event_id": event.event_id,
        "room_id": event.room_id,
        "sender": event.user_id,
        "type": event.event_type,
        "content": event.content,
        "origin_server_ts": event.origin_server_ts,
        "state_key": event.state_key
    })))
}

pub(crate) async fn get_event_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let room_id = room_id.replace("%21", "!").replace("%3A", ":");
    let event_id = event_id.replace("%24", "$");

    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::not_found(
            "Event not found in this room".to_string(),
        ));
    }

    Ok(Json(json!({
        "event_id": event.event_id,
        "room_id": event.room_id,
        "keys": []
    })))
}

pub(crate) async fn get_room_thread(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let room_id = room_id.replace("%21", "!").replace("%3A", ":");
    let event_id = event_id.replace("%24", "$");

    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let root_event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if root_event.room_id != room_id {
        return Err(ApiError::not_found(
            "Event not found in this room".to_string(),
        ));
    }

    let mut replies_json = Vec::new();
    let mut reply_count = 0;
    let mut participants_json = Vec::new();

    if let Some(thread_root) = state
        .services
        .thread_storage
        .get_thread_root_by_event(&room_id, &event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get thread root: {}", e)))?
    {
        let thread_id = thread_root.thread_id.clone().unwrap_or_default();
        if !thread_id.is_empty() {
            let replies = state
                .services
                .thread_storage
                .get_thread_replies(&room_id, &thread_id, Some(50), None)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get thread replies: {}", e)))?;
            reply_count = state
                .services
                .thread_storage
                .get_reply_count(&room_id, &thread_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get reply count: {}", e)))?;
            participants_json = state
                .services
                .thread_storage
                .get_thread_participants(&room_id, &thread_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get participants: {}", e)))?;

            replies_json = replies
                .into_iter()
                .map(|reply| {
                    json!({
                        "event_id": reply.event_id,
                        "thread_id": reply.thread_id,
                        "room_id": reply.room_id,
                        "sender": reply.sender,
                        "content": reply.content,
                        "origin_server_ts": reply.origin_server_ts,
                        "in_reply_to_event_id": reply.in_reply_to_event_id,
                        "is_edited": reply.is_edited,
                        "is_redacted": reply.is_redacted
                    })
                })
                .collect();
        }
    }

    Ok(Json(json!({
        "root": {
            "event_id": root_event.event_id,
            "room_id": root_event.room_id,
            "sender": root_event.user_id,
            "type": root_event.event_type,
            "content": root_event.content,
            "origin_server_ts": root_event.origin_server_ts,
            "state_key": root_event.state_key
        },
        "replies": replies_json,
        "reply_count": reply_count,
        "participants": participants_json
    })))
}

pub(crate) async fn get_room_notifications(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let _from = params.get("from").cloned();

    let notifications = sqlx::query(
        r#"
        SELECT event_id, room_id, ts, notification_type, is_read
        FROM notifications
        WHERE user_id = $1 AND room_id = $2
        ORDER BY ts DESC
        LIMIT $3
        "#,
    )
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind(limit as i64)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let notifications_list: Vec<Value> = notifications
        .iter()
        .map(|row| {
            let event_id = row.get::<Option<String>, _>("event_id").unwrap_or_default();
            json!({
                "event_id": event_id,
                "room_id": row.get::<Option<String>, _>("room_id"),
                "ts": row.get::<Option<i64>, _>("ts"),
                "profile_tag": row.get::<Option<String>, _>("notification_type"),
                "read": row.get::<Option<bool>, _>("is_read").unwrap_or(false),
                "room_name": None::<Value>,
                "sender": None::<Value>,
                "prio": "high",
                "client_action": "notify"
            })
        })
        .collect();

    Ok(Json(json!({
        "notifications": notifications_list,
        "next_token": None::<String>
    })))
}

pub(crate) async fn get_room_info(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let user_id = &auth_user.user_id;

    let membership = sqlx::query(
        r#"
        SELECT membership
        FROM room_memberships
        WHERE room_id = $1 AND user_id = $2
        "#,
    )
    .bind(&room_id)
    .bind(user_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to check room membership: {}", e)))?;

    let membership = match membership {
        Some(row) => row.get::<Option<String>, _>("membership"),
        None => None,
    };

    if membership.is_none() {
        return Err(ApiError::not_found(
            "Room not found or not a member".to_string(),
        ));
    }

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let summary = state
        .services
        .room_summary_storage
        .get_summary(&room_id)
        .await
        .ok()
        .flatten();

    let invited_members_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM room_memberships
        WHERE room_id = $1 AND membership = 'invite'
        "#,
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
            events
                .into_iter()
                .find(|event| event.state_key.as_deref() == Some(""))
                .and_then(|event| {
                    event
                        .content
                        .get("guest_access")
                        .and_then(|value| value.as_str())
                        .map(|value| value == "can_join")
                })
        })
        .unwrap_or_else(|| {
            summary
                .as_ref()
                .map(|value| value.guest_access == "can_join")
                .unwrap_or(false)
        });

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
        r#"
        SELECT 1
        FROM room_memberships
        WHERE room_id = $1 AND user_id = $2
        LIMIT 1
        "#,
    )
    .bind(&room_id)
    .bind(&auth_user.user_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to check room membership: {}", e)))?;

    if membership.is_none() {
        return Err(ApiError::not_found(
            "Room not found or not a member".to_string(),
        ));
    }

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
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
        r#"
        SELECT DISTINCT room_id
        FROM room_memberships
        WHERE user_id = $1 AND membership = 'join'
        ORDER BY room_id
        "#,
    )
    .bind(user_id)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get joined rooms: {}", e)))?;

    let room_ids: Vec<String> = rooms
        .iter()
        .filter_map(|row| row.get::<Option<String>, _>("room_id"))
        .collect();

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
        r#"
        SELECT rm.room_id, rm.membership,
               COALESCE(r.name, '') as name,
               COALESCE(r.avatar_url, '') as avatar_url,
               rm.updated_ts
        FROM room_memberships rm
        LEFT JOIN rooms r ON rm.room_id = r.room_id
        WHERE rm.user_id = $1
        ORDER BY rm.updated_ts DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get rooms: {}", e)))?;

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

pub(crate) async fn get_messages(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(&params);
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let direction = params.get("dir").and_then(|v| v.as_str()).unwrap_or("b");

    Ok(Json(
        state
            .services
            .room_service
            .get_room_messages(&room_id, from, limit as i64, direction)
            .await?,
    ))
}

pub(crate) async fn send_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, _txn_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let s = body.to_string();
    if s.len() > 65536 {
        return Err(ApiError::bad_request(
            "Message content too long (max 64KB)".to_string(),
        ));
    }

    state
        .services
        .auth_service
        .verify_message_event_write(&room_id, &auth_user.user_id, &event_type)
        .await?;

    if event_type == "m.room.power_levels" {
        state
            .services
            .auth_service
            .verify_power_levels_change(&room_id, &auth_user.user_id, &body)
            .await?;
    }

    Ok(Json(
        state
            .services
            .room_service
            .send_message(&room_id, &auth_user.user_id, &event_type, &body)
            .await?,
    ))
}

pub(crate) async fn join_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.auth_service.validate_token(&token).await?;

    state
        .services
        .room_service
        .join_room(&room_id, &user_id)
        .await?;
    Ok(Json(json!({
        "room_id": room_id,
        "joined_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn join_room_by_id_or_alias(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id_or_alias): Path<String>,
    body: Option<Json<serde_json::Value>>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room_id = if room_id_or_alias.starts_with('!') {
        validate_room_id(&room_id_or_alias)?;
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        state
            .services
            .room_service
            .get_room_by_alias(&room_id_or_alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    } else {
        let alias = format!(
            "#{}:{}",
            room_id_or_alias, state.services.config.server.name
        );
        state
            .services
            .room_service
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    let via_servers = body
        .and_then(|b| b.get("via_servers").and_then(|v| v.as_array()).cloned())
        .unwrap_or_default();

    ::tracing::info!(
        "User {} joining room {} via {:?}",
        user_id,
        room_id,
        via_servers
    );

    state
        .services
        .room_service
        .join_room(&room_id, &user_id)
        .await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

pub(crate) async fn leave_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    state
        .services
        .room_service
        .leave_room(&room_id, &auth_user.user_id)
        .await?;
    Ok(Json(json!({
        "room_id": room_id,
        "left_ts": chrono::Utc::now().timestamp_millis()
    })))
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpgradeRoomRequest {
    new_version: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpgradeRoomResponse {
    replacement_room: String,
}

pub(crate) async fn upgrade_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<UpgradeRoomRequest>,
) -> Result<Json<UpgradeRoomResponse>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_state_write_access(&state, &auth_user, &room_id, "m.room.tombstone").await?;

    let new_room_id = state
        .services
        .room_service
        .upgrade_room(&room_id, &body.new_version, &auth_user.user_id)
        .await?;

    Ok(Json(UpgradeRoomResponse {
        replacement_room: new_room_id,
    }))
}

pub(crate) async fn forget_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    state
        .services
        .room_service
        .forget_room(&room_id, &auth_user.user_id)
        .await?;
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
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let from = params
        .get("from")
        .and_then(|value| parse_stream_token(value).or_else(|| value.parse::<i64>().ok()))
        .unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(10)
        .clamp(1, 100);

    let state_events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room state: {}", e)))?
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

    let members = state
        .services
        .room_service
        .get_room_members(&room_id, &auth_user.user_id)
        .await?;
    let messages = state
        .services
        .room_service
        .get_room_messages(&room_id, from, limit, "b")
        .await?;

    let member_events = members
        .get("chunk")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
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

pub(crate) async fn get_room_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room = room.ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_member = is_joined_room_member_or_creator(
        &state,
        &user_id,
        &room_id,
        room.creator_user_id.as_deref(),
    )
    .await?;

    if !room.is_public && !is_member {
        ::tracing::warn!(
            target: "security_audit",
            event = "unauthorized_room_members_access",
            user_id = user_id,
            room_id = room_id,
            "User attempted to access members of private room without being a member"
        );
        return Err(ApiError::forbidden(
            "You must be a member to view the member list of this private room".to_string(),
        ));
    }

    let members = state
        .services
        .room_service
        .get_room_members(&room_id, &user_id)
        .await?;
    Ok(Json(members))
}

pub(crate) async fn get_room_members_recent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.auth_service.validate_token(&token).await?;
    let members = state
        .services
        .room_service
        .get_room_members(&room_id, &user_id)
        .await?;

    let from = params
        .get("from")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100);

    let chunk = members
        .get("chunk")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let end_index = std::cmp::min(from.saturating_add(limit), chunk.len());
    let sliced_chunk = if from < chunk.len() {
        chunk[from..end_index].to_vec()
    } else {
        Vec::new()
    };

    Ok(Json(json!({
        "chunk": sliced_chunk,
        "start": from.to_string(),
        "end": end_index.to_string()
    })))
}

pub(crate) async fn get_joined_members(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room = room.ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_member = is_joined_room_member(&state, &auth_user.user_id, &room_id).await?;

    if !room.is_public && !is_member {
        return Err(ApiError::forbidden(
            "You must be a member to view the joined members of this private room".to_string(),
        ));
    }

    let members = state
        .services
        .member_storage
        .get_joined_members(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get joined members: {}", e)))?;

    let joined: std::collections::HashMap<String, Value> = members
        .into_iter()
        .map(|m| {
            let user_id = m.user_id.clone();
            (
                user_id,
                json!({
                    "display_name": m.display_name,
                    "avatar_url": m.avatar_url
                }),
            )
        })
        .collect();

    Ok(Json(json!({
        "joined": joined
    })))
}

pub(crate) async fn invite_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let invitee = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(invitee)?;

    state
        .services
        .auth_service
        .can_invite_user(&room_id, &auth_user.user_id)
        .await?;

    state
        .services
        .room_service
        .invite_user(&room_id, &auth_user.user_id, invitee)
        .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "invited_user_id": invitee,
        "invited_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn knock_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id_or_alias): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room_id = if room_id_or_alias.starts_with('!') {
        validate_room_id(&room_id_or_alias)?;
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        state
            .services
            .room_service
            .get_room_by_alias(&room_id_or_alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    } else {
        let alias = format!(
            "#{}:{}",
            room_id_or_alias, state.services.config.server.name
        );
        state
            .services
            .room_service
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    ::tracing::info!("User {} knocking on room {}", user_id, room_id);

    state
        .services
        .room_service
        .knock_room(&room_id, &user_id, reason.as_deref())
        .await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

pub(crate) async fn invite_user_by_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.auth_service.validate_token(&token).await?;

    validate_room_id(&room_id)?;

    let invitee = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(invitee)?;

    state
        .services
        .auth_service
        .can_invite_user(&room_id, &user_id)
        .await?;

    ::tracing::info!("User {} inviting {} to room {}", user_id, invitee, room_id);

    state
        .services
        .room_service
        .invite_user(&room_id, &user_id, invitee)
        .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "invited_user_id": invitee,
        "invited_ts": chrono::Utc::now().timestamp_millis()
    })))
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
            return Err(ApiError::bad_request(
                "Visibility must be 'public' or 'private'".to_string(),
            ));
        }
    }

    let room_alias = body.get("room_alias_name").and_then(|v| v.as_str());
    if let Some(alias) = room_alias {
        if alias.len() > 255 {
            return Err(ApiError::bad_request(
                "Room alias name too long".to_string(),
            ));
        }
        if !alias
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(ApiError::bad_request(
                "Invalid characters in room alias name".to_string(),
            ));
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

    let invite = body.get("invite").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect::<Vec<String>>()
    });

    if let Some(ref inv) = invite {
        if inv.len() > 100 {
            return Err(ApiError::bad_request(
                "Too many invites (max 100)".to_string(),
            ));
        }
    }

    let preset = body.get("preset").and_then(|v| v.as_str());

    let room_type = body.get("room_type").and_then(|v| v.as_str());

    let config = CreateRoomConfig {
        visibility: visibility.map(|s| s.to_string()),
        room_alias_name: room_alias.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
        topic: topic.map(|s| s.to_string()),
        invite_list: invite,
        preset: preset.map(|s| s.to_string()),
        room_type: room_type.map(|s| s.to_string()),
        ..Default::default()
    };

    let result = state
        .services
        .room_service
        .create_room(&user_id, config.clone())
        .await?;

    if config.room_type.as_deref() == Some("m.space") {
        let space_request = crate::storage::space::CreateSpaceRequest {
            room_id: result
                .get("room_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name: config.name.clone(),
            topic: config.topic.clone(),
            avatar_url: None,
            creator: user_id.to_string(),
            join_rule: config.preset.clone(),
            visibility: config.visibility.clone(),
            is_public: config.visibility.as_ref().map(|v| v == "public"),
            parent_space_id: None,
        };
        if let Err(e) = state
            .services
            .space_service
            .create_space(space_request)
            .await
        {
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = state
        .services
        .room_service
        .get_room_visibility(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room visibility: {}", e)))?;

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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = body
        .get("visibility")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing visibility field".to_string()))?;

    if visibility != "public" && visibility != "private" {
        return Err(ApiError::bad_request(
            "visibility must be 'public' or 'private'".to_string(),
        ));
    }

    ensure_room_member(
        &state,
        &auth_user,
        &room_id,
        "You must be a member of this room to update room visibility",
    )
    .await?;

    let is_creator = state
        .services
        .room_service
        .is_room_creator(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room creator: {}", e)))?;

    if !is_creator {
        return Err(ApiError::forbidden(
            "Only the room creator can update room visibility".to_string(),
        ));
    }

    let is_public = visibility == "public";

    state
        .services
        .room_service
        .set_room_directory(&room_id, is_public)
        .await?;

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

    let rooms = state
        .services
        .room_service
        .get_joined_rooms(&user_id)
        .await?;

    Ok(Json(json!({
        "joined_rooms": rooms
    })))
}

pub(crate) async fn get_room_state(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room_exists = state
        .services
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{}' not found", room_id)));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;
    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "event_id": e.event_id,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(JsonValue::Array(state_events)))
}

pub(crate) async fn get_state_by_type(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room_exists = state
        .services
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{}' not found", room_id)));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &final_event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "event_id": e.event_id,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({ "events": state_events })))
}

pub(crate) async fn get_state_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &final_event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let event = events
        .iter()
        .find(|e| {
            e.state_key.as_deref() == Some(state_key.as_str())
                || (e.state_key.as_ref().map(|s| s.is_empty()) == Some(true)
                    && state_key.is_empty())
        })
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    let mut response = json!({
        "type": event.event_type,
        "event_id": event.event_id,
        "sender": event.sender,
        "state_key": event.state_key
    });

    if let Some(content) = event.content.as_object() {
        for (k, v) in content {
            response[k] = v.clone();
        }
    }

    Ok(Json(response))
}

pub(crate) async fn send_state_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let content = body;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = normalize_room_event_type(&event_type);
    ensure_room_state_write_access(&state, &auth_user, &room_id, &final_event_type).await?;

    let beacon_info_params = if final_event_type.starts_with("m.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3672.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3489.beacon_info")
    {
        let beacon_obj = content
            .get("m.beacon_info")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                ApiError::bad_request("Missing m.beacon_info in beacon_info content".to_string())
            })?;

        let timeout = beacon_obj
            .get("timeout")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ApiError::bad_request("Missing m.beacon_info.timeout".to_string()))?;

        let is_live = beacon_obj
            .get("live")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let description = beacon_obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());

        let created_ts = content
            .get("m.ts")
            .or_else(|| content.get("org.matrix.msc3488.ts"))
            .and_then(|v| v.as_i64())
            .unwrap_or(now);

        let asset_type = content
            .get("m.asset")
            .or_else(|| content.get("org.matrix.msc3488.asset"))
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("m.self")
            .to_string();

        Some(crate::storage::beacon::CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: new_event_id.clone(),
            state_key: auth_user.user_id.clone(),
            sender: auth_user.user_id.clone(),
            description,
            timeout,
            is_live,
            asset_type,
            created_ts,
        })
    } else {
        None
    };

    let state_event = state
        .services
        .room_service
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content,
                state_key: Some(auth_user.user_id.clone()),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to send state event: {}", e)))?;

    if let Some(params) = beacon_info_params {
        state
            .services
            .beacon_service
            .create_beacon(params)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to index beacon_info: {}", e)))?;
    }

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": state_event.event_type,
        "state_key": state_event.state_key
    })))
}

pub(crate) async fn put_state_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = normalize_room_event_type(&event_type);
    ensure_room_state_write_access(&state, &auth_user, &room_id, &final_event_type).await?;

    if (final_event_type.starts_with("m.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3672.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3489.beacon_info"))
        && state_key != auth_user.user_id
    {
        return Err(ApiError::forbidden(
            "beacon_info state_key must match sender".to_string(),
        ));
    }

    let beacon_info_params = if final_event_type.starts_with("m.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3672.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3489.beacon_info")
    {
        let beacon_obj = body
            .get("m.beacon_info")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                ApiError::bad_request("Missing m.beacon_info in beacon_info content".to_string())
            })?;

        let timeout = beacon_obj
            .get("timeout")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ApiError::bad_request("Missing m.beacon_info.timeout".to_string()))?;

        let is_live = beacon_obj
            .get("live")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let description = beacon_obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());

        let created_ts = body
            .get("m.ts")
            .or_else(|| body.get("org.matrix.msc3488.ts"))
            .and_then(|v| v.as_i64())
            .unwrap_or(now);

        let asset_type = body
            .get("m.asset")
            .or_else(|| body.get("org.matrix.msc3488.asset"))
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("m.self")
            .to_string();

        Some(crate::storage::beacon::CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: new_event_id.clone(),
            state_key: state_key.clone(),
            sender: auth_user.user_id.clone(),
            description,
            timeout,
            is_live,
            asset_type,
            created_ts,
        })
    } else {
        None
    };

    let event = state
        .services
        .room_service
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content: body,
                state_key: Some(state_key),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    if let Some(params) = beacon_info_params {
        state
            .services
            .beacon_service
            .create_beacon(params)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to index beacon_info: {}", e)))?;
    }

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

pub(crate) async fn get_state_event_empty_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &final_event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let event = events
        .iter()
        .find(|e| e.state_key.as_ref().map(|s| s.is_empty()) == Some(true))
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    let mut response = json!({
        "type": event.event_type,
        "event_id": event.event_id,
        "sender": event.sender,
        "state_key": event.state_key
    });

    if let Some(content) = event.content.as_object() {
        for (k, v) in content {
            response[k] = v.clone();
        }
    }

    Ok(Json(response))
}

pub(crate) async fn get_power_levels(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, "m.room.power_levels")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get power levels: {}", e)))?;

    let event = events
        .iter()
        .find(|e| e.state_key.as_ref().map(|s| s.is_empty()) == Some(true))
        .ok_or_else(|| ApiError::not_found("Power levels not found".to_string()))?;

    let power_levels_content = event.content.clone();

    Ok(Json(power_levels_content))
}

pub(crate) async fn put_state_event_empty_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = normalize_room_event_type(&event_type);
    ensure_room_state_write_access(&state, &auth_user, &room_id, &final_event_type).await?;

    let event = state
        .services
        .room_service
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content: body,
                state_key: Some("".to_string()),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

pub(crate) async fn put_state_event_no_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = normalize_room_event_type(&event_type);
    ensure_room_state_write_access(&state, &auth_user, &room_id, &final_event_type).await?;

    let event = state
        .services
        .room_service
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type,
                content: body,
                state_key: Some("".to_string()),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

pub(crate) async fn send_receipt(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_receipt_type(&receipt_type)?;
    let event_id = event_id.replace("%24", "$");
    validate_event_id(&event_id)?;

    ensure_room_member(
        &state,
        &auth_user,
        &room_id,
        "You must be a member of this room to send receipts",
    )
    .await?;

    get_room_event(&state.services.event_storage, &room_id, &event_id).await?;

    state
        .services
        .room_storage
        .add_receipt(
            &auth_user.user_id,
            &auth_user.user_id,
            &room_id,
            &event_id,
            &receipt_type,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store receipt: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event_id,
        "receipt_type": receipt_type,
        "ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn get_receipts(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_receipt_type(&receipt_type)?;
    let event_id = event_id.replace("%24", "$");
    validate_event_id(&event_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;
    get_room_event(&state.services.event_storage, &room_id, &event_id).await?;

    let receipts = state
        .services
        .room_storage
        .get_receipts(&room_id, &receipt_type, &event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get receipts: {}", e)))?;

    Ok(Json(build_receipts_chunk(receipts)))
}

pub fn build_receipts_chunk(receipts: Vec<Receipt>) -> Value {
    let receipt_list: Vec<Value> = receipts
        .into_iter()
        .map(|r| {
            json!({
                "user_id": r.user_id,
                "receipt_type": r.receipt_type,
                "event_id": r.event_id,
                "ts": r.ts,
                "data": r.data
            })
        })
        .collect();

    json!({ "chunk": receipt_list })
}

pub(crate) async fn set_read_markers(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_member = is_joined_room_member_or_creator(
        &state,
        &auth_user.user_id,
        &room_id,
        room.creator_user_id.as_deref(),
    )
    .await?;

    if !is_member {
        return Err(ApiError::forbidden(
            "You are not a member of this room".to_string(),
        ));
    }

    write_read_markers_from_body(
        &state.services.room_storage,
        &state.services.event_storage,
        &room_id,
        &auth_user.user_id,
        &body,
    )
    .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub async fn write_read_markers_from_body(
    room_storage: &RoomStorage,
    event_storage: &EventStorage,
    room_id: &str,
    user_id: &str,
    body: &Value,
) -> Result<(), ApiError> {
    if let Some(event_id) = body.get("m.fully_read").and_then(|v| v.as_str()) {
        if event_id.starts_with('$') {
            validate_event_id(event_id)?;
            get_room_event(event_storage, room_id, event_id).await?;
            room_storage
                .update_read_marker_with_type(room_id, user_id, event_id, "m.fully_read")
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to set fully_read marker: {}", e))
                })?;
        }
    }

    if let Some(event_id) = body.get("m.private_read").and_then(|v| v.as_str()) {
        if event_id.starts_with('$') {
            validate_event_id(event_id)?;
            get_room_event(event_storage, room_id, event_id).await?;
            room_storage
                .update_read_marker_with_type(room_id, user_id, event_id, "m.private_read")
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to set private_read marker: {}", e))
                })?;
        }
    }

    if let Some(marked_unread) = body.get("m.marked_unread").and_then(|v| v.as_object()) {
        if let Some(events) = marked_unread.get("events").and_then(|v| v.as_array()) {
            for event in events {
                if let Some(event_id) = event.as_str() {
                    if event_id.starts_with('$') {
                        validate_event_id(event_id)?;
                        get_room_event(event_storage, room_id, event_id).await?;
                        room_storage
                            .update_read_marker_with_type(
                                room_id,
                                user_id,
                                event_id,
                                "m.marked_unread",
                            )
                            .await
                            .map_err(|e| {
                                ApiError::internal(format!(
                                    "Failed to set marked_unread marker: {}",
                                    e
                                ))
                            })?;
                    }
                }
            }
        }
    }

    if let Some(event_id) = body.get("m.read").and_then(|v| v.as_str()) {
        if event_id.starts_with('$') {
            validate_event_id(event_id)?;
            get_room_event(event_storage, room_id, event_id).await?;
            room_storage
                .update_read_marker_with_type(room_id, user_id, event_id, "m.fully_read")
                .await
                .map_err(|e| ApiError::internal(format!("Failed to set read marker: {}", e)))?;
        }
    }

    Ok(())
}

pub(crate) async fn get_room_membership(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, target_user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_user_id(&target_user_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let membership = state
        .services
        .member_storage
        .get_member(&room_id, &target_user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .map(|m| m.membership)
        .unwrap_or_else(|| "leave".to_string());

    Ok(Json(json!({
        "membership": membership
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $5, $5)
        ON CONFLICT (user_id, room_id, data_type)
        DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind(&data_type)
    .bind(&body)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let result = sqlx::query(
        "SELECT data FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3",
    )
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind(&data_type)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => Ok(Json(
            row.get::<Option<Value>, _>("data")
                .unwrap_or_else(|| json!({})),
        )),
        None => Err(ApiError::not_found(
            "Room account data not found".to_string(),
        )),
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let voip_service = crate::services::voip_service::VoipService::new(std::sync::Arc::new(
        state.services.config.voip.clone(),
    ));

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

pub(crate) async fn get_room_sync(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let timeout = params
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    let full_state = params
        .get("full_state")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let since = params.get("since").and_then(|v| v.as_str());

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        state.services.sync_service.room_sync(
            &auth_user.user_id,
            &room_id,
            timeout,
            full_state,
            since,
        ),
    )
    .await;

    match result {
        Ok(Ok(value)) => Ok(Json(value)),
        Ok(Err(e)) => {
            ::tracing::error!(
                "Room sync error for user {} in room {}: {}",
                auth_user.user_id,
                room_id,
                e
            );
            Err(e)
        }
        Err(_) => {
            ::tracing::error!(
                "Room sync timeout for user {} in room {}",
                auth_user.user_id,
                room_id
            );
            Err(ApiError::internal(
                "Room sync operation timed out".to_string(),
            ))
        }
    }
}

pub(crate) async fn get_room_thread_by_id(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, thread_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&thread_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
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

    let thread_detail = state
        .services
        .thread_service
        .get_thread(request, Some(&auth_user.user_id))
        .await?;

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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_encrypted = sqlx::query(
        r#"
        SELECT 1
        FROM room_events
        WHERE room_id = $1 AND event_type = 'm.room.encryption'
        LIMIT 1
        "#,
    )
    .bind(&room_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to check room encryption: {}", e)))?
    .is_some();

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
        "join_rule": join_rule
    })))
}

async fn latest_room_key_backup_version(
    state: &AppState,
    user_id: &str,
) -> Result<Option<String>, ApiError> {
    let backups = state
        .services
        .backup_service
        .get_all_backups(user_id)
        .await?;

    Ok(backups
        .into_iter()
        .max_by_key(|backup| backup.version)
        .map(|backup| backup.version.to_string()))
}

async fn ensure_room_key_backup_version(
    state: &AppState,
    user_id: &str,
) -> Result<String, ApiError> {
    if let Some(version) = latest_room_key_backup_version(state, user_id).await? {
        return Ok(version);
    }

    state
        .services
        .backup_service
        .create_backup(user_id, "m.megolm.v1.aes-sha2", Some(json!({})))
        .await
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
                    } else if let Some(session_id) = value.get("session_id").and_then(Value::as_str)
                    {
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let version = latest_room_key_backup_version(&state, &auth_user.user_id).await?;
    let keys = if let Some(version) = version.clone() {
        state
            .services
            .backup_service
            .get_room_backup_keys(&auth_user.user_id, &room_id, &version)
            .await?
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let version = latest_room_key_backup_version(&state, &auth_user.user_id).await?;
    let count = if let Some(version) = version {
        state
            .services
            .backup_service
            .get_room_backup_keys(&auth_user.user_id, &room_id, &version)
            .await?
            .len()
    } else {
        0
    };

    Ok(Json(json!({
        "count": count
    })))
}

pub(crate) async fn claim_room_keys(
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let version = latest_room_key_backup_version(&state, &auth_user.user_id).await?;
    let requested_sessions = requested_room_key_session_ids(&body, &room_id);
    let keys = if let Some(version) = version {
        state
            .services
            .backup_service
            .get_room_backup_keys(&auth_user.user_id, &room_id, &version)
            .await?
    } else {
        Vec::new()
    };

    let one_time_keys = keys
        .into_iter()
        .filter(|key| {
            requested_sessions
                .as_ref()
                .map(|session_ids| session_ids.contains(&key.session_id))
                .unwrap_or(true)
        })
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let version = latest_room_key_backup_version(&state, &auth_user.user_id)
        .await?
        .unwrap_or_else(|| "0".to_string());

    Ok(Json(json!({
        "version": version
    })))
}

pub(crate) async fn get_room_message_queue(
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let pending_events: Vec<serde_json::Value> = sqlx::query(
        r#"
        SELECT event_id, room_id, user_id, event_type, content, origin_server_ts, status
        FROM events
        WHERE room_id = $1 AND status = 'pending'
        ORDER BY origin_server_ts ASC
        LIMIT 100
        "#,
    )
    .bind(&room_id)
    .fetch_all(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get pending events: {}", e)))?
    .into_iter()
    .map(|row| {
        use sqlx::Row;
        serde_json::json!({
            "event_id": row.get::<Option<String>, _>("event_id"),
            "room_id": row.get::<Option<String>, _>("room_id"),
            "user_id": row.get::<Option<String>, _>("user_id"),
            "event_type": row.get::<Option<String>, _>("event_type"),
            "origin_server_ts": row.get::<Option<i64>, _>("origin_server_ts"),
            "status": row.get::<Option<String>, _>("status")
        })
    })
    .collect();

    let processing_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE room_id = $1 AND status = 'processing'",
    )
    .bind(&room_id)
    .fetch_one(&*state.services.event_storage.pool)
    .await
    .unwrap_or(0);

    let failed_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE room_id = $1 AND status = 'failed'")
            .bind(&room_id)
            .fetch_one(&*state.services.event_storage.pool)
            .await
            .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "queue": {
            "pending": pending_events,
            "pending_count": pending_events.len(),
            "processing_count": processing_count,
            "failed_count": failed_count
        },
        "status": {
            "healthy": failed_count < 100,
            "total_pending": pending_events.len() + processing_count as usize
        }
    })))
}

pub(crate) async fn get_room_timeline(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(&params);
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let direction = params.get("dir").and_then(|v| v.as_str()).unwrap_or("b");

    Ok(Json(
        state
            .services
            .room_service
            .get_room_messages(&room_id, from, limit as i64, direction)
            .await?,
    ))
}

pub(crate) async fn get_room_unread_count(
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let (notification_count, highlight_count) = state
        .services
        .sync_service
        .room_unread_counts(&room_id, &auth_user.user_id)
        .await?;

    Ok(Json(json!({
        "notification_count": notification_count,
        "highlight_count": highlight_count
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
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

    // Remove null fields from response
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

pub(crate) async fn get_room_encrypted_events(
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let encrypted_events = state
        .services
        .event_storage
        .get_room_events_by_type(&room_id, "m.room.encrypted", 100)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get encrypted events: {}", e)))?;

    let events: Vec<serde_json::Value> = encrypted_events
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "event_id": e.event_id,
                "room_id": e.room_id,
                "sender": e.user_id,
                "type": e.event_type,
                "content": e.content,
                "origin_server_ts": e.origin_server_ts
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "events": events,
        "total": events.len()
    })))
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
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
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $5, $5)
        ON CONFLICT (user_id, room_id, data_type)
        DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind("m.room.vault_data")
    .bind(&body)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, 20)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room events: {}", e)))?;

    let lines: Vec<Value> = events
        .iter()
        .rev()
        .map(|event| {
            let body = event
                .content
                .get("body")
                .and_then(|value| value.as_str())
                .unwrap_or("");
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let aliases = state
        .services
        .room_storage
        .get_room_aliases(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room aliases: {}", e)))?;

    let external_ids: Vec<Value> = aliases
        .into_iter()
        .map(|alias| json!({"type": "room_alias", "value": alias}))
        .collect();

    Ok(Json(json!({
        "room_id": room_id,
        "external_ids": external_ids
    })))
}

pub(crate) async fn get_room_event_perspective(
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, 100)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room events: {}", e)))?;

    let mut senders: HashMap<String, usize> = HashMap::new();
    let mut event_types: HashMap<String, usize> = HashMap::new();
    for event in &events {
        *senders.entry(event.user_id.clone()).or_insert(0) += 1;
        *event_types.entry(event.event_type.clone()).or_insert(0) += 1;
    }

    Ok(Json(json!({
        "room_id": room_id,
        "perspective": {
            "event_count": events.len(),
            "latest_event_id": events.first().map(|event| event.event_id.clone()),
            "sender_activity": senders,
            "event_types": event_types
        }
    })))
}

pub(crate) async fn get_room_user_fragments(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_user_id(&user_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, 200)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room events: {}", e)))?;

    let fragments: Vec<Value> = events
        .into_iter()
        .filter(|event| event.user_id == user_id)
        .map(|event| {
            json!({
                "event_id": event.event_id,
                "type": event.event_type,
                "snippet": event.content.get("body").and_then(|value| value.as_str()),
                "origin_server_ts": event.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": user_id,
        "fragments": fragments,
        "total": fragments.len()
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
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

pub(crate) async fn translate_room_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = get_room_event(&state.services.event_storage, &room_id, &event_id).await?;
    let source_text = event
        .content
        .get("body")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let requested_text = body.get("text").and_then(|value| value.as_str());

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event_id,
        "source_text": source_text,
        "requested_text": requested_text,
        "translated_text": if source_text.is_empty() {
            requested_text.unwrap_or("")
        } else {
            source_text
        }
    })))
}

pub(crate) async fn convert_room_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = get_room_event(&state.services.event_storage, &room_id, &event_id).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event.event_id,
        "converted": {
            "type": event.event_type,
            "sender": event.user_id,
            "body": event.content.get("body"),
            "msgtype": event.content.get("msgtype"),
            "content": event.content,
            "origin_server_ts": event.origin_server_ts
        }
    })))
}

pub(crate) async fn get_room_reduced_events(
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, 100)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room events: {}", e)))?;

    let mut seen_types = HashSet::new();
    let reduced_events: Vec<Value> = events
        .into_iter()
        .filter(|event| seen_types.insert(event.event_type.clone()))
        .map(|event| {
            json!({
                "event_id": event.event_id,
                "room_id": event.room_id,
                "sender": event.user_id,
                "type": event.event_type,
                "content": event.content,
                "origin_server_ts": event.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "room_id": room_id,
        "events": reduced_events,
        "total": reduced_events.len()
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let device = state
        .services
        .device_storage
        .get_device(&device_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Device not found".to_string()))?;

    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &device.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to verify device membership: {}", e)))?;

    if !is_member {
        return Err(ApiError::not_found(
            "Device not found in this room".to_string(),
        ));
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

pub(crate) async fn forward_room_keys(
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let keys = normalize_forwarded_room_keys(&body, &room_id);
    let version = ensure_room_key_backup_version(&state, &auth_user.user_id).await?;

    if !keys.is_empty() {
        state
            .services
            .backup_service
            .upload_room_keys_for_room(&auth_user.user_id, &room_id, &version, keys.clone())
            .await?;
    }

    Ok(Json(json!({
        "count": keys.len(),
        "etag": version,
        "version": version
    })))
}

pub(crate) async fn get_room_event_url(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to this room".to_string(),
        ));
    }

    let content = event.content.as_object().cloned().unwrap_or_default();
    let mut urls: Vec<serde_json::Value> = Vec::new();

    if let Some(url) = content.get("url").and_then(|v| v.as_str()) {
        urls.push(serde_json::json!({
            "type": "mxc",
            "url": url,
            "field": "url"
        }));
    }

    if let Some(info) = content.get("info").and_then(|v| v.as_object()) {
        if let Some(thumbnail_url) = info.get("thumbnail_url").and_then(|v| v.as_str()) {
            urls.push(serde_json::json!({
                "type": "mxc",
                "url": thumbnail_url,
                "field": "info.thumbnail_url"
            }));
        }
    }

    Ok(Json(serde_json::json!({
        "event_id": event_id,
        "room_id": room_id,
        "urls": urls,
        "total": urls.len()
    })))
}

pub(crate) async fn sign_room_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to this room".to_string(),
        ));
    }

    let device_id = body
        .get("device_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let default_key_id = format!("ed25519:{}", device_id);
    let key_id = body
        .get("key_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&default_key_id);

    let signature = body
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("signature is required".to_string()))?;

    let algorithm = body
        .get("algorithm")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            key_id
                .split(':')
                .next()
                .filter(|value| !value.is_empty())
                .unwrap_or("ed25519")
                .to_string()
        });

    let created_ts = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO event_signatures (id, event_id, user_id, device_id, signature, key_id, algorithm, created_ts)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (event_id, user_id, device_id, key_id) DO UPDATE
        SET signature = EXCLUDED.signature,
            algorithm = EXCLUDED.algorithm,
            created_ts = EXCLUDED.created_ts
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&event_id)
    .bind(&auth_user.user_id)
    .bind(device_id)
    .bind(signature)
    .bind(key_id)
    .bind(&algorithm)
    .bind(created_ts)
    .execute(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to save signature: {}", e)))?;

    Ok(Json(serde_json::json!({
        "event_id": event_id,
        "room_id": room_id,
        "user_id": auth_user.user_id,
        "device_id": device_id,
        "key_id": key_id,
        "algorithm": algorithm,
        "signed": true,
        "created_ts": created_ts
    })))
}

pub(crate) async fn verify_room_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to this room".to_string(),
        ));
    }

    let signatures: Vec<crate::e2ee::signature::EventSignature> = sqlx::query_as(
        r#"
        SELECT id, event_id, user_id, device_id, signature, key_id, created_ts
        FROM event_signatures
        WHERE event_id = $1
        "#,
    )
    .bind(&event_id)
    .fetch_all(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get signatures: {}", e)))?;

    let verify_user_id = body.get("user_id").and_then(|v| v.as_str());
    let verify_device_id = body.get("device_id").and_then(|v| v.as_str());

    let verified_signatures: Vec<serde_json::Value> = signatures
        .iter()
        .filter(|s| {
            verify_user_id.is_none_or(|uid| s.user_id == uid)
                && verify_device_id.is_none_or(|did| s.device_id == did)
        })
        .map(|s| {
            serde_json::json!({
                "user_id": s.user_id,
                "device_id": s.device_id,
                "key_id": s.key_id,
                "signature": s.signature,
                "created_ts": s.created_ts
            })
        })
        .collect();

    let is_valid = !verified_signatures.is_empty();

    Ok(Json(serde_json::json!({
        "event_id": event_id,
        "room_id": room_id,
        "valid": is_valid,
        "signatures": verified_signatures,
        "total": verified_signatures.len()
    })))
}

pub(crate) async fn get_room_invites(
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let _invites = state
        .services
        .member_storage
        .get_joined_members(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room invites: {}", e)))?;

    let invited_members: Vec<serde_json::Value> = state
        .services
        .member_storage
        .get_room_members(&room_id, "invite")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room members: {}", e)))?
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "user_id": m.user_id,
                "sender": m.sender,
                "display_name": m.display_name,
                "avatar_url": m.avatar_url,
                "event_id": m.event_id,
                "reason": m.reason,
                "updated_ts": m.updated_ts
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "invites": invited_members,
        "total": invited_members.len()
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room_policy = state
        .services
        .retention_storage
        .get_room_policy(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get retention policy: {}", e)))?;

    let server_policy = state
        .services
        .retention_storage
        .get_server_policy()
        .await
        .ok();

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
            let default =
                server_policy.unwrap_or(crate::storage::retention::ServerRetentionPolicy {
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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let rooms = if let Some(space) = state
        .services
        .space_service
        .get_space_by_room(&room_id)
        .await?
    {
        let children = state
            .services
            .space_service
            .get_space_children(&space.space_id)
            .await?;

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
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
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
        .or_else(|| {
            body.get("search")
                .and_then(|value| value.get("term"))
                .and_then(Value::as_str)
        })
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
        r#"
        SELECT event_id, room_id, sender, event_type, content, origin_server_ts
        FROM events
        WHERE room_id = $1
          AND event_type = 'm.room.message'
          AND LOWER(content::text) LIKE $2
        ORDER BY origin_server_ts DESC
        LIMIT $3
        "#,
    )
    .bind(&room_id)
    .bind(&search_pattern)
    .bind(limit)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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

pub(crate) async fn get_membership_events(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as i64;

    let memberships = state
        .services
        .member_storage
        .get_membership_history(&room_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get membership events: {}", e)))?;

    let events: Vec<Value> = memberships
        .into_iter()
        .map(|m| {
            json!({
                "event_id": m.event_id,
                "type": m.event_type,
                "sender": m.sender,
                "state_key": m.user_id,
                "content": {
                    "membership": m.membership
                },
                "origin_server_ts": m.joined_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events
    })))
}

pub(crate) async fn redact_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, _txn_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !event_id.starts_with('$') {
        return Ok(Json(json!({
            "event_id": event_id
        })));
    }
    validate_event_id(&event_id)?;

    let original_event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if original_event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to this room".to_string(),
        ));
    }

    state
        .services
        .auth_service
        .can_redact_event(&room_id, &auth_user.user_id, &original_event.user_id)
        .await?;

    let reason = body.get("reason").and_then(|v| v.as_str());

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let content = json!({
        "reason": reason
    });

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.redaction".to_string(),
                content,
                state_key: None,
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to redact event: {}", e)))?;

    state
        .services
        .event_storage
        .redact_event_content(&event_id)
        .await
        .map_err(|e| {
            ::tracing::warn!(
                target: "security_audit",
                event = "redaction_content_failed",
                event_id = %event_id,
                error = %e,
                "Redaction event created but content redaction failed"
            );
            ApiError::internal(format!("Failed to redact event content: {}", e))
        })?;

    Ok(Json(json!({
        "event_id": new_event_id
    })))
}

pub(crate) async fn kick_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    if let Some(r) = reason {
        if r.len() > 512 {
            return Err(ApiError::bad_request("Reason too long".to_string()));
        }
    }

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .auth_service
        .can_kick_user(&room_id, &auth_user.user_id, target)
        .await?;

    let kicked_by = auth_user.user_id.clone();
    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "leave",
        "reason": reason.unwrap_or("")
    });

    state
        .services
        .member_storage
        .remove_member(&room_id, target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to kick user: {}", e)))?;

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: "m.room.member".to_string(),
                content,
                state_key: Some(target.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| {
            ::tracing::warn!(
                "Failed to create membership event for room {}: {}",
                room_id,
                e
            );
        })
        .ok();

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": target,
        "kicked_by": kicked_by,
        "membership": "leave",
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn ban_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    if let Some(r) = reason {
        if r.len() > 512 {
            return Err(ApiError::bad_request("Reason too long".to_string()));
        }
    }

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .auth_service
        .can_ban_user(&room_id, &auth_user.user_id, target)
        .await?;

    let banned_by = auth_user.user_id.clone();
    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "ban",
        "reason": reason.unwrap_or("")
    });

    state
        .services
        .member_storage
        .add_member(&room_id, target, "ban", None, None, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to ban user: {}", e)))?;

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: "m.room.member".to_string(),
                content,
                state_key: Some(target.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| {
            ::tracing::warn!(
                "Failed to create membership event for room {}: {}",
                room_id,
                e
            );
        })
        .ok();

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": target,
        "banned_by": banned_by,
        "membership": "ban",
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn unban_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    state
        .services
        .auth_service
        .can_unban_user(&room_id, &auth_user.user_id, target)
        .await?;

    let unbanned_by = auth_user.user_id.clone();
    state
        .services
        .member_storage
        .remove_member(&room_id, target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to unban user: {}", e)))?;

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "leave"
    });

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: "m.room.member".to_string(),
                content,
                state_key: Some(target.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| {
            ::tracing::warn!(
                "Failed to create membership event for room {}: {}",
                room_id,
                e
            );
        })
        .ok();

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": target,
        "unbanned_by": unbanned_by,
        "membership": "leave",
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn get_room_permissions(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let power_levels_events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, "m.room.power_levels")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get power levels: {}", e)))?;

    let pl_content = power_levels_events
        .iter()
        .find(|e| e.state_key.as_ref().map(|s| s.is_empty()) == Some(true))
        .map(|e| e.content.clone())
        .unwrap_or(json!({}));

    let join_rules_events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, "m.room.join_rules")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get join rules: {}", e)))?;

    let join_rule = join_rules_events
        .iter()
        .find(|e| e.state_key.as_ref().map(|s| s.is_empty()) == Some(true))
        .and_then(|e| e.content.get("join_rule").cloned())
        .unwrap_or(json!("invite"));

    let user_pl = pl_content
        .get("users")
        .and_then(|u| u.get(&auth_user.user_id))
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| {
            pl_content
                .get("users_default")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
        });

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": auth_user.user_id,
        "user_power_level": user_pl,
        "join_rule": join_rule,
        "power_levels": pl_content
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
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let aliases = state
        .services
        .room_storage
        .get_room_aliases(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room aliases: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "aliases": aliases,
        "name": room.name,
        "canonical_alias": room.canonical_alias
    })))
}
