use crate::common::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Json, Path, Query, State},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;

pub fn create_search_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v3/search", post(search))
        .route("/_matrix/client/r0/search", post(search))
        .route(
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads",
            get(get_threads),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/hierarchy",
            get(get_room_hierarchy),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/timestamp_to_event",
            get(timestamp_to_event),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/context/{event_id}",
            get(get_event_context),
        )
        .with_state(state)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    pub search_categories: SearchCategories,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchCategories {
    #[serde(default)]
    pub room_events: Option<RoomEventsSearch>,
    #[serde(default)]
    pub users: Option<UsersSearch>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RoomEventsSearch {
    pub search_term: String,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub filter: Option<Filter>,
    #[serde(default)]
    pub groupings: Option<Groupings>,
    #[serde(default = "default_order_by")]
    pub order_by: String,
    #[serde(default)]
    pub next_batch: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsersSearch {
    pub search_term: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Filter {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub rooms: Option<Vec<String>>,
    #[serde(default)]
    pub not_rooms: Option<Vec<String>>,
    #[serde(default)]
    pub types: Option<Vec<String>>,
    #[serde(default)]
    pub not_types: Option<Vec<String>>,
    #[serde(default)]
    pub senders: Option<Vec<String>>,
    #[serde(default)]
    pub not_senders: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Groupings {
    #[serde(default)]
    pub group_by: Vec<GroupBy>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupBy {
    #[serde(rename = "key")]
    pub key: String,
}

fn default_order_by() -> String {
    "rank".to_string()
}

async fn search(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<SearchRequest>,
) -> Result<Json<Value>, ApiError> {
    let next_batch = params.get("next_batch").cloned();

    let mut results = json!({
        "search_categories": {}
    });

    if let Some(room_events) = &body.search_categories.room_events {
        let room_results = search_room_events(
            &state,
            &auth_user.user_id,
            room_events,
            next_batch.as_deref(),
        )
        .await?;
        results["search_categories"]["room_events"] = room_results;
    }

    if let Some(users_search) = &body.search_categories.users {
        let user_results = search_users(&state, &auth_user.user_id, users_search).await?;
        results["search_categories"]["users"] = user_results;
    }

    Ok(Json(results))
}

async fn search_room_events(
    state: &AppState,
    _user_id: &str,
    search: &RoomEventsSearch,
    _next_batch: Option<&str>,
) -> Result<Value, ApiError> {
    let limit = search.filter.as_ref().and_then(|f| f.limit).unwrap_or(10) as i64;
    let search_pattern = format!("%{}%", search.search_term.to_lowercase());

    let mut query_builder = sqlx::QueryBuilder::new(
        "SELECT event_id, room_id, sender, type, content, origin_server_ts FROM events WHERE ",
    );

    query_builder.push("(LOWER(content::text) LIKE ");
    query_builder.push_bind(&search_pattern);
    query_builder.push(" OR LOWER(sender) LIKE ");
    query_builder.push_bind(&search_pattern);
    query_builder.push(")");

    if let Some(filter) = &search.filter {
        if let Some(rooms) = &filter.rooms {
            if !rooms.is_empty() {
                query_builder.push(" AND room_id IN (");
                for (i, room) in rooms.iter().enumerate() {
                    if i > 0 {
                        query_builder.push(", ");
                    }
                    query_builder.push_bind(room);
                }
                query_builder.push(")");
            }
        }

        if let Some(not_rooms) = &filter.not_rooms {
            if !not_rooms.is_empty() {
                query_builder.push(" AND room_id NOT IN (");
                for (i, room) in not_rooms.iter().enumerate() {
                    if i > 0 {
                        query_builder.push(", ");
                    }
                    query_builder.push_bind(room);
                }
                query_builder.push(")");
            }
        }

        if let Some(types) = &filter.types {
            if !types.is_empty() {
                query_builder.push(" AND type IN (");
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        query_builder.push(", ");
                    }
                    query_builder.push_bind(t);
                }
                query_builder.push(")");
            }
        }

        if let Some(senders) = &filter.senders {
            if !senders.is_empty() {
                query_builder.push(" AND sender IN (");
                for (i, sender) in senders.iter().enumerate() {
                    if i > 0 {
                        query_builder.push(", ");
                    }
                    query_builder.push_bind(sender);
                }
                query_builder.push(")");
            }
        }
    }

    query_builder.push(" ORDER BY origin_server_ts DESC LIMIT ");
    query_builder.push_bind(limit);

    let rows = query_builder
        .build()
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let results: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "result": {
                    "event_id": row.get::<Option<String>, _>("event_id"),
                    "room_id": row.get::<Option<String>, _>("room_id"),
                    "sender": row.get::<Option<String>, _>("sender"),
                    "type": row.get::<Option<String>, _>("type"),
                    "content": row.get::<Option<Value>, _>("content").unwrap_or(json!({})),
                    "origin_server_ts": row.get::<Option<i64>, _>("origin_server_ts").unwrap_or(0)
                },
                "rank": 0.5
            })
        })
        .collect();

    let count = results.len() as i64;

    Ok(json!({
        "results": results,
        "count": count,
        "highlights": [],
        "state": {
            "rooms": {}
        },
        "groups": {
            "room_id": {},
            "sender": {}
        },
        "next_batch": null
    }))
}

