use super::audit::{record_audit_event, resolve_request_id};
use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use crate::storage::room::{
    decode_room_search_cursor, RoomSearchCursor, RoomSearchOrder,
};
use crate::storage::sliding_sync::{
    decode_room_token_sync_cursor, encode_room_token_sync_cursor, RoomTokenSyncCursor,
};
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[cfg(test)]
mod cursor_tests {
    use crate::storage::room::encode_room_search_cursor;
    use super::{
        decode_room_search_cursor, decode_room_token_sync_cursor, encode_room_token_sync_cursor,
        RoomSearchCursor, RoomTokenSyncCursor,
    };

    #[test]
    fn test_room_search_created_cursor_round_trip() {
        let cursor = encode_room_search_cursor(&RoomSearchCursor::Created {
            created_ts: 1_700_000_000_000,
            room_id: "!room:example.com".to_string(),
        });
        assert_eq!(
            decode_room_search_cursor(Some(&cursor)),
            Some(RoomSearchCursor::Created {
                created_ts: 1_700_000_000_000,
                room_id: "!room:example.com".to_string(),
            })
        );
    }

    #[test]
    fn test_room_search_name_cursor_round_trip() {
        let cursor = encode_room_search_cursor(&RoomSearchCursor::Name {
            name: Some("Alpha|Beta".to_string()),
            created_ts: 1_700_000_000_000,
            room_id: "!room:example.com".to_string(),
        });
        assert_eq!(
            decode_room_search_cursor(Some(&cursor)),
            Some(RoomSearchCursor::Name {
                name: Some("Alpha|Beta".to_string()),
                created_ts: 1_700_000_000_000,
                room_id: "!room:example.com".to_string(),
            })
        );
    }

    #[test]
    fn test_room_search_size_cursor_round_trip() {
        let cursor = encode_room_search_cursor(&RoomSearchCursor::Size {
            member_count: 42,
            created_ts: 1_700_000_000_000,
            room_id: "!room:example.com".to_string(),
        });
        assert_eq!(
            decode_room_search_cursor(Some(&cursor)),
            Some(RoomSearchCursor::Size {
                member_count: 42,
                created_ts: 1_700_000_000_000,
                room_id: "!room:example.com".to_string(),
            })
        );
    }

    #[test]
    fn test_room_search_cursor_rejects_invalid_value() {
        assert_eq!(decode_room_search_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_room_search_cursor(Some("created|123|")), None);
        assert_eq!(
            decode_room_search_cursor(Some("name|0|bad%%%|123|!room:example.com")),
            None
        );
    }

    #[test]
    fn test_room_token_sync_cursor_round_trip() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE".to_string(),
            conn_id: Some("main|conn".to_string()),
        };
        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn test_room_token_sync_cursor_rejects_invalid_value() {
        assert_eq!(decode_room_token_sync_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_room_token_sync_cursor(Some("123|")), None);
    }
}

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
        .route(
            "/_synapse/admin/v1/rooms/cleanup",
            post(cleanup_abnormal_rooms),
        )
}

pub fn admin_room_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/rooms"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}"),
        (Method::DELETE, "/_synapse/admin/v1/rooms/{room_id}"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/delete"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/members"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/state"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/messages"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/aliases"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/version"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/block"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/block"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/unblock"),
        (
            Method::POST,
            "/_synapse/admin/v1/rooms/{room_id}/make_admin",
        ),
        (Method::PUT, "/_synapse/admin/v1/rooms/{room_id}/make_admin"),
        (
            Method::POST,
            "/_synapse/admin/v1/rooms/{room_id}/purge_history",
        ),
        (Method::POST, "/_synapse/admin/v1/purge_history"),
        (Method::POST, "/_synapse/admin/v1/purge_room"),
        (Method::POST, "/_synapse/admin/v1/shutdown_room"),
        (Method::GET, "/_synapse/admin/v1/spaces"),
        (Method::GET, "/_synapse/admin/v1/spaces/{space_id}"),
        (Method::DELETE, "/_synapse/admin/v1/spaces/{space_id}"),
        (Method::GET, "/_synapse/admin/v1/spaces/{space_id}/users"),
        (Method::GET, "/_synapse/admin/v1/spaces/{space_id}/rooms"),
        (Method::GET, "/_synapse/admin/v1/spaces/{space_id}/stats"),
        (Method::GET, "/_synapse/admin/v1/room_stats"),
        (Method::GET, "/_synapse/admin/v1/room_stats/{room_id}"),
        (
            Method::PUT,
            "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
        ),
        (
            Method::DELETE,
            "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
        ),
        (
            Method::POST,
            "/_synapse/admin/v1/rooms/{room_id}/ban/{user_id}",
        ),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/ban"),
        (
            Method::POST,
            "/_synapse/admin/v1/rooms/{room_id}/unban/{user_id}",
        ),
        (
            Method::POST,
            "/_synapse/admin/v1/rooms/{room_id}/kick/{user_id}",
        ),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/kick"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/listings"),
        (
            Method::PUT,
            "/_synapse/admin/v1/rooms/{room_id}/listings/public",
        ),
        (
            Method::DELETE,
            "/_synapse/admin/v1/rooms/{room_id}/listings/public",
        ),
        (
            Method::GET,
            "/_synapse/admin/v1/rooms/{room_id}/event_context/{event_id}",
        ),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/token_sync"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/search"),
        (Method::POST, "/_synapse/admin/v1/rooms/search"),
        (Method::GET, "/_synapse/admin/v1/rooms/search"),
        (
            Method::GET,
            "/_synapse/admin/v1/rooms/{room_id}/forward_extremities",
        ),
        (Method::POST, "/_synapse/admin/v1/rooms/cleanup"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::room"))
    .collect()
}

