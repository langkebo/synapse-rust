use super::{
    ensure_room_view_access, get_room_event, parse_pagination_direction, parse_room_messages_from_token, RoomWriteQuery,
};
use crate::routes::context::RoomContext;
use crate::routes::extractors::{EventId, RoomId, UserId};
use crate::routes::MatrixJson;
use crate::routes::{validate_event_id, validate_room_id, AuthenticatedUser};
use crate::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use synapse_common::current_timestamp_millis;
use synapse_common::map_internal;
use synapse_common::{ApiError, MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
// 别名只为让 `clamp` 与 `query.limit` 落在**同一物理行**：分页门禁是逐行扫描
// （`tests/unit/test_pagination_limit_clamp_tests.rs`），而 `max_width = 120`
// 放不下完整路径。上限仍是 `client_push_service` 的那一个常量，不另立数字。
use synapse_services::client_push_service::MAX_NOTIFICATIONS_LIMIT as MAX_NOTIFICATIONS;
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

    // `get_event_for_client` (not `get_event`): an event from before the caller was
    // involved in the room must not be readable, and must 404 rather than 403 so
    // the endpoint cannot be used to probe a room's timeline.
    let event = ctx.room_service.messaging().get_event_for_client(&room_id, &auth_user.user_id, &event_id).await?;

    Ok(Json(event))
}

/// See [`get_event_keys`].
///
/// C-1: this endpoint has no key storage behind it. It used to answer
/// `200 {"keys": []}`, which is indistinguishable from "this event has no keys"
/// — a caller would conclude the feature works and the event simply carries
/// nothing. Answer 501 `M_UNKNOWN` instead, so the caller knows the endpoint is
/// not implemented here.
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

    Err(ApiError::not_supported("Event keys are not implemented by this server"))
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

/// The `GET /rooms/{roomId}/notifications` query parameters.
///
/// `only` is **repeatable** (`?only=type:m.room.message&only=sender:@a:b`), and
/// the `serde_urlencoded` deserializer behind `axum::extract::Query` rejects a
/// repeated key in a derived struct with "duplicate field" — hence the
/// hand-written `Deserialize` (same pattern as `routes/push.rs`). Unknown
/// parameters are ignored, as Matrix requires for query strings.
#[derive(Debug, Default)]
pub(crate) struct RoomNotificationsQuery {
    pub(crate) from: Option<String>,
    pub(crate) limit: Option<String>,
    pub(crate) only: Vec<String>,
}

impl<'de> Deserialize<'de> for RoomNotificationsQuery {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RoomNotificationsQueryVisitor;

        impl<'de> serde::de::Visitor<'de> for RoomNotificationsQueryVisitor {
            type Value = RoomNotificationsQuery;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("the query string of GET /rooms/{room_id}/notifications")
            }

            fn visit_map<A>(self, mut map: A) -> Result<RoomNotificationsQuery, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut query = RoomNotificationsQuery::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "from" => query.from = map.next_value()?,
                        "limit" => query.limit = map.next_value()?,
                        "only" => query.only.push(map.next_value()?),
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(query)
            }
        }

        deserializer.deserialize_map(RoomNotificationsQueryVisitor)
    }
}

/// The `GET /rooms/{roomId}/messages` query parameters.
///
/// `limit` / `filter` stay `Option<String>` because the handler used to read
/// them off a JSON query value (where every urlencoded value is a string) and
/// parsed them itself; a bad `limit` must keep falling back to the default
/// rather than turning into a 400.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RoomMessagesQuery {
    pub(crate) from: Option<String>,
    pub(crate) limit: Option<String>,
    pub(crate) dir: Option<String>,
    pub(crate) filter: Option<String>,
}

/// The `GET /rooms/{roomId}/timeline` query parameters.
///
/// `to` closes the `[from, to]` pagination interval (exclusive bound); `filter`
/// is accepted by the spec shape but not implemented here, so the handler
/// rejects a non-empty value instead of silently returning unfiltered events.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RoomTimelineQuery {
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) limit: Option<String>,
    pub(crate) dir: Option<String>,
    pub(crate) filter: Option<String>,
}

