use crate::common::{ApiError, ContentSanitizer};
use crate::map_internal;
use crate::storage::CreateEventParams;
use crate::web::routes::{validate_event_id, validate_room_id, AppState, AuthenticatedUser};
use super::{ensure_room_view_access, get_room_event, parse_room_messages_from_token};
use axum::{
    extract::{Json, Path, Query, State},
};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

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
        .map_err(map_internal!("Failed to get event"))?
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
        .map_err(map_internal!("Failed to get event"))?
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
        .map_err(map_internal!("Failed to get event"))?
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
        .map_err(map_internal!("Failed to get thread root"))?
    {
        let thread_id = thread_root.thread_id.clone().unwrap_or_default();
        if !thread_id.is_empty() {
            let replies = state
                .services
                .thread_storage
                .get_thread_replies(&room_id, &thread_id, Some(50), None)
                .await
                .map_err(map_internal!("Failed to get thread replies"))?;
            reply_count = state
                .services
                .thread_storage
                .get_reply_count(&room_id, &thread_id)
                .await
                .map_err(map_internal!("Failed to get reply count"))?;
            participants_json = state
                .services
                .thread_storage
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
    .map_err(map_internal!("Database error"))?;

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
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(&params);
    let limit = params
        .get("limit")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(10)
        .min(1000) as i64;
    let direction = params.get("dir").and_then(|v| v.as_str()).unwrap_or("b");

    Ok(Json(
        state
            .services
            .room_service
            .get_room_messages(&room_id, &auth_user.user_id, from, limit, direction)
            .await?,
    ))
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
        return Err(ApiError::bad_request(
            "Message content too long (max 64KB)".to_string(),
        ));
    }

    if !txn_id.is_empty() {
        let cache_key = format!("txn:{}:{}:{}", auth_user.user_id, room_id, txn_id);
        if let Ok(Some(cached)) = state.services.cache.get::<String>(&cache_key).await {
            if let Ok(event_id) = serde_json::from_str::<serde_json::Value>(&cached) {
                return Ok(Json(event_id));
            }
        }
    }

    state
        .services
        .auth_service
        .verify_message_event_write(&room_id, &auth_user.user_id, &event_type)
        .await?;

    if event_type == "m.room.encrypted" {
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
        .map_err(map_internal!("Failed to check room encryption status"))?
        .is_some();

        if !is_encrypted {
            return Err(ApiError::bad_request(
                "Cannot send encrypted message to a room where encryption is not enabled. Enable encryption first by sending an m.room.encryption state event.".to_string(),
            ));
        }
    }

    if event_type == "m.room.power_levels" {
        state
            .services
            .auth_service
            .verify_power_levels_change(&room_id, &auth_user.user_id, &body)
            .await?;
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

    let result = state
        .services
        .room_service
        .send_message(&room_id, &auth_user.user_id, &event_type, &body)
        .await?;

    if !txn_id.is_empty() {
        let cache_key = format!("txn:{}:{}:{}", auth_user.user_id, room_id, txn_id);
        let _ = state
            .services
            .cache
            .set(&cache_key, &result.to_string(), 3600)
            .await;
    }

    Ok(Json(result))
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
        .map_err(map_internal!("Failed to check room existence"))?
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
    .map_err(map_internal!("Failed to get pending events"))?
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
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let from = parse_room_messages_from_token(&params);
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as i64;
    let direction = params.get("dir").and_then(|v| v.as_str()).unwrap_or("b");

    Ok(Json(
        state
            .services
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
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
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
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let encrypted_events = state
        .services
        .event_storage
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
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .event_storage
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
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .event_storage
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
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let events = state
        .services
        .event_storage
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
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(map_internal!("Failed to get event"))?
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
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(map_internal!("Failed to get event"))?
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

    let default_key_id = format!("ed25519:{device_id}");
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
    .map_err(map_internal!("Failed to save signature"))?;

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
        .map_err(map_internal!("Failed to check room existence"))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(map_internal!("Failed to get event"))?
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
    .map_err(map_internal!("Failed to get signatures"))?;

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
        .map_err(map_internal!("Failed to get event"))?
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
        .map_err(map_internal!("Failed to redact event"))?;

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
            ApiError::internal(format!("Failed to redact event content: {e}"))
        })?;

    Ok(Json(json!({
        "event_id": new_event_id
    })))
}

#[allow(dead_code)]
pub(crate) async fn get_relations(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, rel_type)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let room_id = room_id.replace("%21", "!").replace("%3A", ":");
    let event_id = event_id.replace("%24", "$");

    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;

    let mut chunk = Vec::new();

    if rel_type == "m.thread" {
        if let Some(thread_root) = state
            .services
            .thread_storage
            .get_thread_root_by_event(&room_id, &event_id)
            .await
            .map_err(map_internal!("Failed to get thread root"))?
        {
            let thread_id = thread_root.thread_id.clone().unwrap_or_default();
            if !thread_id.is_empty() {
                let replies = state
                    .services
                    .thread_storage
                    .get_thread_replies(&room_id, &thread_id, Some(100), None)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to get thread replies: {e}"))
                    })?;

                for reply in replies {
                    chunk.push(json!({
                        "type": "m.room.message",
                        "room_id": reply.room_id,
                        "sender": reply.sender,
                        "content": reply.content,
                        "event_id": reply.event_id,
                        "origin_server_ts": reply.origin_server_ts,
                    }));
                }
            }
        }

        let root_event = state
            .services
            .event_storage
            .get_event(&event_id)
            .await
            .map_err(map_internal!("Failed to get event"))?;

        if let Some(root) = root_event {
            chunk.insert(
                0,
                json!({
                    "type": root.event_type,
                    "room_id": root.room_id,
                    "sender": root.user_id,
                    "content": root.content,
                    "event_id": root.event_id,
                    "origin_server_ts": root.origin_server_ts,
                }),
            );
        }
    } else {
        let params = crate::storage::relations::RelationQueryParams {
            room_id: room_id.clone(),
            relates_to_event_id: event_id.clone(),
            relation_type: Some(rel_type.clone()),
            limit: Some(100),
            from: None,
            direction: Some("b".to_string()),
        };
        let related_events = state
            .services
            .relations_storage
            .get_relations(params)
            .await
            .map_err(map_internal!("Failed to get relations"))?;

        for evt in related_events {
            chunk.push(json!({
                "type": "m.room.message",
                "room_id": evt.room_id,
                "sender": evt.sender,
                "content": evt.content,
                "event_id": evt.event_id,
                "origin_server_ts": evt.origin_server_ts,
            }));
        }
    }

    Ok(Json(json!({
        "chunk": chunk,
        "next_batch": Option::<String>::None,
        "prev_batch": Option::<String>::None,
    })))
}