#[axum::debug_handler]
pub async fn cleanup_abnormal_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let min_age_ms = body.get("min_age_ms").and_then(|v| v.as_i64());

    let results = state
        .services
        .room_storage
        .cleanup_abnormal_data(min_age_ms)
        .await
        .map_err(|e| ApiError::internal(format!("Cleanup failed: {e}")))?;

    Ok(Json(results))
}

async fn resolve_space_id(state: &AppState, identifier: &str) -> Result<String, ApiError> {
    state
        .services
        .space_storage
        .resolve_space_id(identifier)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
        .ok_or_else(|| ApiError::not_found("Space not found".to_string()))
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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
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
    let order = RoomSearchOrder::from_query(params.get("order_by").map(String::as_str));
    let cursor = decode_room_search_cursor(params.get("from").map(String::as_str));

    if params.contains_key("from") && cursor.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }

    match (&order, &cursor) {
        (RoomSearchOrder::Created, Some(RoomSearchCursor::Created { .. }))
        | (RoomSearchOrder::Name, Some(RoomSearchCursor::Name { .. }))
        | (RoomSearchOrder::Size, Some(RoomSearchCursor::Size { .. }))
        | (_, None) => {}
        _ => {
            return Err(ApiError::bad_request(
                "Cursor does not match requested order_by".to_string(),
            ))
        }
    }

    let (rooms_with_members, next_batch) = state
        .services
        .room_storage
        .get_all_rooms_with_members(limit, cursor, order)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    let total = state
        .services
        .room_storage
        .get_room_count()
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
        "total": total,
        "next_batch": next_batch
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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state
        .services
        .room_storage
        .delete_room(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let now = chrono::Utc::now().timestamp_millis();

    state
        .services
        .room_storage
        .block_room(&room_id, now, &admin.user_id, body.reason.as_deref())
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let blocked_at = state
        .services
        .room_storage
        .get_room_block_status(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    match blocked_at {
        Some(blocked_at) => Ok(Json(json!({
            "block": true,
            "blocked_at": blocked_at
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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state
        .services
        .room_storage
        .unblock_room(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<MakeRoomAdminRequest>,
) -> Result<Json<Value>, ApiError> {
    super::ensure_super_admin_for_privilege_change(&admin)?;
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if state
        .services
        .user_storage
        .get_user_by_id(&body.user_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
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

    state
        .services
        .event_storage
        .upsert_power_levels_event(&event_id, &room_id, &user_id, power_levels, now, &admin_user)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let deleted_count = state
        .services
        .event_storage
        .delete_events_before(room_id, timestamp)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to purge history: {e}")))?;

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state
        .services
        .room_storage
        .delete_room(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to purge room: {e}")))?;

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state
        .services
        .room_storage
        .shutdown_room(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to shutdown room: {e}")))?;

    state
        .services
        .member_storage
        .remove_all_members(room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
    let spaces = state
        .services
        .space_storage
        .get_all_spaces_for_admin()
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    let space_list: Vec<Value> = spaces
        .iter()
        .map(|s| {
            json!({
                "space_id": s.space_id,
                "room_id": s.room_id,
                "name": s.name,
                "topic": s.topic,
                "creator": s.creator,
                "created_ts": s.created_ts
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
    let space = state
        .services
        .space_storage
        .get_space_by_identifier(&space_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    match space {
        Some(s) => Ok(Json(json!({
            "space_id": s.space_id,
            "room_id": s.room_id,
            "name": s.name,
            "topic": s.topic,
            "creator": s.creator,
            "created_ts": s.created_ts
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
    let resolved_space_id = resolve_space_id(&state, &space_id).await?;
    let rows_affected = state
        .services
        .space_storage
        .delete_space_returning_count(&resolved_space_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    if rows_affected == 0 {
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
    let resolved_space_id = resolve_space_id(&state, &space_id).await?;

    let user_list = state
        .services
        .space_storage
        .get_space_user_ids(&resolved_space_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
    let resolved_space_id = resolve_space_id(&state, &space_id).await?;

    let room_list = state
        .services
        .space_storage
        .get_space_room_ids(&resolved_space_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
    let resolved_space_id = resolve_space_id(&state, &space_id).await?;

    let (member_count, child_count) = state
        .services
        .space_storage
        .get_space_member_and_child_count(&resolved_space_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    Ok(Json(json!({
        "space_id": resolved_space_id,
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
    let stats = state
        .services
        .room_storage
        .get_room_stats_overview()
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    Ok(Json(stats))
}

/// Get statistics for a single room
#[axum::debug_handler]
pub async fn get_single_room_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let stats = state
        .services
        .room_storage
        .get_single_room_stats(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    match stats {
        Some(stats) => Ok(Json(stats)),
        None => Err(ApiError::not_found("Room not found".to_string())),
    }
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
        .map_err(|e| ApiError::internal(format!("Failed to check room: {e}")))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user: {e}")))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let existing_membership = state
        .services
        .member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
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
        .map_err(|e| ApiError::internal(format!("Failed to check room: {e}")))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user: {e}")))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let existing_membership = state
        .services
        .member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
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
    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room: {e}")))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user: {e}")))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let existing_membership = state
        .services
        .member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
        .map(|member| member.membership);

    let actor_is_admin = state
        .services
        .user_storage
        .get_user_by_id(actor_user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check actor: {e}")))?
        .is_some_and(|user| user.is_admin);

    if actor_is_admin {
        state
            .services
            .member_storage
            .ban_member(room_id, user_id, actor_user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to ban user: {e}")))?;
    } else {
        state
            .services
            .room_service
            .ban_user(room_id, user_id, actor_user_id, reason)
            .await?;
    }

    if existing_membership.as_deref() == Some("join") {
        state
            .services
            .room_storage
            .decrement_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update member count: {e}")))?;
    }

    if let Some(reason) = reason {
        state
            .services
            .member_storage
            .set_ban_reason(room_id, user_id, reason)
            .await
            .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;
    }

    #[cfg(feature = "friends")]
    if let Err(error) = state
        .services
        .friend_room_service
        .sync_dm_room_membership_change(room_id, user_id, "banned", Some(actor_user_id), reason)
        .await
    {
        ::tracing::warn!(
            "Failed to sync friend DM ban state for room {} and user {}: {}",
            room_id,
            user_id,
            error
        );
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
        .map_err(|e| ApiError::internal(format!("Failed to check room: {e}")))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user: {e}")))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .member_storage
        .unban_member(room_id, user_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
    actor_user_id: &str,
    reason: Option<&str>,
) -> Result<Value, ApiError> {
    let existing_membership = state
        .services
        .member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
        .map(|member| member.membership);

    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room: {e}")))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user: {e}")))?
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
            state
                .services
                .member_storage
                .force_leave_membership(room_id, user_id, now)
                .await
                .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;
        }
        None => {}
    }

    #[cfg(feature = "friends")]
    if let Err(error) = state
        .services
        .friend_room_service
        .sync_dm_room_membership_change(room_id, user_id, "kicked", Some(actor_user_id), reason)
        .await
    {
        ::tracing::warn!(
            "Failed to sync friend DM kick state for room {} and user {}: {}",
            room_id,
            user_id,
            error
        );
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
    admin: AdminUser,
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<BanRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        kick_user_internal(
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
pub async fn kick_user_by_body(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<RoomUserActionRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        kick_user_internal(
            &state,
            &room_id,
            &body.user_id,
            &admin.user_id,
            body.reason.as_deref(),
        )
        .await?,
    ))
}

/// Get room listings (public/directory status)
#[axum::debug_handler]
pub async fn get_room_listings(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let listing = state
        .services
        .room_storage
        .get_room_listings_status(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    let Some((is_public, in_directory)) = listing else {
        return Err(ApiError::not_found("Room not found".to_string()));
    };

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
    let found = state
        .services
        .room_storage
        .set_room_public_with_directory(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    if !found {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

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
    let found = state
        .services
        .room_storage
        .set_room_private_with_directory(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    if !found {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::not_found(
            "Event not found in this room".to_string(),
        ));
    }

    let events_before = state
        .services
        .event_storage
        .get_events_before_context(&room_id, event.origin_server_ts, 5)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    let events_after = state
        .services
        .event_storage
        .get_events_after_context(&room_id, event.origin_server_ts, 5)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
    pub from: Option<String>,
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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = params
        .limit
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let cursor = decode_room_token_sync_cursor(params.from.as_deref());
    if params.from.is_some() && cursor.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }
    if params.offset.unwrap_or(0) > 0 && cursor.is_none() {
        return Err(ApiError::bad_request(
            "Offset pagination is no longer supported for this endpoint; use from".to_string(),
        ));
    }

    let (entries, total) = state
        .services
        .sliding_sync_service
        .get_room_token_sync(&room_id, limit, cursor)
        .await?;

    let has_more = entries.len() as i64 > limit;
    let visible_entries = if has_more {
        &entries[..limit as usize]
    } else {
        &entries[..]
    };
    let next_batch = if has_more {
        visible_entries.last().map(|entry| {
            encode_room_token_sync_cursor(&RoomTokenSyncCursor {
                room_updated_ts: entry.room_updated_ts,
                user_id: entry.user_id.clone(),
                device_id: entry.device_id.clone(),
                conn_id: entry.conn_id.clone(),
            })
        })
    } else {
        None
    };

    let active_token_count = visible_entries
        .iter()
        .filter(|entry| entry.pos.is_some() && !entry.is_expired)
        .count();
    let expired_token_count = visible_entries
        .iter()
        .filter(|entry| entry.is_expired)
        .count();
    let distinct_users = visible_entries
        .iter()
        .map(|entry| entry.user_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let distinct_devices = visible_entries
        .iter()
        .map(|entry| format!("{}|{}", entry.user_id, entry.device_id))
        .collect::<std::collections::HashSet<_>>()
        .len();

    let results = visible_entries
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
                "invited": entry.is_invited,
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
        "next_batch": next_batch,
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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = body.limit.unwrap_or(50).min(200) as i64;
    let search_pattern = format!("%{}%", body.search_term.to_lowercase());

    let events = state
        .services
        .event_storage
        .search_room_messages_admin(&room_id, &search_pattern, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Search failed: {e}")))?;

    let results: Vec<Value> = events
        .iter()
        .map(|e| {
            let mut event = e.clone();
            if let Some(obj) = event.as_object_mut() {
                obj.insert("room_id".to_string(), Value::String(room_id.clone()));
            }
            event
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
    let version = state
        .services
        .room_storage
        .get_room_version_only(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    match version {
        Some(version) => Ok(Json(json!({
            "room_id": room_id,
            "room_version": version
        }))),
        None => Err(ApiError::not_found(format!("Room {room_id} not found"))),
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
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let count = state
        .services
        .event_storage
        .get_forward_extremities_count(&room_id)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

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
    pub from: Option<String>,
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
    let order = RoomSearchOrder::from_query(body.order_by.as_deref());
    let cursor = decode_room_search_cursor(body.from.as_deref());

    if body.from.is_some() && cursor.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }

    match (&order, &cursor) {
        (RoomSearchOrder::Created, Some(RoomSearchCursor::Created { .. }))
        | (RoomSearchOrder::Name, Some(RoomSearchCursor::Name { .. }))
        | (RoomSearchOrder::Size, Some(RoomSearchCursor::Size { .. }))
        | (_, None) => {}
        _ => {
            return Err(ApiError::bad_request(
                "Cursor does not match requested order_by".to_string(),
            ))
        }
    }

    let (results, total, next_batch) = state
        .services
        .room_storage
        .search_all_rooms_admin(
            body.search_term.as_deref(),
            limit,
            order,
            cursor,
            body.is_public,
            body.is_encrypted,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Search failed: {e}")))?;

    Ok(Json(json!({
        "results": results,
        "count": results.len(),
        "total": total,
        "limit": limit,
        "next_batch": next_batch
    })))
}