/// See [`get_room_notifications`].
pub(crate) async fn get_room_notifications(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<RoomNotificationsQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    // 边界层自己夹一次：服务层（`get_room_notifications_page`）也会夹，但 `?limit=-1`
    // 不该靠"下游还会管"来兜 —— 负 LIMIT 直接是 PostgreSQL 错误。
    let limit = query.limit.as_deref().and_then(|v| v.parse::<i64>().ok()).unwrap_or(20).clamp(1, MAX_NOTIFICATIONS);
    let filters = synapse_services::client_push_service::NotificationFilters::from_only_params(&query.only)?;

    let page = ctx
        .client_push_service
        .get_room_notifications_page(&auth_user.user_id, &room_id, limit, query.from.as_deref(), filters)
        .await?;

    Ok(Json(json!({
        "notifications": page.notifications,
        "next_token": page.next_token
    })))
}

/// See [`get_messages`].
pub(crate) async fn get_messages(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<RoomMessagesQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(query.from.as_deref())?;
    let limit = query.limit.as_deref().and_then(|s| s.parse::<u64>().ok()).unwrap_or(10).clamp(1, 1000) as i64;
    let direction = parse_pagination_direction(query.dir.as_deref())?;

    // A9: `/messages` `filter` is not implemented on this server. Reject it
    // explicitly instead of silently returning unfiltered events (a client
    // that relies on `filter` would otherwise receive too much data without
    // any signal that its constraint was dropped).
    if let Some(filter) = query.filter.as_deref() {
        if !filter.is_empty() {
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
    Query(query): Query<RoomWriteQuery>,
    MatrixJson(body): MatrixJson<Value>,
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

    // The whole message write gate — power level, room encryption, the
    // power-levels change rules and the MSC3806 content scan — lives in one
    // place (`ensure_message_write_access`) so the dedicated MSC4140 scheduling
    // endpoint cannot become a second, weaker door into this same write.
    let mut body = body;
    super::ensure_message_write_access(&ctx, &auth_user, &room_id, &event_type, &mut body).await?;

    // MSC4140: If the request (query parameter, or the legacy body field)
    // carries `org.matrix.msc4140.delay`, schedule the event as a delayed event
    // instead of sending immediately. The server returns a `delay_id` (numeric)
    // that clients use to cancel/restart/send via the management endpoint.
    if let Some(response) = super::schedule_delayed_event_if_requested(
        &ctx,
        &auth_user.user_id,
        auth_user.device_id.as_deref().unwrap_or(""),
        &room_id,
        &event_type,
        None,
        &body,
        &query,
        Some(&txn_id),
    )
    .await?
    {
        return Ok(response);
    }

    // MSC4354: a `?org.matrix.msc4354.sticky_duration_ms=` hint on `/send` is
    // honoured by recording the TTL on the sent event's `unsigned`. Parsed here —
    // after the delayed path above, which refuses the delay+sticky combination
    // outright — so an invalid duration fails before the event is written. The
    // dedicated `/delayed_event` surface and the private `sticky_events` pins
    // remain separate; see `apply_msc4354_sticky_ttl` for how the two models
    // coexist.
    let sticky_duration_ms = super::msc4354_query_sticky_duration_ms(&query)?;

    // ISSUE-03: txn 去重的唯一事实源是 DB 唯一约束（room_event_txn_dedup），
    // 上方缓存仅为快路径；缓存丢失/过期时重试仍返回同一 event_id。
    let result = ctx
        .room_service
        .messaging()
        .send_message_with_txn(&room_id, &auth_user.user_id, &event_type, &body, &txn_id)
        .await?;

    if let Some(duration_ms) = sticky_duration_ms {
        if let Some(event_id) = result.get("event_id").and_then(|v| v.as_str()) {
            // Propagate a failure here rather than swallow it: the event is
            // already persisted and the txn is *not* cached yet, so a retry
            // re-enters through `send_message_with_txn` (returning the same
            // event_id via DB dedup) and re-attempts the `unsigned` write.
            super::apply_msc4354_sticky_ttl(&ctx, event_id, duration_ms).await?;
        }
    }

    // NEW-13①：标准 `/send` 带 `m.relates_to.rel_type = m.thread` 时必须**补登记线程根** ——
    // 此前只有 vendor 的 `POST /_matrix/vendor/v1/threads` 会建 `thread_roots` 阴影行，
    // 所以用标准 `/send` 发线程回复的客户端"写得进事件、读不到线程"。
    // **best-effort**：事件已经落库，登记失败不能让整条发送失败（否则客户端会重试消息）；
    // 失败只记 warn，下一次回复会再试（`ensure_thread_root_for_reply` 本身就是幂等的）。
    if let Some(event_id) = result.get("event_id").and_then(|v| v.as_str()) {
        if let Some(root_event_id) = super::thread_root_event_id(&body) {
            if root_event_id != event_id {
                if let Err(e) = ctx
                    .thread_service
                    .ensure_thread_root_for_reply(&auth_user.user_id, &room_id, &root_event_id, event_id)
                    .await
                {
                    ::tracing::warn!(
                        room_id = %room_id,
                        root_event_id = %root_event_id,
                        error = %e,
                        "Failed to register thread root for a threaded /send; the thread stays unlistable until a retry"
                    );
                }
            }
        }
    }

    if !txn_id.is_empty() {
        let cache_key = format!("txn:{}:{}:{}", auth_user.user_id, room_id, txn_id);
        if let Err(e) = ctx.cache.set(&cache_key, &result.to_string(), 3600).await {
            ::tracing::warn!("Failed to cache transaction ID dedup marker: {e}");
        }
    }

    Ok(Json(result))
}

/// Query parameters for [`get_room_message_queue`].
///
/// The endpoint exposes a **snapshot** of the live send queue, not a paged
/// collection: the pending rows are the ones awaiting delivery right now, so
/// they are consumed as they are sent and a keyset cursor would skip or repeat
/// rows. Only `limit` is accepted; `from` is rejected rather than ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MessageQueueQuery {
    /// Maximum number of pending events to include (1-500, default 100).
    pub limit: Option<i64>,
}

/// See [`get_room_message_queue`].
pub(crate) async fn get_room_message_queue(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<MessageQueueQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let pending_events = ctx.room_service.messaging().get_pending_events(&room_id, limit).await?;

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
    Query(query): Query<RoomTimelineQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(query.from.as_deref())?;
    let to = parse_room_messages_from_token(query.to.as_deref())?;
    let limit = query.limit.as_deref().and_then(|s| s.parse::<u64>().ok()).unwrap_or(10).clamp(1, 1000) as i64;
    let direction = parse_pagination_direction(query.dir.as_deref())?;

    // Same stance as `/messages`: an unimplemented `filter` is rejected instead
    // of being accepted and dropped.
    if let Some(filter) = query.filter.as_deref() {
        if !filter.is_empty() {
            return Err(ApiError::invalid_param("The 'filter' parameter is not supported on this endpoint"));
        }
    }

    Ok(Json(
        ctx.room_service
            .messaging()
            .get_room_messages_bounded(&room_id, &auth_user.user_id, from, to, limit, direction)
            .await?,
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

/// Query parameters accepted by [`get_room_encrypted_events`].
///
/// `from` is a plain offset into the newest-first encrypted timeline and
/// `limit` is clamped to the platform pagination bounds. A non-numeric value is
/// rejected by the typed extractor with `400`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct EncryptedEventsQuery {
    pub from: Option<i64>,
    pub limit: Option<i64>,
}

/// See [`get_room_encrypted_events`].
pub(crate) async fn get_room_encrypted_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<EncryptedEventsQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await.map_err(map_internal!("Failed to check room existence"))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let limit = query.limit.unwrap_or(100).clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let offset = query.from.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::invalid_param("from must be a non-negative offset"));
    }

    // Fetch one extra row so `next_batch` reflects whether more events remain
    // rather than whether this page happened to be exactly full. PERF-08: the
    // window is applied in SQL, so a deep page returns `limit + 1` rows instead of
    // transferring every skipped row and slicing it away in memory.
    let fetch_limit = limit.saturating_add(1);
    let encrypted_events = ctx
        .room_service
        .messaging()
        .get_room_events_by_type(&room_id, "m.room.encrypted", fetch_limit, offset)
        .await
        .map_err(map_internal!("Failed to get encrypted events"))?;

    let take = usize::try_from(limit).unwrap_or(0);
    let has_more = encrypted_events.len() > take;

    let events: Vec<serde_json::Value> = encrypted_events
        .into_iter()
        .take(take)
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

    let next_batch = if has_more { Some(offset.saturating_add(limit).to_string()) } else { None };

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "events": events,
        "total": events.len(),
        "next_batch": next_batch
    })))
}

