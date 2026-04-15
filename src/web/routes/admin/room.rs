use super::audit::{record_audit_event, resolve_request_id};
use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn create_room_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/rooms", get(get_rooms))
        .route("/_synapse/admin/v1/rooms/{room_id}", get(get_room))
        .route("/_synapse/admin/v1/rooms/{room_id}", delete(delete_room))
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/delete",
            post(delete_room),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/members",
            get(get_room_members_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/state",
            get(get_room_state_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/messages",
            get(get_room_messages_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/aliases",
            get(get_room_aliases_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/version",
            get(get_room_version),
        )
        .route("/_synapse/admin/v1/rooms/{room_id}/block", post(block_room))
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/block",
            get(get_room_block_status),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/unblock",
            post(unblock_room),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/make_admin",
            post(make_room_admin).put(make_room_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/purge_history",
            post(purge_history_by_room),
        )
        .route("/_synapse/admin/v1/purge_history", post(purge_history))
        .route("/_synapse/admin/v1/purge_room", post(purge_room))
        .route("/_synapse/admin/v1/shutdown_room", post(shutdown_room))
        .route("/_synapse/admin/v1/spaces", get(get_spaces))
        .route("/_synapse/admin/v1/spaces/{space_id}", get(get_space))
        .route("/_synapse/admin/v1/spaces/{space_id}", delete(delete_space))
        .route(
            "/_synapse/admin/v1/spaces/{space_id}/users",
            get(get_space_users),
        )
        .route(
            "/_synapse/admin/v1/spaces/{space_id}/rooms",
            get(get_space_rooms),
        )
        .route(
            "/_synapse/admin/v1/spaces/{space_id}/stats",
            get(get_space_stats),
        )
        // Room statistics
        .route("/_synapse/admin/v1/room_stats", get(get_room_stats))
        .route(
            "/_synapse/admin/v1/room_stats/{room_id}",
            get(get_single_room_stats),
        )
        // Room membership management
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
            put(join_room_member),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
            delete(remove_room_member),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/ban/{user_id}",
            post(ban_user),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/ban",
            post(ban_user_by_body),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/unban/{user_id}",
            post(unban_user),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/kick/{user_id}",
            post(kick_user),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/kick",
            post(kick_user_by_body),
        )
        // Room listing
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/listings",
            get(get_room_listings),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/listings/public",
            put(set_room_public),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/listings/public",
            delete(set_room_private),
        )
        // Additional room APIs for 100% coverage
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/event_context/{event_id}",
            get(get_event_context_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/token_sync",
            get(get_room_token_sync_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/search",
            post(search_room_messages_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/search",
            post(search_all_rooms).get(search_all_rooms_query),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/forward_extremities",
            get(get_room_forward_extremities),
        )
}

async fn ensure_space_exists(state: &AppState, space_id: &str) -> Result<(), ApiError> {
    let space: Option<String> =
        sqlx::query_scalar("SELECT space_id FROM spaces WHERE space_id = $1")
            .bind(space_id)
            .fetch_optional(&*state.services.room_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if space.is_none() {
        return Err(ApiError::not_found("Space not found".to_string()));
    }

    Ok(())
}

#[axum::debug_handler]
pub async fn get_room_aliases_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    Ok(Json(json!({
        "aliases": []
    })))
}

#[derive(Debug, Deserialize)]
pub struct BlockRoomRequest {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MakeRoomAdminRequest {
    pub user_id: String,
}

#[axum::debug_handler]
pub async fn get_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .clamp(0, i64::MAX);

    let rooms_with_members = state
        .services
        .room_storage
        .get_all_rooms_with_members(limit, offset)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let total = state
        .services
        .room_storage
        .get_room_count()
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room_list: Vec<Value> = rooms_with_members
        .iter()
        .map(|(r, joined_members)| {
            json!({
                "room_id": r.room_id.clone(),
                "name": r.name.clone().unwrap_or_default(),
                "topic": r.topic.clone().unwrap_or_default(),
                "creator": r.creator_user_id.clone().unwrap_or_default(),
                "joined_members": joined_members,
                "joined_local_members": joined_members,
                "is_public": r.is_public
            })
        })
        .collect();

    Ok(Json(json!({
        "rooms": room_list,
        "total": total
    })))
}

#[axum::debug_handler]
pub async fn get_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match room {
        Some(r) => Ok(Json(json!({
            "room_id": r.room_id,
            "name": r.name.unwrap_or_default(),
            "topic": r.topic.unwrap_or_default(),
            "creator": r.creator_user_id.unwrap_or_default(),
            "member_count": r.member_count,
            "room_version": r.room_version,
            "encryption": r.encryption,
            "is_public": r.is_public,
            "join_rule": r.join_rule
        }))),
        None => Err(ApiError::not_found("Room not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state
        .services
        .room_storage
        .delete_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "deleted": true
    })))
}

