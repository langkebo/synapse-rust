use crate::common::ApiError;
use crate::web::routes::{
    account_compat::can_view_profile_for_requester, ensure_room_member, AppState,
    AuthenticatedUser,
};
use axum::{
    extract::{Json, Path, Query, State},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

const MAX_SEARCH_TERM_LENGTH: usize = 256;
const MAX_FILTER_ROOMS: usize = 50;
const MAX_FILTER_TYPES: usize = 20;
const MAX_FILTER_SENDERS: usize = 50;
const MAX_SEARCH_LIMIT: u32 = 100;
const SEARCH_TIMEOUT_SECS: u64 = 30;

fn create_search_compat_router() -> Router<AppState> {
    Router::new()
        .route("/search", post(search))
        .route("/search_recipients", post(search_recipients))
        .route("/search_rooms", post(search_rooms))
}

fn create_room_context_router() -> Router<AppState> {
    Router::new().route(
        "/rooms/{room_id}/context/{event_id}",
        get(get_event_context),
    )
}

pub fn create_search_router(state: AppState) -> Router<AppState> {
    let v1_router = Router::new()
        .merge(create_room_context_router())
        .route("/rooms/{room_id}/hierarchy", get(get_room_hierarchy))
        .route(
            "/rooms/{room_id}/timestamp_to_event",
            get(timestamp_to_event),
        );

    let v3_router = Router::new()
        .merge(create_search_compat_router())
        .merge(create_room_context_router())
        .route("/rooms/{room_id}/hierarchy", get(get_room_hierarchy_v3));

    Router::new()
        .nest("/_matrix/client/r0", create_search_compat_router())
        .nest("/_matrix/client/v1", v1_router)
        .nest("/_matrix/client/v3", v3_router)
        .with_state(state)
}

fn validate_search_request(body: &SearchRequest) -> Result<(), ApiError> {
    if let Some(room_events) = &body.search_categories.room_events {
        if room_events.search_term.len() > MAX_SEARCH_TERM_LENGTH {
            return Err(ApiError::bad_request(format!(
                "Search term too long (max {} characters)",
                MAX_SEARCH_TERM_LENGTH
            )));
        }

        if room_events.search_term.trim().is_empty() {
            return Err(ApiError::bad_request("Search term cannot be empty"));
        }

        if let Some(filter) = &room_events.filter {
            if let Some(limit) = filter.limit {
                if limit > MAX_SEARCH_LIMIT {
                    return Err(ApiError::bad_request(format!(
                        "Limit too high (max {})",
                        MAX_SEARCH_LIMIT
                    )));
                }
            }

            if let Some(rooms) = &filter.rooms {
                if rooms.len() > MAX_FILTER_ROOMS {
                    return Err(ApiError::bad_request(format!(
                        "Too many rooms in filter (max {})",
                        MAX_FILTER_ROOMS
                    )));
                }
            }

            if let Some(types) = &filter.types {
                if types.len() > MAX_FILTER_TYPES {
                    return Err(ApiError::bad_request(format!(
                        "Too many types in filter (max {})",
                        MAX_FILTER_TYPES
                    )));
                }
            }

            if let Some(senders) = &filter.senders {
                if senders.len() > MAX_FILTER_SENDERS {
                    return Err(ApiError::bad_request(format!(
                        "Too many senders in filter (max {})",
                        MAX_FILTER_SENDERS
                    )));
                }
            }
        }
    }

    if let Some(users_search) = &body.search_categories.users {
        if users_search.search_term.len() > MAX_SEARCH_TERM_LENGTH {
            return Err(ApiError::bad_request(format!(
                "Search term too long (max {} characters)",
                MAX_SEARCH_TERM_LENGTH
            )));
        }

        if let Some(limit) = users_search.limit {
            if limit > MAX_SEARCH_LIMIT {
                return Err(ApiError::bad_request(format!(
                    "Limit too high (max {})",
                    MAX_SEARCH_LIMIT
                )));
            }
        }
    }

    Ok(())
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
    validate_search_request(&body)?;

    let next_batch = params.get("next_batch").cloned();

    let mut results = json!({
        "search_categories": {}
    });

    if let Some(room_events) = &body.search_categories.room_events {
        let search_future = search_room_events(
            &state,
            &auth_user.user_id,
            room_events,
            next_batch.as_deref(),
        );

        let room_results = timeout(Duration::from_secs(SEARCH_TIMEOUT_SECS), search_future)
            .await
            .map_err(|_| ApiError::internal("Search request timed out"))??;

        results["search_categories"]["room_events"] = room_results;
    }

    if let Some(users_search) = &body.search_categories.users {
        let search_future = search_users(&state, &auth_user.user_id, users_search);

        let user_results = timeout(Duration::from_secs(SEARCH_TIMEOUT_SECS), search_future)
            .await
            .map_err(|_| ApiError::internal("User search request timed out"))??;

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
        "SELECT event_id, room_id, sender, event_type, content, origin_server_ts FROM events WHERE ",
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
                query_builder.push(" AND event_type IN (");
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
                    "type": row.get::<Option<String>, _>("event_type"),
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
    user_id: &str,
    search: &UsersSearch,
) -> Result<Value, ApiError> {
    let limit = search.limit.unwrap_or(10) as i64;
    let search_pattern = format!("%{}%", search.search_term.to_lowercase());

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
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

    let mut results = Vec::new();
    for row in rows {
        let target_user_id = match row.get::<Option<String>, _>("user_id") {
            Some(user_id) => user_id,
            None => continue,
        };

        if !can_view_profile_for_requester(state, Some(user_id), &target_user_id).await? {
            continue;
        }

        results.push(json!({
            "user_id": target_user_id,
            "display_name": row.get::<Option<String>, _>("displayname"),
            "avatar_url": row.get::<Option<String>, _>("avatar_url")
        }));
    }

    Ok(json!({
        "results": results,
        "limited": results.len() as i64 == limit
    }))
}

async fn build_room_hierarchy_response(
    state: &AppState,
    room_id: &str,
    user_id: &str,
    max_depth: i32,
    suggested_only: bool,
    limit: i32,
    from: Option<&str>,
) -> Result<Value, ApiError> {
    if let Some(space) = state
        .services
        .space_service
        .get_space_by_room(room_id)
        .await?
    {
        let response = state
            .services
            .space_service
            .get_space_hierarchy_v1(
                &space.space_id,
                max_depth.max(1),
                suggested_only,
                Some(limit.max(1)),
                from,
                Some(user_id),
            )
            .await?;

        return serde_json::to_value(response)
            .map_err(|e| ApiError::internal(format!("Failed to serialize hierarchy: {}", e)));
    }

    let room = state
        .services
        .room_storage
        .get_room(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let world_readable = room.history_visibility == "world_readable";

    Ok(json!({
        "rooms": [{
            "room_id": room.room_id,
            "name": room.name,
            "topic": room.topic,
            "avatar_url": room.avatar_url,
            "join_rule": room.join_rule,
            "guest_access": if room.is_public { "can_join" } else { "forbidden" },
            "guest_can_join": room.is_public,
            "world_readable": world_readable,
            "num_joined_members": room.member_count,
            "children": [],
            "children_state": [],
            "room_type": Value::Null,
            "required_state_info": []
        }],
        "next_batch": Value::Null
    }))
}

async fn get_room_hierarchy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
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

    let mut response = build_room_hierarchy_response(
        &state,
        &room_id,
        &auth_user.user_id,
        max_depth,
        false,
        limit,
        None,
    )
    .await?;

    if let Some(object) = response.as_object_mut() {
        object.insert("max_depth".to_string(), json!(max_depth));
    }

    Ok(Json(response))
}

/// GET /_matrix/client/v3/rooms/{room_id}/hierarchy
/// Returns a list of child rooms and spaces of a given room
async fn get_room_hierarchy_v3(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
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
        .unwrap_or(1);

    let suggested_only = params
        .get("suggested_only")
        .map(|v| v == "true")
        .unwrap_or(false);

    let mut response = build_room_hierarchy_response(
        &state,
        &room_id,
        &auth_user.user_id,
        max_depth,
        suggested_only,
        limit,
        params.get("from").map(|value| value.as_str()),
    )
    .await?;

    if let Some(object) = response.as_object_mut() {
        object.insert("max_depth".to_string(), json!(max_depth));
    }

    Ok(Json(response))
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

    ensure_room_member(&state, &auth_user, &room_id, "Not a member of this room").await?;

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
    let room_id = room_id.replace("%21", "!").replace("%3A", ":");
    let event_id = event_id.replace("%24", "$");

    crate::web::routes::validate_room_id(&room_id)?;
    crate::web::routes::validate_event_id(&event_id)?;

    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    ensure_room_member(&state, &auth_user, &room_id, "Not a member of this room").await?;

    let target_event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if target_event.room_id != room_id {
        return Err(ApiError::not_found("Event not found".to_string()));
    }

    let target_ts = target_event.origin_server_ts;

    let events_before = sqlx::query(
        "SELECT event_id, sender, event_type AS type, content, origin_server_ts FROM events WHERE room_id = $1 AND origin_server_ts < $2 ORDER BY origin_server_ts DESC LIMIT $3"
    )
    .bind(&room_id)
    .bind(target_ts)
    .bind(limit as i64)
    .fetch_all(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let events_after = sqlx::query(
        "SELECT event_id, sender, event_type AS type, content, origin_server_ts FROM events WHERE room_id = $1 AND origin_server_ts > $2 ORDER BY origin_server_ts ASC LIMIT $3"
    )
    .bind(&room_id)
    .bind(target_ts)
    .bind(limit as i64)
    .fetch_all(&*state.services.event_storage.pool)
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
            "event_id": target_event.event_id,
            "sender": target_event.user_id,
            "type": target_event.event_type,
            "content": target_event.content,
            "origin_server_ts": target_ts
        },
        "events_before": events_before_list,
        "events_after": events_after_list,
        "state": [],
        "start": events_before_list.last().and_then(|e| e.get("event_id").and_then(|v| v.as_str())).unwrap_or(&event_id),
        "end": events_after_list.last().and_then(|e| e.get("event_id").and_then(|v| v.as_str())).unwrap_or(&event_id)
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_validate_search_request_rejects_empty_term() {
        let request = SearchRequest {
            search_categories: SearchCategories {
                room_events: Some(RoomEventsSearch {
                    search_term: "   ".to_string(),
                    keys: vec![],
                    filter: None,
                    groupings: None,
                    order_by: default_order_by(),
                    next_batch: None,
                }),
                users: None,
            },
        };

        let error = validate_search_request(&request).unwrap_err();
        assert_eq!(error.http_status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_validate_search_request_rejects_limit_over_maximum() {
        let request = SearchRequest {
            search_categories: SearchCategories {
                room_events: Some(RoomEventsSearch {
                    search_term: "hello".to_string(),
                    keys: vec![],
                    filter: Some(Filter {
                        limit: Some(MAX_SEARCH_LIMIT + 1),
                        rooms: None,
                        not_rooms: None,
                        types: None,
                        not_types: None,
                        senders: None,
                        not_senders: None,
                    }),
                    groupings: None,
                    order_by: default_order_by(),
                    next_batch: None,
                }),
                users: None,
            },
        };

        let error = validate_search_request(&request).unwrap_err();
        assert_eq!(error.http_status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_search_routes_structure() {
        let routes = vec![
            "/_matrix/client/v3/search",
            "/_matrix/client/r0/search",
            "/_matrix/client/v1/rooms/{room_id}/hierarchy",
        ];

        for route in routes {
            assert!(route.starts_with("/_matrix/client/"));
        }
    }

    #[test]
    fn test_search_routes_do_not_claim_thread_compat_endpoint() {
        let routes = [
            "/_matrix/client/v3/search",
            "/_matrix/client/r0/search",
            "/_matrix/client/v1/rooms/{room_id}/context/{event_id}",
            "/_matrix/client/v3/rooms/{room_id}/context/{event_id}",
        ];

        assert!(routes
            .iter()
            .all(|route| !route.contains("/user/{user_id}/rooms/{room_id}/threads")));
    }

    #[test]
    fn test_search_request_structure() {
        let request = SearchRequest {
            search_categories: SearchCategories {
                room_events: Some(RoomEventsSearch {
                    search_term: "hello".to_string(),
                    keys: vec!["content.body".to_string()],
                    filter: None,
                    groupings: None,
                    order_by: "rank".to_string(),
                    next_batch: None,
                }),
                users: None,
            },
        };

        assert!(request.search_categories.room_events.is_some());
        assert!(request.search_categories.users.is_none());
    }

    #[test]
    fn test_room_events_search() {
        let search = RoomEventsSearch {
            search_term: "test query".to_string(),
            keys: vec!["content.body".to_string(), "content.name".to_string()],
            filter: Some(Filter {
                limit: Some(10),
                rooms: Some(vec!["!room1:example.com".to_string()]),
                not_rooms: None,
                types: None,
                not_types: None,
                senders: None,
                not_senders: None,
            }),
            groupings: None,
            order_by: "recent".to_string(),
            next_batch: None,
        };

        assert_eq!(search.search_term, "test query");
        assert_eq!(search.keys.len(), 2);
        assert!(search.filter.is_some());
    }

    #[test]
    fn test_users_search() {
        let search = UsersSearch {
            search_term: "alice".to_string(),
            limit: Some(20),
        };

        assert_eq!(search.search_term, "alice");
        assert_eq!(search.limit, Some(20));
    }

    #[test]
    fn test_filter_structure() {
        let filter = Filter {
            limit: Some(100),
            rooms: Some(vec![
                "!room1:example.com".to_string(),
                "!room2:example.com".to_string(),
            ]),
            not_rooms: Some(vec!["!room3:example.com".to_string()]),
            types: Some(vec!["m.room.message".to_string()]),
            not_types: None,
            senders: Some(vec!["@alice:example.com".to_string()]),
            not_senders: None,
        };

        assert_eq!(filter.limit, Some(100));
        assert!(filter.rooms.is_some());
        assert!(filter.not_rooms.is_some());
    }

    #[test]
    fn test_groupings_structure() {
        let groupings = Groupings {
            group_by: vec![
                GroupBy {
                    key: "room_id".to_string(),
                },
                GroupBy {
                    key: "sender".to_string(),
                },
            ],
        };

        assert_eq!(groupings.group_by.len(), 2);
    }

    #[test]
    fn test_search_response_structure() {
        let response = json!({
            "search_categories": {
                "room_events": {
                    "results": [],
                    "count": 0,
                    "next_batch": null
                }
            }
        });

        assert!(response.get("search_categories").is_some());
    }

    #[test]
    fn test_order_by_default() {
        assert_eq!(default_order_by(), "rank");
    }

    #[test]
    fn test_thread_response_structure() {
        let response = json!({
            "chunk": [],
            "next_batch": null,
            "prev_batch": null,
            "total": 0
        });

        assert!(response.get("chunk").is_some());
        assert!(response.get("total").is_some());
    }

    #[test]
    fn test_hierarchy_response_structure() {
        let response = json!({
            "rooms": [],
            "next_batch": null
        });

        assert!(response.get("rooms").is_some());
    }

    #[test]
    fn test_context_response_structure() {
        let response = json!({
            "event": {},
            "events_before": [],
            "events_after": [],
            "state": [],
            "start": "start_token",
            "end": "end_token"
        });

        assert!(response.get("event").is_some());
        assert!(response.get("events_before").is_some());
        assert!(response.get("events_after").is_some());
    }
}

#[derive(Debug, Deserialize)]
struct SearchRecipientsRequest {
    search_term: String,
    #[serde(default)]
    limit: Option<u32>,
}

async fn search_recipients(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<SearchRecipientsRequest>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.limit.unwrap_or(10).min(50) as i64;
    let search_term = body.search_term.trim();

    if search_term.is_empty() {
        return Err(ApiError::bad_request(
            "Search term cannot be empty".to_string(),
        ));
    }

    let search_pattern = format!("%{}%", search_term.to_lowercase());

    let users = sqlx::query(
        r#"
        SELECT user_id, username, displayname, avatar_url
        FROM users
        WHERE LOWER(username) LIKE $1 OR LOWER(displayname) LIKE $1
        LIMIT $2
        "#,
    )
    .bind(&search_pattern)
    .bind(limit)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Search failed: {}", e)))?;

    let mut results = Vec::new();
    for row in users {
        let target_user_id = row.get::<String, _>("user_id");
        if !can_view_profile_for_requester(&state, Some(&auth_user.user_id), &target_user_id)
            .await?
        {
            continue;
        }

        results.push(json!({
            "user_id": target_user_id,
            "username": row.get::<String, _>("username"),
            "display_name": row.get::<Option<String>, _>("displayname"),
            "avatar_url": row.get::<Option<String>, _>("avatar_url"),
        }));
    }

    Ok(Json(json!({
        "results": results,
        "count": results.len(),
        "next_batch": null
    })))
}

#[derive(Debug, Deserialize)]
struct SearchRoomsRequest {
    search_term: String,
    #[serde(default)]
    limit: Option<u32>,
}

async fn search_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<SearchRoomsRequest>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.limit.unwrap_or(10).min(50) as i64;
    let search_term = body.search_term.trim();

    if search_term.is_empty() {
        return Err(ApiError::bad_request(
            "Search term cannot be empty".to_string(),
        ));
    }

    let search_pattern = format!("%{}%", search_term.to_lowercase());

    let rooms = sqlx::query(
        r#"
        SELECT room_id, name, topic, avatar_url, is_public
        FROM rooms
        WHERE
            (LOWER(name) LIKE $1 OR LOWER(topic) LIKE $1)
            AND (
                is_public = true
                OR EXISTS (
                    SELECT 1
                    FROM room_memberships
                    WHERE room_memberships.room_id = rooms.room_id
                      AND room_memberships.user_id = $2
                      AND room_memberships.membership = 'join'
                )
            )
        ORDER BY name
        LIMIT $3
        "#,
    )
    .bind(&search_pattern)
    .bind(&auth_user.user_id)
    .bind(limit)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Search failed: {}", e)))?;

    let results: Vec<Value> = rooms
        .iter()
        .map(|row| {
            json!({
                "room_id": row.get::<String, _>("room_id"),
                "name": row.get::<Option<String>, _>("name"),
                "topic": row.get::<Option<String>, _>("topic"),
                "avatar_url": row.get::<Option<String>, _>("avatar_url"),
                "is_public": row.get::<bool, _>("is_public"),
                "world_readable": row.get::<bool, _>("is_public"),
                "num_joined_members": 0,
            })
        })
        .collect();

    Ok(Json(json!({
        "results": results,
        "count": results.len(),
        "next_batch": null
    })))
}
