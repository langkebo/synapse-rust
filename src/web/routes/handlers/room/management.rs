use super::{ensure_room_state_write_access, ensure_room_view_access, UpgradeRoomRequest, UpgradeRoomResponse};
use crate::common::ApiError;
use crate::map_internal;
use crate::services::CreateRoomConfig;
use crate::web::routes::{
    ensure_room_member, extract_token_from_headers, validate_room_id, AppState, AuthenticatedUser,
    OptionalAuthenticatedUser,
};
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{json, Value};

// =============================================================================
// Observed repeated patterns (Phase 4 extraction candidates)
// =============================================================================
//
// 1. Null-cleaning after JSON construction (get_room_metadata, lines ~718-737)
//    After building a JSON response from a Room record (which carries
//    Option<String> fields), the code strips null-valued keys:
//
//        if let Some(obj) = response.as_object_mut() {
//            if obj.get("name").is_some_and(|v| v.is_null()) { obj.remove("name"); }
//            // ... repeats for topic, avatar_url, canonical_alias, creator, encryption
//        }
//
//    This pattern is needed because Room struct fields like name, topic,
//    avatar_url are Option<String> — they serialize as `null` rather than
//    being absent. An extraction would be a helper that takes a list of keys
//    and removes any whose value is JSON Null, applied across all response-
//    building handlers that construct JSON from Room records.
//
// 2. Room-type extraction from m.room.create events (get_room_metadata
//    and various hierarchy handlers)
//
//        let room_type = state_events
//            .iter()
//            .find(|e| e.get("type").and_then(|v| v.as_str()) == Some("m.room.create"))
//            .and_then(|e| e.get("content"))
//            .and_then(|c| c.get("type"))
//            .and_then(|v| v.as_str())
//            .map_or(Value::Null, |s| Value::String(s.to_string()));
//
//    This walks the state events array, finds the create event, digs into
//    content.type, and maps the result. Repeated verbatim in search.rs
//    (build_room_hierarchy_response). Extraction: a method on RoomService
//    that accepts the state_events slice and returns the room type string.
// =============================================================================

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

    let membership = state.services.rooms.room_service.get_room_membership(&room_id, user_id).await?;

    if membership.is_none() {
        return Err(ApiError::not_found("Room not found or not a member".to_string()));
    }

    let room = state
        .services
        .rooms
        .room_service
        .get_room_record(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let summary = state.services.rooms.room_summary_service.get_summary(&room_id).await.ok().flatten();
    let invited_members_count = state.services.rooms.room_service.get_invited_members_count(&room_id).await?;

    let guest_can_join = state
        .services
        .rooms
        .room_service
        .get_state_events_by_type(&room_id, "m.room.guest_access")
        .await
        .ok()
        .and_then(|events| {
            events.into_iter().find(|event| event.get("state_key").and_then(Value::as_str) == Some("")).and_then(
                |event| {
                    event
                        .get("content")
                        .and_then(|content| content.get("guest_access"))
                        .and_then(Value::as_str)
                        .map(|value| value == "can_join")
                },
            )
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

    let membership = state.services.rooms.room_service.get_room_membership(&room_id, &auth_user.user_id).await?;

    if membership.is_none() {
        return Err(ApiError::not_found("Room not found or not a member".to_string()));
    }

    let room = state
        .services
        .rooms
        .room_service
        .get_room_record(&room_id)
        .await?
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
    let room_ids = state.services.rooms.room_service.get_joined_rooms(&auth_user.user_id).await?;

    Ok(Json(json!({
        "joined_rooms": room_ids
    })))
}

pub(crate) async fn get_my_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let room_list = state.services.rooms.room_service.get_user_room_list(&auth_user.user_id).await?;

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
    let request_id = resolve_request_id(&headers);
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _, _, _) = state.services.core.auth_service.validate_token(&token).await?;

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

    let invite = match body.get("invite") {
        Some(value) => {
            let Some(invites) = value.as_array() else {
                return Err(ApiError::invalid_param("invite must be an array".to_string()));
            };
            let mut invitees = Vec::with_capacity(invites.len());
            for invitee in invites {
                let Some(user_id) = invitee.as_str() else {
                    return Err(ApiError::invalid_param("invite entries must be strings".to_string()));
                };
                invitees.push(user_id.to_string());
            }
            Some(invitees)
        }
        None => None,
    };

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

    let result = state.services.rooms.room_service.create_room(&user_id, config.clone()).await?;

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
        if let Err(e) = state.services.rooms.space_service.create_space(space_request).await {
            ::tracing::error!(
                request_id = %request_id,
                user_id = %user_id,
                room_type = ?config.room_type,
                error = %e,
                "Failed to create space record"
            );
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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = state
        .services
        .rooms
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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
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
        .rooms
        .room_service
        .is_room_creator(&room_id, &auth_user.user_id)
        .await
        .map_err(map_internal!("Failed to check room creator"))?;

    if !is_creator {
        return Err(ApiError::forbidden("Only the room creator can update room visibility".to_string()));
    }

    let is_public = visibility == "public";

    state.services.rooms.room_service.set_room_directory(&room_id, is_public).await?;

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

    let rooms = state.services.rooms.room_service.get_joined_rooms(&user_id).await?;

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

    let new_room_id =
        state.services.rooms.room_service.upgrade_room(&room_id, &body.new_version, &auth_user.user_id).await?;

    Ok(Json(UpgradeRoomResponse { replacement_room: new_room_id }))
}

pub(crate) async fn forget_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    state.services.rooms.room_service.forget_room(&room_id, &auth_user.user_id).await?;
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
        .rooms
        .room_service
        .get_room_record(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let from = params
        .get("from")
        .and_then(|value| crate::common::parse_stream_token(value).or_else(|| value.parse::<i64>().ok()))
        .unwrap_or(0);
    let limit = params.get("limit").and_then(|value| value.parse::<i64>().ok()).unwrap_or(10).clamp(1, 100);

    let state_events = state
        .services
        .rooms
        .room_service
        .get_state_event_records(&room_id)
        .await?
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

    let members = state.services.rooms.room_service.get_room_members(&room_id, &auth_user.user_id).await?;
    let messages =
        state.services.rooms.room_service.get_room_messages(&room_id, &auth_user.user_id, from, limit, "b").await?;

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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .rooms
        .room_service
        .get_room_record(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let encryption_status = state.services.rooms.room_service.get_room_encryption_status(&room_id).await?;

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
            "encryption": encryption_status.is_encrypted,
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

    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let timeout = params.timeout.unwrap_or(30000);
    let full_state = params.full_state.unwrap_or(false);
    let since = params.since.as_deref();

    let result = state
        .services
        .rooms
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

pub(crate) async fn get_room_thread_by_id(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, thread_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    crate::web::routes::validate_event_id(&thread_id)?;

    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let request = crate::services::thread_service::GetThreadRequest {
        room_id: room_id.clone(),
        thread_id: thread_id.clone(),
        include_replies: true,
        reply_limit: Some(100),
    };

    let thread_detail = state.services.rooms.thread_service.get_thread(request, Some(&auth_user.user_id)).await?;

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

    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    state
        .services
        .core
        .account_data_service
        .set_room_account_data(&auth_user.user_id, &room_id, &data_type, &body)
        .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "data_type": data_type,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn get_room_account_data(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, data_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let result = state
        .services
        .core
        .account_data_service
        .get_room_account_data(&auth_user.user_id, &room_id, &data_type)
        .await?;

    match result {
        Some(data) => Ok(Json(data)),
        None => Err(ApiError::not_found("Room account data not found".to_string())),
    }
}

pub(crate) async fn get_room_turn_server(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let voip_service = &state.services.extensions.rtc_domain_service.infra;

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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .rooms
        .room_service
        .get_room_record(&room_id)
        .await?
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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let result = state
        .services
        .core
        .account_data_service
        .get_room_account_data_with_ts(&auth_user.user_id, &room_id, "m.room.vault_data")
        .await?;

    match result {
        Some((data, updated_ts)) => Ok(Json(json!({
            "room_id": room_id,
            "vault_data": data,
            "updated_ts": updated_ts
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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    state
        .services
        .core
        .account_data_service
        .set_room_account_data(&auth_user.user_id, &room_id, "m.room.vault_data", &body)
        .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "vault_data": body,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn get_room_rendered(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .rooms
        .room_service
        .get_room_record(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let events = state.services.rooms.room_service.get_room_events(&room_id, 20).await?;

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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let aliases = state.services.rooms.room_service.get_room_aliases(&room_id).await?;

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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let room = state
        .services
        .rooms
        .room_service
        .get_room_record(&room_id)
        .await?
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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }
    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let device = state
        .services
        .account
        .account_device_list_service
        .get_device(&device_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Device not found".to_string()))?;

    let is_member = state
        .services
        .rooms
        .room_service
        .get_room_membership(&room_id, &device.user_id)
        .await?
        .is_some_and(|membership| membership == "join");

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
        .rooms
        .room_service
        .get_room_record(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let aliases = state.services.rooms.room_service.get_room_aliases(&room_id).await?;

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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }
    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let rooms = if let Some(space) = state.services.rooms.space_service.get_space_by_room(&room_id).await? {
        let children = state.services.rooms.space_service.get_space_children(&space.space_id).await?;

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
        .rooms
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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
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

    let results = state.services.core.search_service.search_room_messages(&room_id, search_term, limit).await?;

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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let policy = state.services.admin.modules.retention_service.resolve_effective_policy(&room_id).await?;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "max_lifetime": policy.max_lifetime,
        "min_lifetime": policy.min_lifetime,
        "expire_on_clients": policy.is_expire_on_clients,
        "is_server_default": policy.is_server_default,
        "created_ts": policy.created_ts,
        "updated_ts": policy.updated_ts
    })))
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