#[axum::debug_handler]
pub async fn get_room_members_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let member_list: Vec<Value> = members
        .iter()
        .map(|m| {
            json!({
                "user_id": m.user_id,
                "displayname": m.display_name,
                "avatar_url": m.avatar_url,
                "membership": m.membership
            })
        })
        .collect();

    Ok(Json(json!({
        "members": member_list,
        "total": member_list.len()
    })))
}

#[axum::debug_handler]
pub async fn get_room_state_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "state_key": e.state_key,
                "content": e.content,
                "sender": e.user_id,
                "event_id": e.event_id
            })
        })
        .collect();

    Ok(Json(json!({ "state": state_events })))
}

#[axum::debug_handler]
pub async fn get_room_messages_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);

    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let messages: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "content": e.content,
                "sender": e.user_id,
                "origin_server_ts": e.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "chunk": messages,
        "start": params.get("from").unwrap_or(&"0".to_string()).clone(),
        "end": messages.last().and_then(|m| m.get("event_id").and_then(|e| e.as_str()).map(|s| s.to_string()))
    })))
}

#[axum::debug_handler]
pub async fn block_room(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<BlockRoomRequest>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO blocked_rooms (room_id, blocked_at, blocked_by, reason)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (room_id) DO UPDATE SET blocked_at = $2, reason = $4
        "#,
    )
    .bind(&room_id)
    .bind(now)
    .bind(&admin.user_id)
    .bind(&body.reason)
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    record_audit_event(
        &state,
        &admin.user_id,
        "admin.room.block",
        "room",
        &room_id,
        resolve_request_id(&headers),
        json!({
            "block": body.block,
            "reason": body.reason
        }),
    )
    .await?;

    Ok(Json(json!({ "block": body.block })))
}

#[axum::debug_handler]
pub async fn get_room_block_status(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let result = sqlx::query("SELECT room_id, blocked_at FROM blocked_rooms WHERE room_id = $1")
        .bind(&room_id)
        .fetch_optional(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => Ok(Json(json!({
            "block": true,
            "blocked_at": row.get::<Option<i64>, _>("blocked_at")
        }))),
        None => Ok(Json(json!({ "block": false }))),
    }
}

#[axum::debug_handler]
pub async fn unblock_room(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    sqlx::query("DELETE FROM blocked_rooms WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    record_audit_event(
        &state,
        &admin.user_id,
        "admin.room.unblock",
        "room",
        &room_id,
        resolve_request_id(&headers),
        json!({ "block": false }),
    )
    .await?;

    Ok(Json(json!({ "block": false })))
}

