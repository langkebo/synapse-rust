use super::super::ensure_room_view_access;
use crate::routes::extractors::RoomId;
use crate::routes::{validate_room_id, AuthenticatedUser};
use axum::extract::{Json, Path, Query, State};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::types::DeviceId;
use synapse_common::ApiError;

use crate::routes::context::RoomContext;

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

/// The `RoomSyncQueryDto` struct.
#[derive(Debug, Deserialize, Default)]
pub(crate) struct RoomSyncQueryDto {
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    timeout: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_optional_bool")]
    full_state: Option<bool>,
    #[serde(default)]
    since: Option<String>,
}

/// See [`room_initial_sync`].
pub(crate) async fn room_initial_sync(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await.map_err(|e| ApiError::from(e))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let from = params.get("from").and_then(|value| {
        synapse_common::parse_pagination_token(value)
            .or_else(|| synapse_common::parse_stream_token(value).map(|ts| (ts, None)))
            .or_else(|| value.parse::<i64>().ok().map(|ts| (ts, None)))
    });
    let limit = params.get("limit").and_then(|value| value.parse::<i64>().ok()).unwrap_or(10).clamp(1, 100);

    let state_events = ctx
        .room_service
        .messaging()
        .get_state_event_records(&room_id)
        .await.map_err(|e| ApiError::from(e))?
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

    let members = ctx.room_service.membership().get_room_members(&room_id, &auth_user.user_id).await.map_err(|e| ApiError::from(e))?;
    let messages =
        ctx.room_service.messaging().get_room_messages(&room_id, &auth_user.user_id, from, limit, "b").await.map_err(|e| ApiError::from(e))?;

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

/// See [`get_room_sync`].
pub(crate) async fn get_room_sync(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(params): Query<RoomSyncQueryDto>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let timeout = params.timeout.unwrap_or(30000);
    let full_state = params.full_state.unwrap_or(false);
    let since = params.since.as_deref();

    let result =
        ctx.sync_service.room_sync_with_timeout(&auth_user.user_id, &room_id, timeout, full_state, since).await.map_err(|e| ApiError::from(e))?;

    Ok(Json(result))
}

/// See [`get_room_capabilities`].
pub(crate) async fn get_room_capabilities(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await.map_err(|e| ApiError::from(e))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let encryption_status = ctx.room_service.state().get_room_encryption_status(&room_id).await.map_err(|e| ApiError::from(e))?;

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

/// See [`get_room_thread_by_id`].
pub(crate) async fn get_room_thread_by_id(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    crate::routes::validate_event_id(&thread_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let request = synapse_services::thread_service::GetThreadRequest {
        room_id: room_id.to_string(),
        thread_id: thread_id.clone(),
        include_replies: true,
        reply_limit: Some(100),
    };

    let thread_detail = ctx.thread_service.get_thread(request, Some(&auth_user.user_id)).await.map_err(|e| ApiError::from(e))?;

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

/// See [`get_room_turn_server`].
pub(crate) async fn get_room_turn_server(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let voip_service = &ctx.rtc_domain_service.infra;

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

/// See [`get_room_metadata`].
pub(crate) async fn get_room_metadata(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await.map_err(|e| ApiError::from(e))?
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

/// See [`get_room_vault_data`].
pub(crate) async fn get_room_vault_data(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let result = ctx
        .account_data_service
        .get_room_account_data_with_ts(&auth_user.user_id, &room_id, "m.room.vault_data")
        .await.map_err(|e| ApiError::from(e))?;

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

/// See [`set_room_vault_data`].
pub(crate) async fn set_room_vault_data(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    ctx.account_data_service.set_room_account_data(&auth_user.user_id, &room_id, "m.room.vault_data", &body).await.map_err(|e| ApiError::from(e))?;

    Ok(Json(json!({
        "room_id": room_id,
        "vault_data": body,
        "updated_ts": current_timestamp_millis()
    })))
}

/// See [`get_room_rendered`].
pub(crate) async fn get_room_rendered(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await.map_err(|e| ApiError::from(e))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let events = ctx.room_service.messaging().get_room_events(&room_id, 20).await.map_err(|e| ApiError::from(e))?;

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

/// See [`get_room_external_ids`].
pub(crate) async fn get_room_external_ids(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let aliases = ctx.room_service.state().get_room_aliases(&room_id).await.map_err(|e| ApiError::from(e))?;

    let external_ids: Vec<Value> =
        aliases.into_iter().map(|alias| json!({"type": "room_alias", "value": alias})).collect();

    Ok(Json(json!({
        "room_id": room_id,
        "external_ids": external_ids
    })))
}

/// See [`get_room_service_types`].
pub(crate) async fn get_room_service_types(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await.map_err(|e| ApiError::from(e))?
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

/// See [`get_room_device`].
pub(crate) async fn get_room_device(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, device_id)): Path<(String, DeviceId)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }
    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let device = ctx
        .account_device_list_service
        .get_device(device_id.as_str())
        .await.map_err(|e| ApiError::from(e))?
        .ok_or_else(|| ApiError::not_found("Device not found".to_string()))?;

    let is_member = ctx
        .room_service
        .membership()
        .get_room_membership(&room_id, &device.user_id)
        .await.map_err(|e| ApiError::from(e))?
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

/// See [`get_room_resolve`].
pub(crate) async fn get_room_resolve(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await.map_err(|e| ApiError::from(e))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let aliases = ctx.room_service.state().get_room_aliases(&room_id).await.map_err(|e| ApiError::from(e))?;

    Ok(Json(json!({
        "room_id": room_id,
        "aliases": aliases,
        "name": room.name,
        "canonical_alias": room.canonical_alias
    })))
}

/// See [`get_room_spaces`].
pub(crate) async fn get_room_spaces(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }
    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let rooms = if let Some(space) = ctx.space_service.get_space_by_room(&room_id).await.map_err(|e| ApiError::from(e))? {
        let children = ctx.space_service.get_space_children(&space.space_id).await.map_err(|e| ApiError::from(e))?;

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

    let spaces = ctx
        .space_service
        .get_parent_spaces(&room_id)
        .await.map_err(|e| ApiError::from(e))?
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

/// See [`search_room_messages`].
pub(crate) async fn search_room_messages(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

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

    let results = ctx.search_service.search_room_messages(&room_id, search_term, limit).await.map_err(|e| ApiError::from(e))?;

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

/// See [`get_retention_policy`].
pub(crate) async fn get_retention_policy(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(|e| ApiError::from(e))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await.map_err(|e| ApiError::from(e))?;

    let policy = ctx.retention_service.resolve_effective_policy(&room_id).await.map_err(|e| ApiError::from(e))?;

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

// =============================================================================
// Query parameter deserialization helpers for RoomSyncQueryDto
// =============================================================================

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
