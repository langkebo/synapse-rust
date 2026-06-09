pub mod types;
pub mod management;
pub mod spaces;

use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use crate::services::{
    decode_room_search_cursor, decode_room_token_sync_cursor, encode_room_token_sync_cursor, RoomSearchCursor,
    RoomSearchOrder, RoomTokenSyncCursor,
};
use crate::web::routes::admin::room::types::{
    RoomTokenSyncQueryParams, SearchAllRoomsRequest, SearchRoomMessagesRequest,
};
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::{json, Value};

#[cfg(test)]
mod cursor_tests {
    use super::{
        decode_room_search_cursor, decode_room_token_sync_cursor, encode_room_token_sync_cursor, RoomSearchCursor,
        RoomTokenSyncCursor,
    };
    use crate::services::encode_room_search_cursor;

    #[test]
    fn test_room_search_created_cursor_round_trip() {
        let cursor = encode_room_search_cursor(&RoomSearchCursor::Created {
            created_ts: 1_700_000_000_000,
            room_id: "!room:example.com".to_string(),
        });
        assert_eq!(
            decode_room_search_cursor(Some(&cursor)),
            Some(RoomSearchCursor::Created { created_ts: 1_700_000_000_000, room_id: "!room:example.com".to_string() })
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
        assert_eq!(decode_room_search_cursor(Some("name|0|bad%%%|123|!room:example.com")), None);
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
        .route("/_synapse/admin/v1/rooms/{room_id}/block", post(management::block_room))
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/block",
            get(management::get_room_block_status),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/unblock",
            post(management::unblock_room),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/make_admin",
            post(management::make_room_admin).put(management::make_room_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/purge_history",
            post(management::purge_history_by_room),
        )
        .route("/_synapse/admin/v1/purge_history", post(management::purge_history))
        .route("/_synapse/admin/v1/purge_room", post(management::purge_room))
        .route("/_synapse/admin/v1/shutdown_room", post(shutdown_room))
        .route("/_synapse/admin/v1/spaces", get(spaces::get_spaces))
        .route("/_synapse/admin/v1/spaces/{space_id}", get(spaces::get_space))
        .route("/_synapse/admin/v1/spaces/{space_id}", delete(spaces::delete_space))
        .route(
            "/_synapse/admin/v1/spaces/{space_id}/users",
            get(spaces::get_space_users),
        )
        .route(
            "/_synapse/admin/v1/spaces/{space_id}/rooms",
            get(spaces::get_space_rooms),
        )
        .route(
            "/_synapse/admin/v1/spaces/{space_id}/stats",
            get(spaces::get_space_stats),
        )
        // Room statistics
        .route("/_synapse/admin/v1/room_stats", get(spaces::get_room_stats))
        .route(
            "/_synapse/admin/v1/room_stats/{room_id}",
            get(spaces::get_single_room_stats),
        )
        // Room membership management
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
            put(management::join_room_member),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
            delete(management::remove_room_member),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/ban/{user_id}",
            post(management::ban_user),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/ban",
            post(management::ban_user_by_body),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/unban/{user_id}",
            post(management::unban_user),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/kick/{user_id}",
            post(management::kick_user),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/kick",
            post(management::kick_user_by_body),
        )
        // Room listing
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/listings",
            get(spaces::get_room_listings),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/listings/public",
            put(spaces::set_room_public),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/listings/public",
            delete(spaces::set_room_private),
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
            post(management::cleanup_abnormal_rooms),
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
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/make_admin"),
        (Method::PUT, "/_synapse/admin/v1/rooms/{room_id}/make_admin"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/purge_history"),
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
        (Method::PUT, "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}"),
        (Method::DELETE, "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/ban/{user_id}"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/ban"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/unban/{user_id}"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/kick/{user_id}"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/kick"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/listings"),
        (Method::PUT, "/_synapse/admin/v1/rooms/{room_id}/listings/public"),
        (Method::DELETE, "/_synapse/admin/v1/rooms/{room_id}/listings/public"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/event_context/{event_id}"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/token_sync"),
        (Method::POST, "/_synapse/admin/v1/rooms/{room_id}/search"),
        (Method::POST, "/_synapse/admin/v1/rooms/search"),
        (Method::GET, "/_synapse/admin/v1/rooms/search"),
        (Method::GET, "/_synapse/admin/v1/rooms/{room_id}/forward_extremities"),
        (Method::POST, "/_synapse/admin/v1/rooms/cleanup"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::room"))
    .collect()
}

#[axum::debug_handler]
pub async fn get_room_aliases_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    Ok(Json(json!({
        "aliases": []
    })))
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
        _ => return Err(ApiError::bad_request("Cursor does not match requested order_by".to_string())),
    }

    let (rooms_with_members, next_batch) =
        state.services.rooms.room_storage.get_all_rooms_with_members(limit, cursor, order).await.map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

    let total = state.services.rooms.room_storage.get_room_count().await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

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
    let room = state.services.rooms.room_storage.get_room(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

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
    if !state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state.services.rooms.room_storage.delete_room(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

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
    if !state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let members = state.services.rooms.member_storage.get_room_members(&room_id, "join").await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

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
    if !state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let events = state.services.rooms.event_storage.get_state_events(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

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
    if !state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);

    let events = state.services.rooms.event_storage.get_room_events(&room_id, limit).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

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
pub async fn shutdown_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;

    if !state.services.rooms.room_storage.room_exists(room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state
        .services.rooms.room_storage
        .shutdown_room(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to shutdown room", &e))?;

    state.services.rooms.member_storage.remove_all_members(room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

    Ok(Json(json!({
        "kicked_users": [],
        "failed_to_kick_users": [],
        "closed_room": true
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

    let room_exists = state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let event = state
        .services.rooms.event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::not_found("Event not found in this room".to_string()));
    }

    let events_before =
        state.services.rooms.event_storage.get_events_before_context(&room_id, event.origin_server_ts, 5).await.map_err(
            |e| {
                tracing::error!("Database error: {e}");
                ApiError::database("A database error occurred".to_string())
            },
        )?;

    let events_after =
        state.services.rooms.event_storage.get_events_after_context(&room_id, event.origin_server_ts, 5).await.map_err(
            |e| {
                tracing::error!("Database error: {e}");
                ApiError::database("A database error occurred".to_string())
            },
        )?;

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

#[axum::debug_handler]
pub async fn get_room_token_sync_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<RoomTokenSyncQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = params.limit.unwrap_or(100).clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let cursor = decode_room_token_sync_cursor(params.from.as_deref());
    if params.from.is_some() && cursor.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }
    if params.offset.unwrap_or(0) > 0 && cursor.is_none() {
        return Err(ApiError::bad_request(
            "Offset pagination is no longer supported for this endpoint; use from".to_string(),
        ));
    }

    let (entries, total) = state.services.rooms.sliding_sync_service.get_room_token_sync(&room_id, limit, cursor).await?;

    let has_more = entries.len() as i64 > limit;
    let visible_entries = if has_more { &entries[..limit as usize] } else { &entries[..] };
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

    let active_token_count = visible_entries.iter().filter(|entry| entry.pos.is_some() && !entry.is_expired).count();
    let expired_token_count = visible_entries.iter().filter(|entry| entry.is_expired).count();
    let distinct_users =
        visible_entries.iter().map(|entry| entry.user_id.clone()).collect::<std::collections::HashSet<_>>().len();
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

#[axum::debug_handler]
pub async fn search_room_messages_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<SearchRoomMessagesRequest>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = body.limit.unwrap_or(50).min(200) as i64;
    let search_pattern = format!("%{}%", body.search_term.to_lowercase());

    let events = state
        .services.rooms.event_storage
        .search_room_messages_admin(&room_id, &search_pattern, limit)
        .await
        .map_err(|e| ApiError::internal_with_log("Search failed", &e))?;

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
    let version = state.services.rooms.room_storage.get_room_version_only(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

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
    let room_exists = state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let count = state.services.rooms.event_storage.get_forward_extremities_count(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

    Ok(Json(json!({
        "room_id": room_id,
        "forward_extremities": count
    })))
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

async fn search_all_rooms_impl(state: &AppState, body: SearchAllRoomsRequest) -> Result<Json<Value>, ApiError> {
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
        _ => return Err(ApiError::bad_request("Cursor does not match requested order_by".to_string())),
    }

    let (results, total, next_batch) = state
        .services.rooms.room_storage
        .search_all_rooms_admin(body.search_term.as_deref(), limit, order, cursor, body.is_public, body.is_encrypted)
        .await
        .map_err(|e| ApiError::internal_with_log("Search failed", &e))?;

    Ok(Json(json!({
        "results": results,
        "count": results.len(),
        "total": total,
        "limit": limit,
        "next_batch": next_batch
    })))
}