#[axum::debug_handler]
pub async fn make_room_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<MakeRoomAdminRequest>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if state
        .services
        .user_storage
        .get_user_by_id(&body.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .is_none()
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let now = chrono::Utc::now().timestamp_millis();
    let event_id = crate::common::crypto::generate_event_id(&state.services.config.server.name);
    let user_id = body.user_id.clone();
    let admin_user = "@admin:".to_string() + &state.services.config.server.name;
    let power_levels = json!({
        "users": {
            user_id.clone(): 100
        },
        "users_default": 0,
        "events_default": 0,
        "state_default": 50,
        "ban": 50,
        "kick": 50,
        "redact": 50,
        "invite": 0
    });

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, user_id, event_type, content, state_key, origin_server_ts, sender, unsigned)
        VALUES ($1, $2, $3, 'm.room.power_levels', $4, '', $5, $6, '{}'::jsonb)
        ON CONFLICT (event_id) DO UPDATE SET content = $4
        "#
    )
    .bind(&event_id)
    .bind(&room_id)
    .bind(&user_id)
    .bind(power_levels)
    .bind(now)
    .bind(&admin_user)
    .execute(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn purge_history(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;
    let timestamp = body
        .get("purge_up_to_ts")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() - (30 * 24 * 60 * 60 * 1000));

    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let deleted_count = state
        .services
        .event_storage
        .delete_events_before(room_id, timestamp)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to purge history: {}", e)))?;

    Ok(Json(json!({
        "success": true,
        "deleted_events": deleted_count
    })))
}

#[axum::debug_handler]
pub async fn purge_history_by_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let merged_body = match body {
        Value::Object(mut map) => {
            map.insert("room_id".to_string(), Value::String(room_id));
            Value::Object(map)
        }
        _ => json!({ "room_id": room_id }),
    };

    purge_history(_admin, State(state), Json(merged_body)).await
}

#[axum::debug_handler]
pub async fn purge_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;

    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state
        .services
        .room_storage
        .delete_room(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to purge room: {}", e)))?;

    Ok(Json(json!({
        "purge_id": uuid::Uuid::new_v4().to_string(),
        "success": true
    })))
}

#[axum::debug_handler]
pub async fn shutdown_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;

    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state
        .services
        .room_storage
        .shutdown_room(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to shutdown room: {}", e)))?;

    state
        .services
        .member_storage
        .remove_all_members(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "kicked_users": [],
        "failed_to_kick_users": [],
        "closed_room": true
    })))
}