async fn search_users(
    state: &AppState,
    _user_id: &str,
    search: &UsersSearch,
) -> Result<Value, ApiError> {
    let limit = search.limit.unwrap_or(10) as i64;
    let search_pattern = format!("%{}%", search.search_term.to_lowercase());

    let rows = sqlx::query(
        r#"
        SELECT user_id, displayname, avatar_url
        FROM users
        WHERE LOWER(user_id) LIKE $1 OR LOWER(displayname) LIKE $1
        ORDER BY user_id
        LIMIT $2
        "#,
    )
    .bind(&search_pattern)
    .bind(limit)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let results: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "user_id": row.get::<Option<String>, _>("user_id"),
                "display_name": row.get::<Option<String>, _>("displayname"),
                "avatar_url": row.get::<Option<String>, _>("avatar_url")
            })
        })
        .collect();

    Ok(json!({
        "results": results,
        "limited": results.len() as i64 == limit
    }))
}

async fn get_threads(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((_user_id, room_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let from = params.get("from").cloned();

    let member_check =
        sqlx::query("SELECT 1 FROM room_members WHERE room_id = $1 AND user_id = $2")
            .bind(&room_id)
            .bind(&auth_user.user_id)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if member_check.is_none() {
        return Err(ApiError::forbidden("Not a member of this room".to_string()));
    }

    let threads = sqlx::query(
        r#"
        SELECT DISTINCT ON (event_id) event_id, sender, content, origin_server_ts
        FROM events
        WHERE room_id = $1
          AND content::jsonb ? 'm.relates_to'
          AND content::jsonb->'m.relates_to' ? 'rel_type'
          AND content::jsonb->'m.relates_to'->>'rel_type' = 'm.thread'
        ORDER BY origin_server_ts DESC
        LIMIT $2
        "#,
    )
    .bind(&room_id)
    .bind(limit as i64)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let chunk: Vec<Value> = threads
        .iter()
        .map(|row| {
            json!({
                "event_id": row.get::<Option<String>, _>("event_id"),
                "sender": row.get::<Option<String>, _>("sender"),
                "content": row.get::<Option<Value>, _>("content").unwrap_or(json!({})),
                "origin_server_ts": row.get::<Option<i64>, _>("origin_server_ts").unwrap_or(0)
            })
        })
        .collect();

    Ok(Json(json!({
        "chunk": chunk,
        "next_batch": from
    })))
}

async fn get_room_hierarchy(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);

    let max_depth = params
        .get("max_depth")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let rooms = sqlx::query(
        r#"
        SELECT room_id, name, topic, avatar_url, join_rules, guest_access, history_visibility
        FROM rooms
        WHERE room_id = $1 OR room_id IN (
            SELECT room_id FROM room_parents WHERE parent_id = $1
        )
        LIMIT $2
        "#,
    )
    .bind(&room_id)
    .bind(limit as i64)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let rooms_list: Vec<Value> = rooms
        .iter()
        .map(|row| {
            let history_visibility = row.get::<Option<String>, _>("history_visibility").unwrap_or_else(|| "shared".to_string());
            let world_readable = history_visibility == "world_readable";
            json!({
                "room_id": row.get::<Option<String>, _>("room_id"),
                "name": row.get::<Option<String>, _>("name"),
                "topic": row.get::<Option<String>, _>("topic"),
                "avatar_url": row.get::<Option<String>, _>("avatar_url"),
                "join_rule": row.get::<Option<String>, _>("join_rules").unwrap_or_else(|| "public".to_string()),
                "guest_access": row.get::<Option<String>, _>("guest_access").unwrap_or_else(|| "can_join".to_string()),
                "world_readable": world_readable,
                "num_joined_members": 0,
                "children_state": []
            })
        })
        .collect();

    Ok(Json(json!({
        "rooms": rooms_list,
        "max_depth": max_depth
    })))
}

async fn timestamp_to_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let ts: i64 = params
        .get("ts")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::bad_request("Missing ts parameter".to_string()))?;

    let dir = params.get("dir").map(|v| v.as_str()).unwrap_or("f");

    let member_check =
        sqlx::query("SELECT 1 FROM room_members WHERE room_id = $1 AND user_id = $2")
            .bind(&room_id)
            .bind(&auth_user.user_id)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if member_check.is_none() {
        return Err(ApiError::forbidden("Not a member of this room".to_string()));
    }

    let event = if dir == "b" {
        sqlx::query(
            "SELECT event_id, origin_server_ts FROM events WHERE room_id = $1 AND origin_server_ts <= $2 ORDER BY origin_server_ts DESC LIMIT 1"
        )
        .bind(&room_id)
        .bind(ts)
        .fetch_optional(&*state.services.user_storage.pool)
        .await
    } else {
        sqlx::query(
            "SELECT event_id, origin_server_ts FROM events WHERE room_id = $1 AND origin_server_ts >= $2 ORDER BY origin_server_ts ASC LIMIT 1"
        )
        .bind(&room_id)
        .bind(ts)
        .fetch_optional(&*state.services.user_storage.pool)
        .await
    }
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match event {
        Some(row) => Ok(Json(json!({
            "event_id": row.get::<Option<String>, _>("event_id"),
            "origin_server_ts": row.get::<Option<i64>, _>("origin_server_ts").unwrap_or(0)
        }))),
        None => Err(ApiError::not_found(
            "No event found at this timestamp".to_string(),
        )),
    }
}