/// Query parameters for [`get_room_event_perspective`].
///
/// `event_id` is required: the endpoint reports the perspective **of an event**,
/// not of the room. It used to take only the room id, so both parameters were
/// silently ignored and every caller got the same room-level aggregate.
#[derive(Debug, Deserialize)]
pub(crate) struct EventPerspectiveQuery {
    pub event_id: String,
    pub room_version: Option<String>,
}

/// See [`get_room_event_perspective`].
pub(crate) async fn get_room_event_perspective(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<EventPerspectiveQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&query.event_id)?;
    if let Some(room_version) = query.room_version.as_deref() {
        if !synapse_common::is_supported_room_version(room_version) {
            return Err(ApiError::unsupported_room_version(format!(
                "Room version {room_version} is not supported by this server"
            )));
        }
    }

    if !ctx.room_service.state().room_exists(&room_id).await.map_err(map_internal!("Failed to check room existence"))? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    // History visibility applies to the anchor as well: an event from before the
    // caller was involved in the room must 404 instead of leaking into the window.
    let anchor =
        ctx.room_service.messaging().get_event_for_client(&room_id, &auth_user.user_id, &query.event_id).await?;

    let anchor_ts = anchor.get("origin_server_ts").and_then(Value::as_i64).unwrap_or_default();
    let events_before = ctx
        .room_service
        .messaging()
        .get_events_before_context(&room_id, anchor_ts, 99)
        .await
        .map_err(map_internal!("Failed to get events before context"))?;

    let mut senders: HashMap<String, usize> = HashMap::new();
    let mut event_types: HashMap<String, usize> = HashMap::new();
    let mut record = |event: &Value| {
        if let Some(sender) = event.get("sender").and_then(Value::as_str) {
            *senders.entry(sender.to_string()).or_insert(0) += 1;
        }
        if let Some(event_type) = event.get("type").and_then(Value::as_str) {
            *event_types.entry(event_type.to_string()).or_insert(0) += 1;
        }
    };
    for event in &events_before {
        record(event);
    }
    record(&anchor);

    let Value::Object(mut response) = anchor else {
        return Err(ApiError::internal("Unexpected event representation"));
    };
    let latest_event_id = response.get("event_id").cloned();
    response.insert(
        "perspective".to_string(),
        json!({
            "event_count": events_before.len() + 1,
            "latest_event_id": latest_event_id,
            "sender_activity": senders,
            "event_types": event_types
        }),
    );

    Ok(Json(Value::Object(response)))
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