#[axum::debug_handler]
pub async fn get_spaces(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let spaces = sqlx::query(
        "SELECT space_id, name, topic, creator, created_ts FROM spaces ORDER BY created_ts DESC",
    )
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let space_list: Vec<Value> = spaces
        .iter()
        .map(|row| {
            json!({
                "room_id": row.get::<String, _>("space_id"),
                "name": row.get::<Option<String>, _>("name"),
                "topic": row.get::<Option<String>, _>("topic"),
                "creator": row.get::<String, _>("creator"),
                "created_ts": row.get::<i64, _>("created_ts")
            })
        })
        .collect();

    Ok(Json(
        json!({ "spaces": space_list, "total": space_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn get_space(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let space = sqlx::query(
        "SELECT space_id, name, topic, creator, created_ts FROM spaces WHERE space_id = $1",
    )
    .bind(&space_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match space {
        Some(row) => Ok(Json(json!({
            "room_id": row.get::<String, _>("space_id"),
            "name": row.get::<Option<String>, _>("name"),
            "topic": row.get::<Option<String>, _>("topic"),
            "creator": row.get::<String, _>("creator"),
            "created_ts": row.get::<i64, _>("created_ts")
        }))),
        None => Err(ApiError::not_found("Space not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_space(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM spaces WHERE space_id = $1")
        .bind(&space_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Space not found".to_string()));
    }

    Ok(Json(json!({ "deleted": true })))
}

#[axum::debug_handler]
pub async fn get_space_users(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_space_exists(&state, &space_id).await?;

    let users = sqlx::query(
        "SELECT user_id FROM space_members WHERE space_id = $1 AND membership = 'join'",
    )
    .bind(&space_id)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let user_list: Vec<String> = users.iter().map(|r| r.get("user_id")).collect();

    Ok(Json(
        json!({ "users": user_list, "total": user_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn get_space_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_space_exists(&state, &space_id).await?;

    let rooms = sqlx::query("SELECT room_id FROM space_children WHERE space_id = $1")
        .bind(&space_id)
        .fetch_all(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room_list: Vec<String> = rooms.iter().map(|r| r.get("room_id")).collect();

    Ok(Json(
        json!({ "rooms": room_list, "total": room_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn get_space_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_space_exists(&state, &space_id).await?;

    let member_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM space_members WHERE space_id = $1 AND membership = 'join'",
    )
    .bind(&space_id)
    .fetch_one(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let child_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM space_children WHERE space_id = $1")
            .bind(&space_id)
            .fetch_one(&*state.services.room_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "space_id": space_id,
        "member_count": member_count,
        "child_room_count": child_count
    })))
}

/// Get overall room statistics
#[axum::debug_handler]
pub async fn get_room_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let pool = &*state.services.room_storage.pool;

    // Total rooms
    let total_rooms: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Encrypted rooms
    let encrypted_rooms: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM room_state_events WHERE room_id IN (SELECT room_id FROM rooms) AND type = 'm.room.encryption'"
    )
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Public rooms
    let public_rooms: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms WHERE is_public = true")
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Total messages
    let total_messages: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE type = 'm.room.message'")
            .fetch_one(pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Total members
    let total_members: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_memberships")
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Active rooms (rooms with messages in last 7 days)
    let active_rooms: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT room_id) FROM events WHERE origin_server_ts > $1",
    )
    .bind(chrono::Utc::now().timestamp_millis() - 7 * 24 * 60 * 60 * 1000)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "total_rooms": total_rooms,
        "encrypted_rooms": encrypted_rooms,
        "public_rooms": public_rooms,
        "total_messages": total_messages,
        "total_members": total_members,
        "active_rooms": active_rooms,
        "average_messages_per_room": if total_rooms > 0 { total_messages / total_rooms } else { 0 }
    })))
}

/// Get statistics for a single room
#[axum::debug_handler]
pub async fn get_single_room_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let pool = &*state.services.room_storage.pool;

    // Check if room exists
    let room_exists: Option<String> =
        sqlx::query_scalar("SELECT room_id FROM rooms WHERE room_id = $1")
            .bind(&room_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if room_exists.is_none() {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // Member count
    let member_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM room_memberships WHERE room_id = $1 AND membership = 'join'",
    )
    .bind(&room_id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Message count
    let message_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE room_id = $1 AND event_type = 'm.room.message'",
    )
    .bind(&room_id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Last message timestamp
    let last_message_ts: Option<i64> =
        sqlx::query_scalar("SELECT MAX(origin_server_ts) FROM events WHERE room_id = $1")
            .bind(&room_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Room state
    let is_encrypted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM room_state_events WHERE room_id = $1 AND type = 'm.room.encryption')"
    )
    .bind(&room_id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Admin count
    let admin_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM room_memberships WHERE room_id = $1 AND membership = 'join' AND user_id IN (SELECT user_id FROM users WHERE is_admin = true)"
    )
    .bind(&room_id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "member_count": member_count,
        "message_count": message_count,
        "last_message_ts": last_message_ts,
        "is_encrypted": is_encrypted,
        "admin_count": admin_count
    })))
}

/// Join a user to a room (force join)
#[axum::debug_handler]
pub async fn join_room_member(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        join_room_member_internal(&state, &room_id, &user_id).await?,
    ))
}

/// Remove a user from a room
#[axum::debug_handler]
pub async fn remove_room_member(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        remove_room_member_internal(&state, &room_id, &user_id).await?,
    ))
}

/// Ban a user from a room
#[derive(Debug, Deserialize)]
pub struct BanRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RoomUserActionRequest {
    pub user_id: String,
    pub reason: Option<String>,
}

async fn join_room_member_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
) -> Result<Value, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let existing_membership = state
        .services
        .member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .map(|member| member.membership);

    if existing_membership.as_deref() != Some("join") {
        state
            .services
            .room_service
            .join_room(room_id, user_id)
            .await?;
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "membership": "join"
    }))
}