async fn get_event_context(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let member_check =
        sqlx::query("SELECT 1 FROM room_members WHERE room_id = $1 AND user_id = $2")
            .bind(&room_id)
            .bind(&auth_user.user_id)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if member_check.is_none() {
        return Err(ApiError::forbidden("Not a member of this room".to_string()));
    }

    let target_event = sqlx::query(
        "SELECT event_id, sender, type, content, origin_server_ts FROM events WHERE room_id = $1 AND event_id = $2"
    )
    .bind(&room_id)
    .bind(&event_id)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let target_event = match target_event {
        Some(e) => e,
        None => return Err(ApiError::not_found("Event not found".to_string())),
    };

    let target_ts = target_event
        .get::<Option<i64>, _>("origin_server_ts")
        .unwrap_or(0);

    let events_before = sqlx::query(
        "SELECT event_id, sender, type, content, origin_server_ts FROM events WHERE room_id = $1 AND origin_server_ts < $2 ORDER BY origin_server_ts DESC LIMIT $3"
    )
    .bind(&room_id)
    .bind(target_ts)
    .bind(limit as i64)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let events_after = sqlx::query(
        "SELECT event_id, sender, type, content, origin_server_ts FROM events WHERE room_id = $1 AND origin_server_ts > $2 ORDER BY origin_server_ts ASC LIMIT $3"
    )
    .bind(&room_id)
    .bind(target_ts)
    .bind(limit as i64)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let events_before_list: Vec<Value> = events_before
        .iter()
        .map(|row| {
            json!({
                "event_id": row.get::<Option<String>, _>("event_id"),
                "sender": row.get::<Option<String>, _>("sender"),
                "type": row.get::<Option<String>, _>("type"),
                "content": row.get::<Option<Value>, _>("content").unwrap_or(json!({})),
                "origin_server_ts": row.get::<Option<i64>, _>("origin_server_ts").unwrap_or(0)
            })
        })
        .collect();

    let events_after_list: Vec<Value> = events_after
        .iter()
        .map(|row| {
            json!({
                "event_id": row.get::<Option<String>, _>("event_id"),
                "sender": row.get::<Option<String>, _>("sender"),
                "type": row.get::<Option<String>, _>("type"),
                "content": row.get::<Option<Value>, _>("content").unwrap_or(json!({})),
                "origin_server_ts": row.get::<Option<i64>, _>("origin_server_ts").unwrap_or(0)
            })
        })
        .collect();

    Ok(Json(json!({
        "event": {
            "event_id": target_event.get::<Option<String>, _>("event_id"),
            "sender": target_event.get::<Option<String>, _>("sender"),
            "type": target_event.get::<Option<String>, _>("type"),
            "content": target_event.get::<Option<Value>, _>("content").unwrap_or(json!({})),
            "origin_server_ts": target_ts
        },
        "events_before": events_before_list,
        "events_after": events_after_list,
        "state": [],
        "start": events_before_list.last().and_then(|e| e.get("event_id").and_then(|v| v.as_str())).unwrap_or(&event_id),
        "end": events_after_list.last().and_then(|e| e.get("event_id").and_then(|v| v.as_str())).unwrap_or(&event_id)
    })))
}
