use super::{ensure_room_view_access, get_room_event, parse_room_messages_from_token};
use crate::common::{ApiError, ContentSanitizer};
use crate::map_internal;
use crate::storage::CreateEventParams;
use crate::web::routes::{validate_event_id, validate_room_id, AppState, AuthenticatedUser};
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub(crate) async fn get_single_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state.services.rooms.room_service.get_event(&room_id, &event_id).await?;

    Ok(Json(event))
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

    let event = state.services.rooms.room_service.get_event(&room_id, &event_id).await?;

    Ok(Json(json!({
        "event_id": event.get("event_id"),
        "room_id": event.get("room_id"),
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

    let root_event = state.services.rooms.room_service.get_event(&room_id, &event_id).await?;

    let mut replies_json = Vec::new();
    let mut reply_count = 0;
    let mut participants_json = Vec::new();

    if let Some(thread_root) = state
        .services
        .rooms
        .thread_service
        .get_thread_root_by_event(&room_id, &event_id)
        .await
        .map_err(map_internal!("Failed to get thread root"))?
    {
        let thread_id = thread_root.thread_id.clone().unwrap_or_default();
        if !thread_id.is_empty() {
            let replies = state
                .services
                .rooms
                .thread_service
                .get_thread_replies(&room_id, &thread_id, Some(100), None)
                .await
                .map_err(map_internal!("Failed to get thread replies"))?;
            reply_count = replies.len();

            if reply_count > 0 {
                participants_json = state
                    .services
                    .rooms
                    .thread_service
                    .get_thread_participants(&room_id, &thread_id)
                    .await
                    .map_err(map_internal!("Failed to get participants"))?;

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
    }

    Ok(Json(json!({
        "root": {
            "event_id": root_event.get("event_id"),
            "room_id": root_event.get("room_id"),
            "sender": root_event.get("sender"),
            "type": root_event.get("type"),
            "content": root_event.get("content"),
            "origin_server_ts": root_event.get("origin_server_ts"),
            "state_key": root_event.get("state_key")
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

    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(20);

    let _from = params.get("from").cloned();

    let notifications = state
        .services
        .admin
        .push_notification_service
        .get_room_notifications(&auth_user.user_id, &room_id, limit)
        .await
        .map_err(map_internal!("Database error"))?;

    let notifications_list: Vec<Value> = notifications
        .iter()
        .map(|n| {
            json!({
                "event_id": n.event_id,
                "room_id": n.room_id,
                "ts": n.ts,
                "profile_tag": n.notification_type,
                "read": n.is_read.unwrap_or(false),
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

pub(crate) async fn get_messages(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(&params);
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(10)
        .min(1000) as i64;
    let direction = params.get("dir").and_then(|v| v.as_str()).unwrap_or("b");

    let response = state
        .services
        .rooms
        .room_service
        .get_room_messages(&room_id, &auth_user.user_id, from, limit, direction)
        .await?;

    // Best-effort outbound backfill trigger: when paginating backwards
    // (`dir=b`) and the local DB returned fewer events than requested, the
    // room likely has federated history we haven't fetched yet.  Spawn an
    // async task to request historical events from federated peers without
    // blocking the current response — the client will pick up the new
    // events on its next `/messages` call.
    //
    // This mirrors Synapse's `FederationHandler.maybe_backfill` trigger
    // point in the `/messages` path, though we use a simpler "fewer than
    // requested" heuristic rather than Synapse's extremity-depth check.
    //
    // A per-room cooldown (60 s) prevents excessive federation requests
    // when a client retries backward pagination rapidly.
    if direction == "b" {
        let chunk_count = response.get("chunk").and_then(|c| c.as_array()).map_or(0, |a| a.len());
        if (chunk_count as i64) < limit {
            let room_id_clone = room_id.clone();
            let federation_client = state.services.federation.federation_client.clone();
            let room_service = state.services.rooms.room_service.clone();
            tokio::spawn(async move {
                // Rate-limit: skip if this room was backfilled recently.
                if !synapse_services::room::backfill::check_backfill_cooldown(&room_id_clone).await {
                    ::tracing::debug!(
                        room_id = %room_id_clone,
                        "Best-effort /messages backfill skipped: within cooldown window"
                    );
                    return;
                }
                if let Err(error) =
                    room_service.backfill_room_history(&federation_client, &room_id_clone, Some(50)).await
                {
                    ::tracing::warn!(
                        room_id = %room_id_clone,
                        error = %error,
                        "Best-effort /messages backfill failed"
                    );
                }
            });
        }
    }

    Ok(Json(response))
}

pub(crate) async fn send_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, txn_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let s = body.to_string();
    if s.len() > 65536 {
        return Err(ApiError::bad_request("Message content too long (max 64KB)".to_string()));
    }

    if !txn_id.is_empty() {
        let cache_key = format!("txn:{}:{}:{}", auth_user.user_id, room_id, txn_id);
        if let Ok(Some(cached)) = state.services.core.cache.get::<String>(&cache_key).await {
            if let Ok(event_id) = serde_json::from_str::<serde_json::Value>(&cached) {
                return Ok(Json(event_id));
            }
        }
    }

    state.services.core.auth_service.verify_message_event_write(&room_id, &auth_user.user_id, &event_type).await?;

    if event_type == "m.room.encrypted" {
        let is_encrypted = state.services.rooms.room_service.check_room_has_encryption(&room_id).await?;

        if !is_encrypted {
            return Err(ApiError::bad_request(
                "Cannot send encrypted message to a room where encryption is not enabled. Enable encryption first by sending an m.room.encryption state event.".to_string(),
            ));
        }
    }

    if event_type == "m.room.power_levels" {
        state.services.core.auth_service.verify_power_levels_change(&room_id, &auth_user.user_id, &body).await?;
    }

    let mut body = body;
    if event_type == "m.room.message" {
        let format = body.get("format").and_then(|v| v.as_str()).unwrap_or("");
        if format == "org.matrix.custom.html" {
            if let Some(html) = body.get("formatted_body").and_then(|v| v.as_str()) {
                let sanitizer = ContentSanitizer::default();
                let cleaned = sanitizer.sanitize(html);
                body["formatted_body"] = serde_json::Value::String(cleaned);
            }
        }
    }

    let result =
        state.services.rooms.room_service.send_message(&room_id, &auth_user.user_id, &event_type, &body).await?;

    if !txn_id.is_empty() {
        let cache_key = format!("txn:{}:{}:{}", auth_user.user_id, room_id, txn_id);
        if let Err(e) = state.services.core.cache.set(&cache_key, &result.to_string(), 3600).await {
            ::tracing::warn!("Failed to cache transaction ID dedup marker: {e}");
        }
    }

    Ok(Json(result))
}

pub(crate) async fn get_room_message_queue(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let pending_events = state.services.rooms.room_service.get_pending_events(&room_id, 100).await?;

    let pending_events_json: Vec<serde_json::Value> = pending_events
        .into_iter()
        .map(|event| {
            serde_json::json!({
                "event_id": event.event_id,
                "room_id": event.room_id,
                "user_id": event.user_id,
                "event_type": event.event_type,
                "origin_server_ts": event.origin_server_ts,
                "status": event.status
            })
        })
        .collect();

    let processing_count = state.services.rooms.room_service.count_events_by_status(&room_id, "processing").await;

    let failed_count = state.services.rooms.room_service.count_events_by_status(&room_id, "failed").await;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "queue": {
            "pending": pending_events_json,
            "pending_count": pending_events_json.len(),
            "processing_count": processing_count,
            "failed_count": failed_count
        },
        "status": {
            "healthy": failed_count < 100,
            "total_pending": pending_events_json.len() + processing_count as usize
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

    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(&params);
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as i64;
    let direction = params.get("dir").and_then(|v| v.as_str()).unwrap_or("b");

    Ok(Json(
        state
            .services
            .rooms
            .room_service
            .get_room_messages(&room_id, &auth_user.user_id, from, limit, direction)
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
        .rooms
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let (notification_count, highlight_count) =
        state.services.rooms.sync_service.room_unread_counts(&room_id, &auth_user.user_id).await?;

    Ok(Json(json!({
        "notification_count": notification_count,
        "highlight_count": highlight_count
    })))
}

pub(crate) async fn get_room_encrypted_events(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .rooms
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let encrypted_events = state
        .services
        .rooms
        .room_service
        .get_room_events_by_type(&room_id, "m.room.encrypted", 100)
        .await
        .map_err(map_internal!("Failed to get encrypted events"))?;

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

pub(crate) async fn get_room_event_perspective(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .rooms
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .rooms
        .room_service
        .get_room_events(&room_id, 100)
        .await
        .map_err(map_internal!("Failed to get room events"))?;

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
    crate::web::routes::validate_user_id(&user_id)?;
    if !state
        .services
        .rooms
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .rooms
        .room_service
        .get_room_events(&room_id, 200)
        .await
        .map_err(map_internal!("Failed to get room events"))?;

    let fragments: Vec<Value> = events
        .into_iter()
        .filter(|event| event.user_id == user_id)
        .map(|event| {
            json!({
                "event_id": event.event_id,
                "type": event.event_type,
                "snippet": event.content.get("body").and_then(|value: &serde_json::Value| value.as_str()),
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

pub(crate) async fn get_room_reduced_events(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !state
        .services
        .rooms
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .rooms
        .room_service
        .get_room_events(&room_id, 100)
        .await
        .map_err(map_internal!("Failed to get room events"))?;

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

pub(crate) async fn get_room_event_url(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    if !state
        .services
        .rooms
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state
        .services
        .rooms
        .room_service
        .get_event_record(&event_id)
        .await
        .map_err(map_internal!("Failed to get event"))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::bad_request("Event does not belong to this room".to_string()));
    }

    let content = event.content.as_object().cloned().unwrap_or_default();
    let mut urls: Vec<serde_json::Value> = Vec::new();

    if let Some(url) = content.get("url").and_then(|v: &serde_json::Value| v.as_str()) {
        urls.push(serde_json::json!({
            "type": "mxc",
            "url": url,
            "field": "url"
        }));
    }

    if let Some(info) = content.get("info").and_then(|v: &serde_json::Value| v.as_object()) {
        if let Some(thumbnail_url) = info.get("thumbnail_url").and_then(|v: &serde_json::Value| v.as_str()) {
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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let _event = state.services.rooms.room_service.get_event(&room_id, &event_id).await?;

    let device_id = body.get("device_id").and_then(|v| v.as_str()).unwrap_or("default");

    let default_key_id = format!("ed25519:{device_id}");
    let key_id = body.get("key_id").and_then(|v| v.as_str()).unwrap_or(&default_key_id);

    let signature = body
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("signature is required".to_string()))?;

    let algorithm = body.get("algorithm").and_then(|v| v.as_str()).map_or_else(
        || key_id.split(':').next().filter(|value| !value.is_empty()).unwrap_or("ed25519").to_string(),
        str::to_owned,
    );

    let created_ts = chrono::Utc::now().timestamp_millis();

    state
        .services
        .rooms
        .room_service
        .save_event_signature(&event_id, &auth_user.user_id, device_id, signature, key_id, &algorithm, created_ts)
        .await?;

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
    if !state.services.rooms.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let _event = state.services.rooms.room_service.get_event(&room_id, &event_id).await?;

    let signatures = state.services.rooms.room_service.get_event_signatures(&event_id).await?;

    let verify_user_id = body.get("user_id").and_then(|v| v.as_str());
    let verify_device_id = body.get("device_id").and_then(|v| v.as_str());

    let verified_signatures: Vec<serde_json::Value> = signatures
        .iter()
        .filter(|s| {
            verify_user_id.is_none_or(|uid| s.user_id == uid) && verify_device_id.is_none_or(|did| s.device_id == did)
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

pub(crate) async fn translate_room_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = get_room_event(&state, &room_id, &event_id).await?;
    let source_text = event.get("content").and_then(|c| c.get("body")).and_then(|value| value.as_str()).unwrap_or("");

    // Extract target language from request body, falling back to config default
    let target_lang = body
        .get("target_lang")
        .and_then(|v| v.as_str())
        .unwrap_or(&state.services.core.config.translate.default_target_lang);

    // Extract optional source language from request body
    let source_lang = body.get("source_lang").and_then(|v| v.as_str());

    // Use the text field if provided, otherwise use the event body
    let text_to_translate = body.get("text").and_then(|v| v.as_str()).unwrap_or(source_text);

    // Call the translation service
    let translation_result = state
        .services
        .extensions
        .translation_service
        .translate(text_to_translate, target_lang, source_lang)
        .await
        .map_err(|e| {
            ::tracing::warn!(
                request_id = %request_id,
                room_id = %room_id,
                event_id = %event_id,
                error = %e,
                "Translation failed"
            );
            ApiError::bad_request(format!("Translation failed: {}", e))
        })?;

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event_id,
        "source_text": source_text,
        "translated_text": translation_result.translated_text,
        "detected_source_lang": translation_result.detected_source_lang,
        "target_lang": translation_result.target_lang,
        "provider": translation_result.provider
    })))
}

pub(crate) async fn translate_text(
    State(state): State<AppState>,
    headers: HeaderMap,
    _auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let text = body.get("text").and_then(|v| v.as_str()).unwrap_or("");

    if text.is_empty() {
        return Ok(Json(json!({
            "translated_text": "",
            "detected_source_lang": null,
            "target_lang": "",
            "provider": "passthrough"
        })));
    }

    // Validate text length
    let max_len = state.services.core.config.translate.max_text_length;
    if text.len() > max_len {
        return Err(ApiError::bad_request(format!("Text too long: {} bytes (max: {})", text.len(), max_len)));
    }

    let target_lang = body
        .get("target_lang")
        .and_then(|v| v.as_str())
        .unwrap_or(&state.services.core.config.translate.default_target_lang);

    let source_lang = body.get("source_lang").and_then(|v| v.as_str());

    let translation_result =
        state.services.extensions.translation_service.translate(text, target_lang, source_lang).await.map_err(|e| {
            ::tracing::warn!(
                request_id = %request_id,
                target_lang = %target_lang,
                error = %e,
                "Translation failed"
            );
            ApiError::bad_request(format!("Translation failed: {}", e))
        })?;

    Ok(Json(json!({
        "translated_text": translation_result.translated_text,
        "detected_source_lang": translation_result.detected_source_lang,
        "target_lang": translation_result.target_lang,
        "provider": translation_result.provider
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

    let event = get_room_event(&state, &room_id, &event_id).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event.get("event_id"),
        "converted": {
            "type": event.get("type"),
            "content": event.get("content"),
            "sender": event.get("sender"),
            "origin_server_ts": event.get("origin_server_ts")
        }
    })))
}

pub(crate) async fn redact_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, _txn_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;
    if !event_id.starts_with('$') {
        return Ok(Json(json!({
            "event_id": event_id
        })));
    }
    validate_event_id(&event_id)?;

    let original_event = state
        .services
        .rooms
        .room_service
        .get_event_record(&event_id)
        .await
        .map_err(map_internal!("Failed to get event"))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if original_event.room_id != room_id {
        return Err(ApiError::bad_request("Event does not belong to this room".to_string()));
    }

    state.services.core.auth_service.can_redact_event(&room_id, &auth_user.user_id, &original_event.user_id).await?;

    let reason = body.get("reason").and_then(|v| v.as_str());

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.core.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    // P0-05: redaction events must carry the target event_id in the `redacts`
    // field.  For room versions 1-10 this is a top-level PDU field (stored in
    // the `events.redacts` column); for v11+ it would live in
    // `content.redacts` (MSC2174/MSC3820), but v11+ creation is disabled until
    // the redaction chain is fully landed.
    let content = json!({
        "reason": reason
    });
    let user_id_for_as = auth_user.user_id.clone();
    let content_for_as = content.clone();
    let redactor_user_id = auth_user.user_id.clone();

    state
        .services
        .rooms
        .room_service
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.redaction".to_string(),
                content,
                state_key: None,
                origin_server_ts: now,
                redacts: Some(event_id.clone()),
            },
            None,
        )
        .await
        .map_err(map_internal!("Failed to redact event"))?;
    state
        .services
        .rooms
        .room_service
        .dispatch_appservice_event(&new_event_id, &room_id, "m.room.redaction", &user_id_for_as, &content_for_as, None)
        .await;

    state.services.rooms.room_service.redact_event_content(&event_id, Some(&redactor_user_id)).await.map_err(|e| {
        ::tracing::warn!(
            target: "security_audit",
            request_id = %request_id,
            event = "redaction_content_failed",
            room_id = %room_id,
            event_id = %event_id,
            error = %e,
            "Redaction event created but content redaction failed"
        );
        ApiError::internal_with_log("Failed to redact event content", &e)
    })?;

    Ok(Json(json!({
        "event_id": new_event_id
    })))
}