async fn remove_room_member_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
) -> Result<Value, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let existing_membership = state
        .services
        .member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .map(|member| member.membership);

    if existing_membership.as_deref() == Some("join") {
        state
            .services
            .room_service
            .leave_room(room_id, user_id)
            .await?;
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "removed": true
    }))
}

async fn ban_user_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
    actor_user_id: &str,
    reason: Option<&str>,
) -> Result<Value, ApiError> {
    let existing_membership = state
        .services
        .member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .map(|member| member.membership);

    state
        .services
        .room_service
        .ban_user(room_id, user_id, actor_user_id, reason)
        .await?;

    if existing_membership.as_deref() == Some("join") {
        state
            .services
            .room_storage
            .decrement_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update member count: {}", e)))?;
    }

    if let Some(reason) = reason {
        sqlx::query(
            r#"
            UPDATE room_memberships
            SET ban_reason = $3
            WHERE room_id = $1 AND user_id = $2
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(reason)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "membership": "ban"
    }))
}

async fn unban_user_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
) -> Result<Value, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .member_storage
        .unban_member(room_id, user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "unbanned": true
    }))
}

async fn kick_user_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
    reason: Option<&str>,
) -> Result<Value, ApiError> {
    let existing_membership = state
        .services
        .member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .map(|member| member.membership);

    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    match existing_membership.as_deref() {
        Some("join") => {
            state
                .services
                .room_service
                .leave_room(room_id, user_id)
                .await?;
        }
        Some(_) => {
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query(
                r#"
                UPDATE room_memberships
                SET membership = 'leave',
                    left_ts = $3,
                    updated_ts = $3
                WHERE room_id = $1 AND user_id = $2
                "#,
            )
            .bind(room_id)
            .bind(user_id)
            .bind(now)
            .execute(&*state.services.room_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        }
        None => {}
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "kicked": true,
        "reason": reason
    }))
}

#[axum::debug_handler]
pub async fn ban_user(
    admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<BanRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        ban_user_internal(
            &state,
            &room_id,
            &user_id,
            &admin.user_id,
            body.reason.as_deref(),
        )
        .await?,
    ))
}

#[axum::debug_handler]
pub async fn ban_user_by_body(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<RoomUserActionRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        ban_user_internal(
            &state,
            &room_id,
            &body.user_id,
            &admin.user_id,
            body.reason.as_deref(),
        )
        .await?,
    ))
}

/// Unban a user from a room
#[axum::debug_handler]
pub async fn unban_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(unban_user_internal(&state, &room_id, &user_id).await?))
}

/// Kick a user from a room
#[axum::debug_handler]
pub async fn kick_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<BanRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        kick_user_internal(&state, &room_id, &user_id, body.reason.as_deref()).await?,
    ))
}

#[axum::debug_handler]
pub async fn kick_user_by_body(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<RoomUserActionRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        kick_user_internal(&state, &room_id, &body.user_id, body.reason.as_deref()).await?,
    ))
}

/// Get room listings (public/directory status)
#[axum::debug_handler]
pub async fn get_room_listings(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let is_public: Option<bool> =
        sqlx::query_scalar("SELECT is_public FROM rooms WHERE room_id = $1")
            .bind(&room_id)
            .fetch_optional(&*state.services.room_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let Some(is_public) = is_public else {
        return Err(ApiError::not_found("Room not found".to_string()));
    };

    let in_directory: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM room_directory WHERE room_id = $1)")
            .bind(&room_id)
            .fetch_one(&*state.services.room_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "public": is_public,
        "in_directory": in_directory
    })))
}

/// Set room as public
#[axum::debug_handler]
pub async fn set_room_public(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("UPDATE rooms SET is_public = true WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // Add to directory
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO room_directory (room_id, is_public, added_ts) VALUES ($1, true, $2) ON CONFLICT (room_id) DO UPDATE SET is_public = true"
    )
    .bind(&room_id)
    .bind(now)
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "public": true
    })))
}

