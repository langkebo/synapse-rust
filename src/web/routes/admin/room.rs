use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
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
        .route("/_synapse/admin/v1/rooms/{room_id}/delete", post(delete_room))
        .route("/_synapse/admin/v1/rooms/{room_id}/members", get(get_room_members_admin))
        .route("/_synapse/admin/v1/rooms/{room_id}/state", get(get_room_state_admin))
        .route("/_synapse/admin/v1/rooms/{room_id}/messages", get(get_room_messages_admin))
        .route("/_synapse/admin/v1/rooms/{room_id}/block", post(block_room))
        .route("/_synapse/admin/v1/rooms/{room_id}/block", get(get_room_block_status))
        .route("/_synapse/admin/v1/rooms/{room_id}/unblock", post(unblock_room))
        .route("/_synapse/admin/v1/rooms/{room_id}/make_admin", post(make_room_admin))
        .route("/_synapse/admin/v1/purge_history", post(purge_history))
        .route("/_synapse/admin/v1/shutdown_room", post(shutdown_room))
        .route("/_synapse/admin/v1/spaces", get(get_spaces))
        .route("/_synapse/admin/v1/spaces/{space_id}", get(get_space))
        .route("/_synapse/admin/v1/spaces/{space_id}", delete(delete_space))
        .route("/_synapse/admin/v1/spaces/{space_id}/users", get(get_space_users))
        .route("/_synapse/admin/v1/spaces/{space_id}/rooms", get(get_space_rooms))
        .route("/_synapse/admin/v1/spaces/{space_id}/stats", get(get_space_stats))
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
            "join_rule": r.join_rules
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
        .unwrap_or_else(|| {
            chrono::Utc::now().timestamp_millis() - (30 * 24 * 60 * 60 * 1000)
        });

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

    Ok(Json(json!({ "spaces": space_list, "total": space_list.len() })))
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

    Ok(Json(json!({ "users": user_list, "total": user_list.len() })))
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

    Ok(Json(json!({ "rooms": room_list, "total": room_list.len() })))
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
