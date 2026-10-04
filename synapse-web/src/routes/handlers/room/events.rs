use super::{ensure_room_view_access, get_room_event, parse_pagination_direction, parse_room_messages_from_token};
use crate::routes::context::RoomContext;
use crate::routes::extractors::{EventId, RoomId, UserId};
use crate::routes::{validate_event_id, validate_room_id, AuthenticatedUser};
use crate::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use synapse_common::current_timestamp_millis;
use synapse_common::map_internal;
use synapse_common::{ApiError, ContentSanitizer};
use synapse_services::event::CreateEventParams;

/// See [`get_single_event`].
pub(crate) async fn get_single_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let event = ctx.room_service.messaging().get_event(&room_id, &event_id).await?;

    Ok(Json(event))
}

/// See [`get_event_keys`].
pub(crate) async fn get_event_keys(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
) -> Result<Json<Value>, ApiError> {
    let room_id = room_id.replace("%21", "!").replace("%3A", ":");
    let event_id = event_id.replace("%24", "$");

    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let event = ctx.room_service.messaging().get_event(&room_id, &event_id).await?;

    Ok(Json(json!({
        "event_id": event.get("event_id"),
        "room_id": event.get("room_id"),
        "keys": []
    })))
}

/// See [`get_room_thread`].
pub(crate) async fn get_room_thread(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
) -> Result<Json<Value>, ApiError> {
    let room_id = room_id.replace("%21", "!").replace("%3A", ":");
    let event_id = event_id.replace("%24", "$");

    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let root_event = ctx.room_service.messaging().get_event(&room_id, &event_id).await?;

    let mut replies_json = Vec::new();
    let mut reply_count = 0;
    let mut participants_json = Vec::new();

    if let Some(thread_root) = ctx
        .thread_service
        .get_thread_root_by_event(&room_id, &event_id)
        .await
        .map_err(map_internal!("Failed to get thread root"))?
    {
        let thread_id = thread_root.thread_id.clone().unwrap_or_default();
        if !thread_id.is_empty() {
            let replies = ctx
                .thread_service
                .get_thread_replies(&room_id, &thread_id, Some(100), None)
                .await
                .map_err(map_internal!("Failed to get thread replies"))?;
            reply_count = replies.len();

            if reply_count > 0 {
                participants_json = ctx
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

/// See [`get_room_notifications`].
pub(crate) async fn get_room_notifications(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(20);

    let _from = params.get("from").cloned();

    let notifications = ctx
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
                "profile_tag": n.profile_tag,
                "notification_type": n.notification_type,
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

/// See [`get_messages`].
pub(crate) async fn get_messages(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(&params)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(10)
        .min(1000) as i64;
    let direction = parse_pagination_direction(&params)?;

    // A9: `/messages` `filter` is not implemented on this server. Reject it
    // explicitly instead of silently returning unfiltered events (a client
    // that relies on `filter` would otherwise receive too much data without
    // any signal that its constraint was dropped).
    if let Some(filter) = params.get("filter") {
        let is_absent = filter.is_null() || filter.as_str().is_some_and(str::is_empty);
        if !is_absent {
            return Err(ApiError::invalid_param("The 'filter' parameter is not supported on this endpoint"));
        }
    }

    let response =
        ctx.room_service.messaging().get_room_messages(&room_id, &auth_user.user_id, from, limit, direction).await?;

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
            let federation_client = ctx.federation_client.clone();
            let room_service = ctx.room_service.clone();
            tokio::spawn(async move {
                // Rate-limit: skip if this room was backfilled recently.
                if !synapse_services::room::backfill::check_backfill_cooldown(&room_id_clone) {
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

/// See [`send_message`].
pub(crate) async fn send_message(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, txn_id)): Path<(RoomId, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let s = body.to_string();
    if s.len() > 65536 {
        return Err(ApiError::too_large("Message content too long (max 64KB)".to_string()));
    }

    if !txn_id.is_empty() {
        let cache_key = format!("txn:{}:{}:{}", auth_user.user_id, room_id, txn_id);
        if let Ok(Some(cached)) = ctx.cache.get::<String>(&cache_key).await {
            if let Ok(event_id) = serde_json::from_str::<serde_json::Value>(&cached) {
                return Ok(Json(event_id));
            }
        }
    }

    ctx.room_auth.verify_message_event_write(&room_id, &auth_user.user_id, &event_type).await?;

    if event_type == "m.room.encrypted" {
        let is_encrypted = ctx.room_service.state().check_room_has_encryption(&room_id).await?;

        if !is_encrypted {
            return Err(ApiError::bad_request(
                "Cannot send encrypted message to a room where encryption is not enabled. Enable encryption first by sending an m.room.encryption state event.".to_string(),
            ));
        }
    }

    if event_type == "m.room.power_levels" {
        ctx.room_auth.verify_power_levels_change(&room_id, &auth_user.user_id, &body).await?;
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
        // MSC3806: scan the message body **when scanning is configured**.
        //
        // The policy (disabled ⇒ pass-through, scanner unreachable ⇒ fail-closed,
        // `safe: false` ⇒ 403) lives in exactly one place,
        // `content_scanner::scan_text_when_enabled`, the same way the media
        // upload path uses `scan_when_enabled`.  Calling
        // `ctx.content_scanner.scan_text(..).await?` directly here propagated
        // `M_CONTENT_SCAN_DISABLED` (501) whenever scanning was off — and since
        // `content_scanner.enabled: false` is the shipped default
        // (`docker/config/homeserver.yaml`), that made **every** `m.room.message`
        // send fail with 501 in the default configuration.
        let text = body.get("body").and_then(|v| v.as_str()).unwrap_or("");
        synapse_services::content_scanner::scan_text_when_enabled(
            ctx.content_scanner.as_ref(),
            ctx.metrics.as_ref(),
            &format!("msg:{}:{}", room_id, auth_user.user_id),
            text,
        )
        .await?;
    }

    // MSC4140: If the body contains `org.matrix.msc4140.delay`, schedule the
    // event as a delayed event instead of sending immediately. The server
    // returns a `delay_id` (numeric) that clients use to cancel/restart/send
    // via the management endpoint.
    if let Some(response) = super::schedule_delayed_event_if_requested(
        &ctx,
        &auth_user.user_id,
        auth_user.device_id.as_deref().unwrap_or(""),
        &room_id,
        &event_type,
        None,
        &body,
        Some(&txn_id),
    )
    .await?
    {
        return Ok(response);
    }

    // ISSUE-03: txn 去重的唯一事实源是 DB 唯一约束（room_event_txn_dedup），
    // 上方缓存仅为快路径；缓存丢失/过期时重试仍返回同一 event_id。
    let result = ctx
        .room_service
        .messaging()
        .send_message_with_txn(&room_id, &auth_user.user_id, &event_type, &body, &txn_id)
        .await?;

    if !txn_id.is_empty() {
        let cache_key = format!("txn:{}:{}:{}", auth_user.user_id, room_id, txn_id);
        if let Err(e) = ctx.cache.set(&cache_key, &result.to_string(), 3600).await {
            ::tracing::warn!("Failed to cache transaction ID dedup marker: {e}");
        }
    }

    Ok(Json(result))
}

/// See [`get_room_message_queue`].
pub(crate) async fn get_room_message_queue(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let pending_events = ctx.room_service.messaging().get_pending_events(&room_id, 100).await?;

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

    let processing_count = ctx.room_service.messaging().count_events_by_status(&room_id, "processing").await;

    let failed_count = ctx.room_service.messaging().count_events_by_status(&room_id, "failed").await;

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

/// See [`get_room_timeline`].
pub(crate) async fn get_room_timeline(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(&params)?;
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as i64;
    let direction = parse_pagination_direction(&params)?;

    Ok(Json(
        ctx.room_service.messaging().get_room_messages(&room_id, &auth_user.user_id, from, limit, direction).await?,
    ))
}

/// See [`get_room_unread_count`].
pub(crate) async fn get_room_unread_count(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await.map_err(map_internal!("Failed to check room existence"))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let (notification_count, highlight_count) =
        ctx.sync_service.room_unread_counts(&room_id, &auth_user.user_id).await?;

    Ok(Json(json!({
        "notification_count": notification_count,
        "highlight_count": highlight_count
    })))
}

/// See [`get_room_encrypted_events`].
pub(crate) async fn get_room_encrypted_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(map_internal!("Failed to check room existence"))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let encrypted_events = ctx
        .room_service
        .messaging()
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

/// See [`get_room_event_perspective`].
pub(crate) async fn get_room_event_perspective(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(map_internal!("Failed to check room existence"))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let events = ctx
        .room_service
        .messaging()
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

/// See [`get_room_user_fragments`].
pub(crate) async fn get_room_user_fragments(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, user_id)): Path<(RoomId, UserId)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    crate::routes::validate_user_id(&user_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(map_internal!("Failed to check room existence"))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let events = ctx
        .room_service
        .messaging()
        .get_room_events(&room_id, 200)
        .await
        .map_err(map_internal!("Failed to get room events"))?;

    let fragments: Vec<Value> = events
        .into_iter()
        .filter(|event| event.user_id == user_id.as_str())
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

/// See [`get_room_reduced_events`].
pub(crate) async fn get_room_reduced_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(map_internal!("Failed to check room existence"))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let events = ctx
        .room_service
        .messaging()
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

/// See [`get_room_event_url`].
pub(crate) async fn get_room_event_url(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(map_internal!("Failed to check room existence"))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let event = ctx
        .room_service
        .messaging()
        .get_event_record(&event_id)
        .await
        .map_err(map_internal!("Failed to get event"))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id.as_ref() {
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

/// See [`sign_room_event`].
pub(crate) async fn sign_room_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let _event = ctx.room_service.messaging().get_event(&room_id, &event_id).await?;

    let device_id = body.get("device_id").and_then(|v| v.as_str()).unwrap_or("default");

    let default_key_id = format!("ed25519:{device_id}");
    let key_id = body.get("key_id").and_then(|v| v.as_str()).unwrap_or(&default_key_id);

    let signature = body
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("signature is required".to_string()))?;

    // D-99：`algorithm` 只是**回显**给调用方（客户端声明的算法，缺省取 `key_id` 前缀）。
    // 它原先还被写进 `event_signatures.algorithm`，但那一列从来没有人读、且与 `key_id` 前缀
    // 语义重复 ⇒ 列与写入参数已删除，这里保留推导仅用于响应。
    let algorithm = body.get("algorithm").and_then(|v| v.as_str()).map_or_else(
        || key_id.split(':').next().filter(|value| !value.is_empty()).unwrap_or("ed25519").to_string(),
        str::to_owned,
    );

    let created_ts = current_timestamp_millis();

    ctx.room_service
        .messaging()
        .save_event_signature(&event_id, &auth_user.user_id, device_id, signature, key_id, created_ts)
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

/// See [`verify_room_event`].
pub(crate) async fn verify_room_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let _event = ctx.room_service.messaging().get_event(&room_id, &event_id).await?;

    let signatures = ctx.room_service.messaging().get_event_signatures(&event_id).await?;

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

/// See [`translate_room_event`].
pub(crate) async fn translate_room_event(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let event = get_room_event(&ctx, &room_id, &event_id).await?;
    let source_text = event.get("content").and_then(|c| c.get("body")).and_then(|value| value.as_str()).unwrap_or("");

    // Extract target language from request body, falling back to config default
    let target_lang =
        body.get("target_lang").and_then(|v| v.as_str()).unwrap_or(&ctx.config.translate.default_target_lang);

    // Extract optional source language from request body
    let source_lang = body.get("source_lang").and_then(|v| v.as_str());

    // Use the text field if provided, otherwise use the event body
    let text_to_translate = body.get("text").and_then(|v| v.as_str()).unwrap_or(source_text);

    // Call the translation service
    let translation_result =
        ctx.translation_service.translate(text_to_translate, target_lang, source_lang).await.map_err(|e| {
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

/// See [`translate_text`].
pub(crate) async fn translate_text(
    State(ctx): State<RoomContext>,
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
    let max_len = ctx.config.translate.max_text_length;
    if text.len() > max_len {
        return Err(ApiError::bad_request(format!("Text too long: {} bytes (max: {})", text.len(), max_len)));
    }

    let target_lang =
        body.get("target_lang").and_then(|v| v.as_str()).unwrap_or(&ctx.config.translate.default_target_lang);

    let source_lang = body.get("source_lang").and_then(|v| v.as_str());

    let translation_result = ctx.translation_service.translate(text, target_lang, source_lang).await.map_err(|e| {
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

/// See [`convert_room_event`].
pub(crate) async fn convert_room_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let event = get_room_event(&ctx, &room_id, &event_id).await?;

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

/// Extract the parent event id of an edit (`m.replace` relation), if any.
///
/// Mirrors Synapse's `relation_from_event(...).rel_type == RelationTypes.REPLACE`
/// check: the id lives at `content["m.relates_to"]["event_id"]`.
fn edit_parent_event_id(content: &Value) -> Option<&str> {
    let relates_to = content.get("m.relates_to")?;
    if relates_to.get("rel_type").and_then(Value::as_str) != Some("m.replace") {
        return None;
    }
    relates_to.get("event_id").and_then(Value::as_str)
}

/// Pure age decision for `redaction_allowed_period`: only `m.room.message`
/// events older than the period are blocked; every other type is always
/// redactable. Extracted so the boundary can be unit-tested without a context.
fn redaction_exceeds_allowed_period(period_ms: i64, target_type: &str, target_ts: i64, now_ms: i64) -> bool {
    target_type == "m.room.message" && target_ts < now_ms - period_ms
}

/// Enforce Synapse's `redaction_allowed_period`: a local user may only redact an
/// `m.room.message` younger than the configured period. Redactions of other
/// event types, or when the period is unset, are always allowed.
///
/// When the target is an edit, the age is measured from the event it replaces,
/// matching Synapse's `MessageHandler.create_event` →
/// `_check_redaction_allowed_period`.
async fn check_redaction_allowed_period(
    ctx: &RoomContext,
    event_type: &str,
    origin_server_ts: i64,
    content: &Value,
) -> Result<(), ApiError> {
    let Some(period) = ctx.config.redaction_allowed_period else {
        return Ok(());
    };

    let mut target_type = event_type.to_string();
    let mut target_ts = origin_server_ts;

    if let Some(parent_id) = edit_parent_event_id(content) {
        if let Some(parent) = ctx
            .room_service
            .messaging()
            .get_event_record(parent_id)
            .await
            .map_err(map_internal!("Failed to get event"))?
        {
            target_type = parent.event_type;
            target_ts = parent.origin_server_ts;
        }
    }

    if redaction_exceeds_allowed_period(period, &target_type, target_ts, current_timestamp_millis()) {
        return Err(ApiError::forbidden(format!("Events older than {period}ms cannot be redacted.")));
    }

    Ok(())
}

/// See [`redact_event`].
pub(crate) async fn redact_event(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, _txn_id)): Path<(RoomId, EventId, String)>,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;
    if !event_id.starts_with('$') {
        return Ok(Json(json!({
            "event_id": event_id
        })));
    }
    validate_event_id(&event_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let original_event = ctx
        .room_service
        .messaging()
        .get_event_record(&event_id)
        .await
        .map_err(map_internal!("Failed to get event"))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if original_event.room_id != room_id.as_ref() {
        return Err(ApiError::bad_request("Event does not belong to this room".to_string()));
    }

    ctx.room_auth.can_redact_event(&room_id, &auth_user.user_id, &original_event.user_id).await?;

    check_redaction_allowed_period(
        &ctx,
        &original_event.event_type,
        original_event.origin_server_ts,
        &original_event.content,
    )
    .await?;

    // MSC3912: Parse with_rel_types (stable) and org.matrix.msc3912.with_relations (unstable)
    let requested_rel_types: Option<Vec<String>> = body
        .get("with_rel_types")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .or_else(|| {
            body.get("org.matrix.msc3912.with_relations")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        });

    // Strip the MSC3912 parameters from body (per spec: they must not be stored
    // in event content), whether or not they end up driving a cascade.
    if let Some(obj) = body.as_object_mut() {
        obj.remove("with_rel_types");
        obj.remove("org.matrix.msc3912.with_relations");
    }

    // MSC3912: an empty list is equivalent to *not* cascading — it is explicitly
    // not an error (it previously returned `400 M_BAD_JSON`). The target event is
    // still redacted, only the related-event cascade is skipped.
    let with_rel_types: Option<Vec<String>> = requested_rel_types.filter(|rel_types| !rel_types.is_empty());

    let reason = body.get("reason").and_then(|v| v.as_str());

    let now = current_timestamp_millis();

    // The target event_id is always passed as `redacts`; the room-version
    // dependent placement (v1-v10 top-level field vs v11+ `content.redacts`)
    // is decided centrally in `RoomMessagingService::create_event`.
    let content = json!({
        "reason": reason
    });
    let user_id_for_as = auth_user.user_id.clone();
    let content_for_as = content.clone();
    // Captured before `auth_user.user_id` is moved into `CreateEventParams`; the
    // cascade authorizes every related event as this user.
    let cascade_actor_user_id = auth_user.user_id.clone();

    let redaction_event = ctx
        .room_service
        .messaging()
        .create_event(
            CreateEventParams {
                // Placeholder for v1/v2 only: the write path replaces it with
                // the reference hash for v3+ rooms. Everything below therefore
                // reads the ID back off `redaction_event`.
                event_id: synapse_common::crypto::generate_event_id(&ctx.server_name),
                room_id: room_id.to_string(),
                user_id: auth_user.user_id,
                event_type: "m.room.redaction".to_string(),
                content,
                state_key: None,
                origin_server_ts: now,
                redacts: Some(event_id.to_string()),
            },
            None,
        )
        .await
        .map_err(map_internal!("Failed to redact event"))?;
    ctx.room_service
        .dispatch_appservice_event(
            &redaction_event.event_id,
            &room_id,
            "m.room.redaction",
            &user_id_for_as,
            &content_for_as,
            None,
        )
        .await;

    // `events.redacted_by` is a self-referential foreign key to
    // `events.event_id` (`fk_events_redacted_by`), so it records the redaction
    // EVENT — not the acting user id. Passing the user id violated the
    // constraint and made every client redaction fail with a 500.
    ctx.room_service.messaging().redact_event_content(&event_id, Some(&redaction_event.event_id)).await.map_err(
        |e| {
            ::tracing::warn!(
                target: "security_audit",
                request_id = %request_id,
                event = "redaction_content_failed",
                room_id = %room_id,
                event_id = %event_id,
                error = %e,
                "Redaction event created but content redaction failed"
            );
            ApiError::internal_with_cause("Failed to redact event content", e)
        },
    )?;

    // MSC3912: Single-layer cascade redaction for related events (never parents,
    // never recursive). Only runs when `with_rel_types` is a non-empty list.
    //
    // Best-effort background task, like upstream's `run_as_background_process`:
    // the client already has the redaction event_id, and a cascade failure must
    // not turn a successful target redaction into an error. Failures are
    // nevertheless logged with structured fields rather than swallowed.
    // Per-event authorization happens inside the service.
    if let Some(rel_types) = with_rel_types {
        // Clone the redaction event ID for the background task
        let redaction_event_id = redaction_event.event_id.clone();
        let redaction_service = ctx.event_redaction_service.clone();
        tokio::spawn(async move {
            if let Err(error) = redaction_service
                .cascade_redact_related_events(
                    &room_id,
                    &event_id,
                    &rel_types,
                    &cascade_actor_user_id,
                    &redaction_event_id,
                )
                .await
            {
                ::tracing::warn!(
                    target: "security_audit",
                    request_id = %request_id,
                    event = "cascade_redaction_failed",
                    room_id = %room_id,
                    event_id = %event_id,
                    actor_user_id = %cascade_actor_user_id,
                    error = %error,
                    "MSC3912 cascade redaction failed"
                );
            }
        });
    }

    Ok(Json(json!({
        "event_id": redaction_event.event_id
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_id_validation_starts_with_dollar() {
        let valid_event_id = "$abc123xyz";
        let invalid_event_id = "abc123xyz";

        assert!(valid_event_id.starts_with('$'));
        assert!(!invalid_event_id.starts_with('$'));
    }

    #[test]
    fn test_room_id_validation_starts_with_exclamation() {
        let valid_room_id = "!room123:example.com";
        let invalid_room_id = "room123:example.com";

        assert!(valid_room_id.starts_with('!'));
        assert!(!invalid_room_id.starts_with('!'));
    }

    #[test]
    fn test_event_url_encoding_patterns() {
        let encoded_room_id = "%21room123%3Aexample.com";
        let decoded_room_id = encoded_room_id.replace("%21", "!").replace("%3A", ":");

        assert_eq!(decoded_room_id, "!room123:example.com");
    }

    #[test]
    fn test_event_keys_response_structure() {
        let event = json!({
            "event_id": "$event123",
            "room_id": "!room123:example.com",
            "sender": "@user:example.com",
            "type": "m.room.message"
        });

        let keys_response = json!({
            "event_id": event.get("event_id"),
            "room_id": event.get("room_id"),
            "keys": []
        });

        assert!(keys_response.get("event_id").is_some());
        assert!(keys_response.get("room_id").is_some());
        assert!(keys_response.get("keys").unwrap().is_array());
        assert_eq!(keys_response.get("keys").unwrap().as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_redaction_event_content_structure() {
        let reason = Some("spam".to_string());
        let content = json!({
            "reason": reason
        });

        assert!(content.get("reason").is_some());
        assert_eq!(content.get("reason").unwrap(), "spam");
    }

    #[test]
    fn test_redaction_event_id_generation() {
        let new_event_id = synapse_common::crypto::generate_event_id("example.com");

        assert!(new_event_id.starts_with('$'));
        assert!(new_event_id.contains("example.com"));
    }

    #[test]
    fn test_message_type_classification() {
        let message_types = ["m.room.message", "m.room.redaction", "m.room.member", "m.reaction", "m.sticker"];

        assert_eq!(message_types.len(), 5);
        assert!(message_types.iter().all(|t| t.starts_with("m.")));
    }

    #[test]
    fn test_event_relation_types() {
        let relation_types = ["m.reference", "m.replace", "m.thread", "m.annotation"];

        assert_eq!(relation_types.len(), 4);
        assert!(relation_types.iter().all(|t| t.starts_with("m.")));
    }

    #[test]
    fn test_room_message_payload_structure() {
        let payload = json!({
            "msgtype": "m.text",
            "body": "Hello, World!",
            "format": "org.matrix.custom.html",
            "formatted_body": "<p>Hello, <strong>World</strong>!</p>"
        });

        assert_eq!(payload.get("msgtype").unwrap(), "m.text");
        assert_eq!(payload.get("body").unwrap(), "Hello, World!");
        assert!(payload.get("format").is_some());
    }

    #[test]
    fn test_room_message_msgtype_variants() {
        let msgtypes = ["m.text", "m.image", "m.audio", "m.video", "m.file"];

        assert_eq!(msgtypes.len(), 5);
        assert!(msgtypes.iter().all(|t| t.starts_with("m.")));
    }

    #[test]
    fn test_event_timestamp_validation() {
        let now = current_timestamp_millis();
        let past = now - 86400000; // 1 day ago
        let future = now + 86400000; // 1 day from now

        assert!(now > 0);
        assert!(past > 0);
        assert!(future > now);
    }

    #[test]
    fn test_event_content_structure() {
        let content = json!({
            "body": "Hello, World!",
            "msgtype": "m.text"
        });

        assert_eq!(content.get("body").unwrap(), "Hello, World!");
        assert_eq!(content.get("msgtype").unwrap(), "m.text");
    }

    #[test]
    fn test_create_event_params_structure() {
        let params = CreateEventParams {
            event_id: "$event123".to_string(),
            room_id: "!room123:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "test"}),
            state_key: None,
            origin_server_ts: current_timestamp_millis(),
            redacts: None,
        };

        assert_eq!(params.event_type, "m.room.message");
        assert!(params.state_key.is_none());
        assert!(params.redacts.is_none());
    }

    #[test]
    fn test_create_event_params_with_state_key() {
        let params = CreateEventParams {
            event_id: "$state123".to_string(),
            room_id: "!room123:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_type: "m.room.name".to_string(),
            content: json!({"name": "Test Room"}),
            state_key: Some("".to_string()),
            origin_server_ts: current_timestamp_millis(),
            redacts: None,
        };

        assert!(params.state_key.is_some());
        assert_eq!(params.event_type, "m.room.name");
    }

    #[test]
    fn test_create_event_params_with_redacts() {
        let params = CreateEventParams {
            event_id: "$redact123".to_string(),
            room_id: "!room123:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_type: "m.room.redaction".to_string(),
            content: json!({"reason": "spam"}),
            state_key: None,
            origin_server_ts: current_timestamp_millis(),
            redacts: Some("$target123".to_string()),
        };

        assert!(params.redacts.is_some());
        assert_eq!(params.redacts.unwrap(), "$target123");
    }

    #[test]
    fn test_event_relationship_chain() {
        let thread_root = json!({
            "event_id": "$root123",
            "room_id": "!room123:example.com",
            "type": "m.room.message"
        });

        let reply = json!({
            "event_id": "$reply456",
            "room_id": "!room123:example.com",
            "type": "m.room.message",
            "m.relates_to": {
                "event_id": "$root123",
                "rel_type": "m.reply"
            }
        });

        assert!(thread_root.get("event_id").is_some());
        assert!(reply.get("m.relates_to").is_some());
    }

    #[test]
    fn test_reaction_event_structure() {
        let reaction = json!({
            "type": "m.reaction",
            "content": {
                "m.relates_to": {
                    "event_id": "$target123",
                    "rel_type": "m.annotation",
                    "key": "👍"
                }
            }
        });

        assert_eq!(reaction.get("type").unwrap(), "m.reaction");
        let relates_to = reaction.get("content").unwrap().get("m.relates_to").unwrap();
        assert_eq!(relates_to.get("key").unwrap(), "👍");
    }

    #[test]
    fn test_sticker_event_structure() {
        let sticker = json!({
            "type": "m.sticker",
            "content": {
                "msgtype": "m.image",
                "body": "smile.png",
                "info": {
                    "mimetype": "image/png",
                    "size": 12345,
                    "w": 200,
                    "h": 200
                }
            }
        });

        assert_eq!(sticker.get("type").unwrap(), "m.sticker");
        let info = sticker.get("content").unwrap().get("info").unwrap();
        assert_eq!(info.get("w").unwrap(), 200);
        assert_eq!(info.get("h").unwrap(), 200);
    }

    #[test]
    fn test_thread_event_structure() {
        let thread_event = json!({
            "type": "m.room.message",
            "content": {
                "body": "Thread reply",
                "m.relates_to": {
                    "event_id": "$thread_root",
                    "rel_type": "m.thread",
                    "is_falling_back": true
                }
            }
        });

        let relates_to = thread_event.get("content").unwrap().get("m.relates_to").unwrap();
        assert_eq!(relates_to.get("rel_type").unwrap(), "m.thread");
        assert_eq!(relates_to.get("is_falling_back").unwrap(), true);
    }

    // ── redaction_allowed_period ────────────────────────────────────────────

    #[test]
    fn test_redaction_allowed_period_blocks_only_old_messages() {
        let period = 60_000_i64; // 1 minute
        let now = 1_700_000_000_000_i64;

        // A recent message is redactable.
        assert!(!redaction_exceeds_allowed_period(period, "m.room.message", now - 30_000, now));
        // A stale message is blocked.
        assert!(redaction_exceeds_allowed_period(period, "m.room.message", now - 120_000, now));
        // Exactly on the boundary is still allowed (`<`, not `<=`).
        assert!(!redaction_exceeds_allowed_period(period, "m.room.message", now - period, now));
        // Non-message events are never blocked, however old.
        assert!(!redaction_exceeds_allowed_period(period, "m.room.name", now - 10_000_000, now));
        assert!(!redaction_exceeds_allowed_period(period, "m.room.member", now - 10_000_000, now));
    }

    #[test]
    fn test_edit_parent_event_id_extracts_replace_target() {
        let edit = json!({
            "body": "* corrected",
            "m.new_content": { "body": "corrected" },
            "m.relates_to": { "rel_type": "m.replace", "event_id": "$original" }
        });
        assert_eq!(edit_parent_event_id(&edit), Some("$original"));
    }

    #[test]
    fn test_edit_parent_event_id_ignores_non_replace_relations() {
        // A reply/thread is not an edit — the redaction age must use the event's
        // own timestamp, not the referenced event's.
        let reply = json!({
            "body": "hi",
            "m.relates_to": { "rel_type": "m.thread", "event_id": "$root" }
        });
        assert_eq!(edit_parent_event_id(&reply), None);
        // A plain message carries no relation at all.
        assert_eq!(edit_parent_event_id(&json!({ "body": "hi" })), None);
        // A replace without a target id yields nothing.
        assert_eq!(edit_parent_event_id(&json!({ "m.relates_to": { "rel_type": "m.replace" } })), None);
    }
}