/// Request body for [`sign_room_event`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignRoomEventRequest {
    /// Device that produced the signature (defaults to `default`).
    pub device_id: Option<String>,
    /// Signing key identifier (defaults to `ed25519:<device_id>`).
    pub key_id: Option<String>,
    /// Base64 signature; required.
    pub signature: Option<String>,
    /// Algorithm echoed back to the caller (derived from `key_id` when absent).
    pub algorithm: Option<String>,
}

/// Request body for [`verify_room_event`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifyRoomEventRequest {
    /// Only return signatures from this user.
    pub user_id: Option<String>,
    /// Only return signatures from this device.
    pub device_id: Option<String>,
}

/// Request body shared by the translation endpoints.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TranslateRequest {
    /// Target language; defaults to the configured language.
    pub target_lang: Option<String>,
    /// Optional source language hint.
    pub source_lang: Option<String>,
    /// Text to translate (otherwise the event body is used).
    pub text: Option<String>,
}

/// See [`sign_room_event`].
pub(crate) async fn sign_room_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
    MatrixJson(body): MatrixJson<SignRoomEventRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let _event = ctx.room_service.messaging().get_event(&room_id, &event_id).await?;

    let device_id = body.device_id.as_deref().unwrap_or("default");

    let default_key_id = format!("ed25519:{device_id}");
    let key_id = body.key_id.as_deref().unwrap_or(&default_key_id);

    let signature =
        body.signature.as_deref().ok_or_else(|| ApiError::bad_request("signature is required".to_string()))?;

    // D-99：`algorithm` 只是**回显**给调用方（客户端声明的算法，缺省取 `key_id` 前缀）。
    // 它原先还被写进 `event_signatures.algorithm`，但那一列从来没有人读、且与 `key_id` 前缀
    // 语义重复 ⇒ 列与写入参数已删除，这里保留推导仅用于响应。
    let algorithm = body
        .algorithm
        .clone()
        .unwrap_or_else(|| key_id.split(':').next().filter(|value| !value.is_empty()).unwrap_or("ed25519").to_string());

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
    MatrixJson(body): MatrixJson<VerifyRoomEventRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let _event = ctx.room_service.messaging().get_event(&room_id, &event_id).await?;

    let signatures = ctx.room_service.messaging().get_event_signatures(&event_id).await?;

    let verify_user_id = body.user_id.as_deref();
    let verify_device_id = body.device_id.as_deref();

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
    MatrixJson(body): MatrixJson<TranslateRequest>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let event = get_room_event(&ctx, &room_id, &event_id).await?;
    let source_text = event.get("content").and_then(|c| c.get("body")).and_then(|value| value.as_str()).unwrap_or("");

    // Extract target language from request body, falling back to config default
    let target_lang = body.target_lang.as_deref().unwrap_or(&ctx.config.translate.default_target_lang);

    // Extract optional source language from request body
    let source_lang = body.source_lang.as_deref();

    // Use the text field if provided, otherwise use the event body
    let text_to_translate = body.text.as_deref().unwrap_or(source_text);

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
    MatrixJson(body): MatrixJson<TranslateRequest>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let text = body.text.as_deref().unwrap_or("");

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

    let target_lang = body.target_lang.as_deref().unwrap_or(&ctx.config.translate.default_target_lang);

    let source_lang = body.source_lang.as_deref();

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
    MatrixJson(mut body): MatrixJson<Value>,
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
    ctx.room_service
        .messaging()
        .redact_event_content(&event_id, &room_id, Some(&redaction_event.event_id))
        .await
        .map_err(|e| {
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
        })?;

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
    fn test_event_keys_semantics_are_not_a_success_shaped_empty_array() {
        // C-1: `GET /rooms/{room_id}/keys/{event_id}` has no storage behind it.
        // Answering `200 {"keys": []}` reads as "this event has no keys", so the
        // handler answers 501 `M_UNKNOWN` instead.
        let err = ApiError::not_supported("Event keys are not implemented by this server");
        assert!(err.code_is(synapse_common::MatrixErrorCode::Unknown));
    }

    #[test]
    fn event_perspective_query_requires_event_id() {
        // D-14: the endpoint is about one event, so `event_id` is required. It
        // used to be absent from the extractor entirely and silently ignored.
        let query: EventPerspectiveQuery = serde_json::from_value(json!({
            "event_id": "$abc:example.com",
            "room_version": "10",
            "unrelated": "ignored"
        }))
        .expect("query should parse");
        assert_eq!(query.event_id, "$abc:example.com");
        assert_eq!(query.room_version.as_deref(), Some("10"));

        assert!(serde_json::from_value::<EventPerspectiveQuery>(json!({ "room_version": "10" })).is_err());
    }

    #[test]
    fn event_perspective_rejects_unsupported_room_version() {
        assert!(synapse_common::is_supported_room_version("10"));
        assert!(!synapse_common::is_supported_room_version("not-a-version"));

        let err = ApiError::unsupported_room_version("Room version not-a-version is not supported by this server");
        assert!(err.code_is(synapse_common::MatrixErrorCode::UnsupportedRoomVersion));
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
