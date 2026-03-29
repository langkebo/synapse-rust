use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
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
            post(make_room_admin),
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
            "/_synapse/admin/v1/rooms/{room_id}/unban/{user_id}",
            post(unban_user),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/kick/{user_id}",
            post(kick_user),
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
            "/_synapse/admin/v1/rooms/{room_id}/search",
            post(search_room_messages_admin),
        )
        .route("/_synapse/admin/v1/rooms/search", post(search_all_rooms))
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/forward_extremities",
            get(get_room_forward_extremities),
        )
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
    Json(body): Json<BlockRoomRequest>,
) -> Result<Json<Value>, ApiError> {
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

    Ok(Json(json!({ "block": body.block })))
}

#[axum::debug_handler]
pub async fn get_room_block_status(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
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
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query("DELETE FROM blocked_rooms WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({ "block": false })))
}

#[axum::debug_handler]
pub async fn make_room_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<MakeRoomAdminRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    let event_id = crate::common::crypto::generate_event_id(&state.services.config.server.name);
    let user_id = body.user_id.clone();
    let admin_user = "@admin:".to_string() + &state.services.config.server.name;

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
    .bind(json!({
        "users": {
            "user_id": user_id.clone(),
            "power_level": 100
        }
    }))
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
        "SELECT room_id, name, topic, creator, creation_ts FROM rooms WHERE room_type = 'm.space' ORDER BY creation_ts DESC"
    )
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let space_list: Vec<Value> = spaces
        .iter()
        .map(|row| {
            json!({
                "room_id": row.get::<String, _>("room_id"),
                "name": row.get::<Option<String>, _>("name"),
                "topic": row.get::<Option<String>, _>("topic"),
                "creator": row.get::<String, _>("creator"),
                "created_ts": row.get::<i64, _>("creation_ts")
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
        "SELECT room_id, name, topic, creator, creation_ts FROM rooms WHERE room_id = $1 AND room_type = 'm.space'"
    )
    .bind(&space_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match space {
        Some(row) => Ok(Json(json!({
            "room_id": row.get::<String, _>("room_id"),
            "name": row.get::<Option<String>, _>("name"),
            "topic": row.get::<Option<String>, _>("topic"),
            "creator": row.get::<String, _>("creator"),
            "created_ts": row.get::<i64, _>("creation_ts")
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
    let result = sqlx::query("DELETE FROM rooms WHERE room_id = $1 AND room_type = 'm.space'")
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
    let users = sqlx::query("SELECT user_id FROM room_memberships WHERE room_id = $1")
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
    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM room_memberships WHERE room_id = $1")
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
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO room_memberships (user_id, room_id, membership, joined_ts)
        VALUES ($1, $2, 'join', $3)
        ON CONFLICT (user_id, room_id) DO UPDATE SET membership = 'join'
        "#,
    )
    .bind(&user_id)
    .bind(&room_id)
    .bind(now)
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "user_id": user_id,
        "room_id": room_id,
        "membership": "join"
    })))
}

/// Remove a user from a room
#[axum::debug_handler]
pub async fn remove_room_member(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query("DELETE FROM room_memberships WHERE user_id = $1 AND room_id = $2")
        .bind(&user_id)
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "user_id": user_id,
        "room_id": room_id,
        "removed": true
    })))
}

/// Ban a user from a room
#[derive(Debug, Deserialize)]
pub struct BanRequest {
    pub reason: Option<String>,
}

#[axum::debug_handler]
pub async fn ban_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<BanRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();

    // First remove from room if joined
    sqlx::query("DELETE FROM room_memberships WHERE user_id = $1 AND room_id = $2")
        .bind(&user_id)
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .ok();

    // Then add ban
    sqlx::query(
        r#"
        INSERT INTO room_memberships (user_id, room_id, membership, joined_ts, ban_reason)
        VALUES ($1, $2, 'ban', $3, $4)
        ON CONFLICT (user_id, room_id) DO UPDATE SET membership = 'ban', ban_reason = $4
        "#,
    )
    .bind(&user_id)
    .bind(&room_id)
    .bind(now)
    .bind(&body.reason)
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "user_id": user_id,
        "room_id": room_id,
        "membership": "ban"
    })))
}

/// Unban a user from a room
#[axum::debug_handler]
pub async fn unban_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query(
        "DELETE FROM room_memberships WHERE user_id = $1 AND room_id = $2 AND membership = 'ban'",
    )
    .bind(&user_id)
    .bind(&room_id)
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "user_id": user_id,
        "room_id": room_id,
        "unbanned": true
    })))
}