/// Set room as private
#[axum::debug_handler]
pub async fn set_room_private(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("UPDATE rooms SET is_public = false WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // Remove from directory
    sqlx::query("DELETE FROM room_directory WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "public": false
    })))
}

/// Get event context for admin (extended information about an event)
#[axum::debug_handler]
pub async fn get_event_context_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let room_id = room_id.replace("%21", "!").replace("%3A", ":");
    let event_id = event_id.replace("%24", "$").replace("%3A", ":");

    let room_exists = state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::not_found(
            "Event not found in this room".to_string(),
        ));
    }

    let events_before: Vec<Value> = sqlx::query(
        r#"
        SELECT event_id, event_type AS type, COALESCE(user_id, sender) AS sender, content, origin_server_ts
        FROM events
        WHERE room_id = $1 AND origin_server_ts < $2
        ORDER BY origin_server_ts DESC
        LIMIT 5
        "#,
    )
    .bind(&room_id)
    .bind(event.origin_server_ts)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    .iter()
    .map(|r| {
        json!({
            "event_id": r.get::<String, _>("event_id"),
            "type": r.get::<String, _>("type"),
            "sender": r.get::<String, _>("sender"),
            "content": r.get::<Value, _>("content"),
            "origin_server_ts": r.get::<i64, _>("origin_server_ts")
        })
    })
    .collect();

    let events_after: Vec<Value> = sqlx::query(
        r#"
        SELECT event_id, event_type AS type, COALESCE(user_id, sender) AS sender, content, origin_server_ts
        FROM events
        WHERE room_id = $1 AND origin_server_ts > $2
        ORDER BY origin_server_ts ASC
        LIMIT 5
        "#,
    )
    .bind(&room_id)
    .bind(event.origin_server_ts)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    .iter()
    .map(|r| {
        json!({
            "event_id": r.get::<String, _>("event_id"),
            "type": r.get::<String, _>("type"),
            "sender": r.get::<String, _>("sender"),
            "content": r.get::<Value, _>("content"),
            "origin_server_ts": r.get::<i64, _>("origin_server_ts")
        })
    })
    .collect();

    Ok(Json(json!({
        "event": {
            "event_id": event.event_id,
            "type": event.event_type,
            "sender": event.user_id,
            "state_key": event.state_key,
            "content": event.content,
            "room_id": event.room_id,
            "origin_server_ts": event.origin_server_ts
        },
        "events_before": events_before,
        "events_after": events_after,
        "state": []
    })))
}

#[derive(Debug, Deserialize)]
pub struct RoomTokenSyncQueryParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[axum::debug_handler]
pub async fn get_room_token_sync_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<RoomTokenSyncQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = params
        .limit
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let offset = params.offset.unwrap_or(0).max(0);

    let (entries, total) = state
        .services
        .sliding_sync_service
        .get_room_token_sync(&room_id, limit, offset)
        .await?;

    let active_token_count = entries
        .iter()
        .filter(|entry| entry.pos.is_some() && !entry.is_expired)
        .count();
    let expired_token_count = entries.iter().filter(|entry| entry.is_expired).count();
    let distinct_users = entries
        .iter()
        .map(|entry| entry.user_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let distinct_devices = entries
        .iter()
        .map(|entry| format!("{}|{}", entry.user_id, entry.device_id))
        .collect::<std::collections::HashSet<_>>()
        .len();

    let results = entries
        .iter()
        .map(|entry| {
            json!({
                "user_id": entry.user_id,
                "device_id": entry.device_id,
                "conn_id": entry.conn_id,
                "list_key": entry.list_key,
                "pos": entry.pos,
                "token_created_ts": entry.token_created_ts,
                "token_expires_at": entry.token_expires_at,
                "room_timestamp": entry.room_timestamp,
                "room_updated_ts": entry.room_updated_ts,
                "bump_stamp": entry.bump_stamp,
                "highlight_count": entry.highlight_count,
                "notification_count": entry.notification_count,
                "is_dm": entry.is_dm,
                "is_encrypted": entry.is_encrypted,
                "is_tombstoned": entry.is_tombstoned,
                "invited": entry.invited,
                "name": entry.name,
                "avatar": entry.avatar,
                "is_expired": entry.is_expired
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "room_id": room_id,
        "results": results,
        "total": total,
        "summary": {
            "active_token_count": active_token_count,
            "expired_token_count": expired_token_count,
            "distinct_users": distinct_users,
            "distinct_devices": distinct_devices
        }
    })))
}