/// Kick a user from a room
#[axum::debug_handler]
pub async fn kick_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<BanRequest>,
) -> Result<Json<Value>, ApiError> {
    // Remove from room
    sqlx::query("DELETE FROM room_memberships WHERE user_id = $1 AND room_id = $2")
        .bind(&user_id)
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "user_id": user_id,
        "room_id": room_id,
        "kicked": true,
        "reason": body.reason
    })))
}

/// Get room listings (public/directory status)
#[axum::debug_handler]
pub async fn get_room_listings(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let is_public: bool = sqlx::query_scalar("SELECT is_public FROM rooms WHERE room_id = $1")
        .bind(&room_id)
        .fetch_one(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
    sqlx::query("UPDATE rooms SET is_public = true WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
    sqlx::query("UPDATE rooms SET is_public = false WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    // Remove from directory
    sqlx::query("DELETE FROM room_directory WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .ok();

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
    let event = sqlx::query(
        "SELECT event_id, type, sender, state_key, content, room_id, origin_server_ts FROM room_events WHERE event_id = $1 AND room_id = $2"
    )
    .bind(&event_id)
    .bind(&room_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match event {
        Some(row) => {
            let events_before: Vec<Value> = sqlx::query(
                "SELECT event_id, type, sender, content, origin_server_ts FROM room_events WHERE room_id = $1 AND origin_server_ts < (SELECT origin_server_ts FROM room_events WHERE event_id = $2) ORDER BY origin_server_ts DESC LIMIT 5"
            )
            .bind(&room_id)
            .bind(&event_id)
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
                "SELECT event_id, type, sender, content, origin_server_ts FROM room_events WHERE room_id = $1 AND origin_server_ts > (SELECT origin_server_ts FROM room_events WHERE event_id = $2) ORDER BY origin_server_ts ASC LIMIT 5"
            )
            .bind(&room_id)
            .bind(&event_id)
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
                    "event_id": row.get::<String, _>("event_id"),
                    "type": row.get::<String, _>("type"),
                    "sender": row.get::<String, _>("sender"),
                    "state_key": row.get::<Option<String>, _>("state_key"),
                    "content": row.get::<Value, _>("content"),
                    "room_id": row.get::<String, _>("room_id"),
                    "origin_server_ts": row.get::<i64, _>("origin_server_ts")
                },
                "events_before": events_before,
                "events_after": events_after,
                "state": []
            })))
        }
        None => Err(ApiError::not_found("Event not found".to_string())),
    }
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
    let limit = body.limit.unwrap_or(50).min(200) as i64;
    let search_pattern = format!("%{}%", body.search_term.to_lowercase());

    let events = sqlx::query(
        r#"
        SELECT event_id, type, sender, content, origin_server_ts
        FROM room_events
        WHERE room_id = $1 AND type = 'm.room.message' AND LOWER(content::text) LIKE $2
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
                "type": r.get::<String, _>("type"),
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
    let limit = body.limit.unwrap_or(50).min(200) as i64;
    let offset = body.offset.unwrap_or(0) as i64;
    let pool = &*state.services.room_storage.pool;

    let search_pattern = body.search_term.as_ref().map(|term| format!("%{}%", term));

    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        r#"
        SELECT r.room_id, r.name, r.topic, r.creator, r.is_public, r.creation_ts,
               COUNT(DISTINCT rm.user_id) as member_count,
               CASE WHEN COUNT(DISTINCT re.event_id) > 0 THEN TRUE ELSE FALSE END as is_encrypted
        FROM rooms r
        LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
        LEFT JOIN room_events re ON r.room_id = re.room_id AND re.type = 'm.room.encryption'
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
                " AND EXISTS (SELECT 1 FROM room_events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.type = 'm.room.encryption')",
            );
        } else {
            query.push(
                " AND NOT EXISTS (SELECT 1 FROM room_events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.type = 'm.room.encryption')",
            );
        }
    }

    query.push(" GROUP BY r.room_id, r.name, r.topic, r.creator, r.is_public, r.creation_ts");

    let order_by_clause = match body.order_by.as_deref() {
        Some("name") => " ORDER BY r.name ASC NULLS LAST, r.creation_ts DESC",
        Some("size") => " ORDER BY member_count DESC, r.creation_ts DESC",
        Some("created") => " ORDER BY r.creation_ts DESC",
        _ => " ORDER BY r.creation_ts DESC",
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
                " AND EXISTS (SELECT 1 FROM room_events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.type = 'm.room.encryption')",
            );
        } else {
            count_query.push(
                " AND NOT EXISTS (SELECT 1 FROM room_events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.type = 'm.room.encryption')",
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