/// Search room messages for admin
#[derive(Debug, Deserialize)]
pub struct SearchRoomMessagesRequest {
    pub search_term: String,
    pub limit: Option<u32>,
    pub start_date: Option<i64>,
    pub end_date: Option<i64>,
}

#[axum::debug_handler]
pub async fn search_room_messages_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<SearchRoomMessagesRequest>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = body.limit.unwrap_or(50).min(200) as i64;
    let search_pattern = format!("%{}%", body.search_term.to_lowercase());

    let events = sqlx::query(
        r#"
        SELECT event_id, event_type, sender, content, origin_server_ts
        FROM room_events
        WHERE room_id = $1 AND event_type = 'm.room.message' AND LOWER(content::text) LIKE $2
        ORDER BY origin_server_ts DESC
        LIMIT $3
        "#,
    )
    .bind(&room_id)
    .bind(&search_pattern)
    .bind(limit)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Search failed: {}", e)))?;

    let results: Vec<Value> = events
        .iter()
        .map(|r| {
            json!({
                "event_id": r.get::<String, _>("event_id"),
                "type": r.get::<String, _>("event_type"),
                "sender": r.get::<String, _>("sender"),
                "content": r.get::<Value, _>("content"),
                "origin_server_ts": r.get::<i64, _>("origin_server_ts"),
                "room_id": room_id
            })
        })
        .collect();

    Ok(Json(json!({
        "results": results,
        "count": results.len(),
        "room_id": room_id
    })))
}

/// Get room version
#[axum::debug_handler]
pub async fn get_room_version(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let row = sqlx::query("SELECT room_version FROM rooms WHERE room_id = $1")
        .bind(&room_id)
        .fetch_optional(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match row {
        Some(row) => {
            let version: String = row.get("room_version");
            Ok(Json(json!({
                "room_id": room_id,
                "room_version": version
            })))
        }
        None => Err(ApiError::not_found(format!("Room {} not found", room_id))),
    }
}

/// Get room forward extremities count
#[axum::debug_handler]
pub async fn get_room_forward_extremities(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM room_events
        WHERE room_id = $1
        AND state_key IS NOT NULL
        AND event_id NOT IN (
            SELECT prev_event_id FROM room_events
            WHERE room_id = $1 AND prev_event_id IS NOT NULL
        )
        "#,
    )
    .bind(&room_id)
    .fetch_one(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "forward_extremities": count
    })))
}

#[derive(Debug, Deserialize)]
pub struct SearchAllRoomsRequest {
    pub search_term: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub order_by: Option<String>,
    pub is_public: Option<bool>,
    pub is_encrypted: Option<bool>,
}

#[axum::debug_handler]
pub async fn search_all_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<SearchAllRoomsRequest>,
) -> Result<Json<Value>, ApiError> {
    search_all_rooms_impl(&state, body).await
}

#[axum::debug_handler]
pub async fn search_all_rooms_query(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<SearchAllRoomsRequest>,
) -> Result<Json<Value>, ApiError> {
    search_all_rooms_impl(&state, query).await
}

async fn search_all_rooms_impl(
    state: &AppState,
    body: SearchAllRoomsRequest,
) -> Result<Json<Value>, ApiError> {
    let limit = body.limit.unwrap_or(50).min(200) as i64;
    let offset = body.offset.unwrap_or(0) as i64;
    let pool = &*state.services.room_storage.pool;

    let search_pattern = body.search_term.as_ref().map(|term| format!("%{}%", term));

    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        r#"
        SELECT r.room_id, r.name, r.topic, r.creator, r.is_public, r.created_ts as creation_ts,
               COUNT(DISTINCT rm.user_id) as member_count,
               CASE WHEN COUNT(DISTINCT re.event_id) > 0 THEN TRUE ELSE FALSE END as is_encrypted
        FROM rooms r
        LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
        LEFT JOIN room_events re ON r.room_id = re.room_id AND re.event_type = 'm.room.encryption'
        WHERE 1=1
        "#,
    );

    if let Some(pattern) = &search_pattern {
        query.push(" AND (r.name ILIKE ");
        query.push_bind(pattern);
        query.push(" OR r.topic ILIKE ");
        query.push_bind(pattern);
        query.push(" OR r.room_id ILIKE ");
        query.push_bind(pattern);
        query.push(")");
    }

    if let Some(is_public) = body.is_public {
        query.push(" AND r.is_public = ");
        query.push_bind(is_public);
    }

    if let Some(is_encrypted) = body.is_encrypted {
        if is_encrypted {
            query.push(
                " AND EXISTS (SELECT 1 FROM room_events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.event_type = 'm.room.encryption')",
            );
        } else {
            query.push(
                " AND NOT EXISTS (SELECT 1 FROM room_events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.event_type = 'm.room.encryption')",
            );
        }
    }

    query.push(" GROUP BY r.room_id, r.name, r.topic, r.creator, r.is_public, r.created_ts");

    let order_by_clause = match body.order_by.as_deref() {
        Some("name") => " ORDER BY r.name ASC NULLS LAST, r.created_ts DESC",
        Some("size") => " ORDER BY member_count DESC, r.created_ts DESC",
        Some("created") => " ORDER BY r.created_ts DESC",
        _ => " ORDER BY r.created_ts DESC",
    };
    query.push(order_by_clause);

    query.push(" LIMIT ");
    query.push_bind(limit);
    query.push(" OFFSET ");
    query.push_bind(offset);

    let rooms = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Search failed: {}", e)))?;

    let mut count_query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT COUNT(*) as total FROM rooms r WHERE 1=1",
    );

    if let Some(pattern) = &search_pattern {
        count_query.push(" AND (r.name ILIKE ");
        count_query.push_bind(pattern);
        count_query.push(" OR r.topic ILIKE ");
        count_query.push_bind(pattern);
        count_query.push(" OR r.room_id ILIKE ");
        count_query.push_bind(pattern);
        count_query.push(")");
    }

    if let Some(is_public) = body.is_public {
        count_query.push(" AND r.is_public = ");
        count_query.push_bind(is_public);
    }

    if let Some(is_encrypted) = body.is_encrypted {
        if is_encrypted {
            count_query.push(
                " AND EXISTS (SELECT 1 FROM room_events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.event_type = 'm.room.encryption')",
            );
        } else {
            count_query.push(
                " AND NOT EXISTS (SELECT 1 FROM room_events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.event_type = 'm.room.encryption')",
            );
        }
    }

    let total_row = count_query
        .build()
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::internal(format!("Count failed: {}", e)))?;
    let total: i64 = total_row.get("total");

    let results: Vec<Value> = rooms
        .iter()
        .map(|r| {
            json!({
                "room_id": r.get::<String, _>("room_id"),
                "name": r.get::<Option<String>, _>("name"),
                "topic": r.get::<Option<String>, _>("topic"),
                "creator": r.get::<Option<String>, _>("creator"),
                "is_public": r.get::<bool, _>("is_public"),
                "member_count": r.get::<i64, _>("member_count"),
                "is_encrypted": r.get::<bool, _>("is_encrypted"),
                "creation_ts": r.get::<i64, _>("creation_ts")
            })
        })
        .collect();

    Ok(Json(json!({
        "results": results,
        "count": results.len(),
        "total": total,
        "offset": offset,
        "limit": limit
    })))
}
