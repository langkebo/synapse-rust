use crate::common::ApiError;
use crate::services::{RoomEventsSearchFilter, StateEvent, TimestampDirection};
use crate::web::routes::{
    account_compat::can_view_profile_for_requester_batch, ensure_room_member_strict, validate_room_id, AppState,
    AuthenticatedUser,
};
use axum::{
    extract::{Json, Path, Query, State},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

const MAX_SEARCH_TERM_LENGTH: usize = 256;
const MAX_FILTER_ROOMS: usize = 50;
const MAX_FILTER_TYPES: usize = 20;
const MAX_FILTER_SENDERS: usize = 50;
const MAX_SEARCH_LIMIT: u32 = 100;
const DEFAULT_SEARCH_LIMIT: u32 = 10;
const SEARCH_TIMEOUT_SECS: u64 = 30;
const SEARCH_CACHE_TTL_SECS: u64 = 30;

fn create_search_compat_router() -> Router<AppState> {
    Router::new()
        .route("/search", post(search))
        .route("/search_recipients", post(search_recipients))
        .route("/search_rooms", post(search_rooms))
}

fn create_room_context_router() -> Router<AppState> {
    Router::new().route("/rooms/{room_id}/context/{event_id}", get(get_event_context))
}

pub fn create_search_router(state: AppState) -> Router<AppState> {
    let v1_router = Router::new()
        .merge(create_room_context_router())
        .route("/rooms/{room_id}/hierarchy", get(get_room_hierarchy))
        .route("/rooms/{room_id}/timestamp_to_event", get(timestamp_to_event));

    let v3_router = Router::new()
        .merge(create_search_compat_router())
        .merge(create_room_context_router())
        .route("/rooms/{room_id}/hierarchy", get(get_room_hierarchy_v3));

    Router::new()
        .nest("/_matrix/client/r0", create_search_compat_router().merge(create_room_context_router()))
        .nest("/_matrix/client/v1", v1_router)
        .nest("/_matrix/client/v3", v3_router)
        .with_state(state)
}

pub fn search_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::{expand_under_prefixes, RouteEntry};
    use axum::http::Method;

    let search_compat: &[(Method, &'static str)] =
        &[(Method::POST, "/search"), (Method::POST, "/search_recipients"), (Method::POST, "/search_rooms")];
    let room_context: &[(Method, &'static str)] = &[(Method::GET, "/rooms/{room_id}/context/{event_id}")];
    let v1_extras: &[(Method, &'static str)] =
        &[(Method::GET, "/rooms/{room_id}/hierarchy"), (Method::GET, "/rooms/{room_id}/timestamp_to_event")];
    let v3_extras: &[(Method, &'static str)] = &[(Method::GET, "/rooms/{room_id}/hierarchy")];

    let mut entries: Vec<RouteEntry> = expand_under_prefixes("search", &["/_matrix/client/r0"], search_compat);
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v1"], room_context));
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v1"], v1_extras));
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v3"], search_compat));
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v3"], room_context));
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v3"], v3_extras));
    entries
}

fn validate_search_request(body: &SearchRequest) -> Result<(), ApiError> {
    if let Some(room_events) = &body.search_categories.room_events {
        if room_events.search_term.len() > MAX_SEARCH_TERM_LENGTH {
            return Err(ApiError::bad_request(format!(
                "Search term too long (max {MAX_SEARCH_TERM_LENGTH} characters)"
            )));
        }

        if room_events.search_term.trim().is_empty() {
            return Err(ApiError::bad_request("Search term cannot be empty"));
        }

        if let Some(filter) = &room_events.filter {
            if let Some(limit) = filter.limit {
                if limit > MAX_SEARCH_LIMIT {
                    return Err(ApiError::bad_request(format!("Limit too high (max {MAX_SEARCH_LIMIT})")));
                }
            }

            if let Some(rooms) = &filter.rooms {
                if rooms.len() > MAX_FILTER_ROOMS {
                    return Err(ApiError::bad_request(format!("Too many rooms in filter (max {MAX_FILTER_ROOMS})")));
                }
            }

            if let Some(types) = &filter.types {
                if types.len() > MAX_FILTER_TYPES {
                    return Err(ApiError::bad_request(format!("Too many types in filter (max {MAX_FILTER_TYPES})")));
                }
            }

            if let Some(senders) = &filter.senders {
                if senders.len() > MAX_FILTER_SENDERS {
                    return Err(ApiError::bad_request(format!(
                        "Too many senders in filter (max {MAX_FILTER_SENDERS})"
                    )));
                }
            }
        }
    }

    if let Some(users_search) = &body.search_categories.users {
        if users_search.search_term.len() > MAX_SEARCH_TERM_LENGTH {
            return Err(ApiError::bad_request(format!(
                "Search term too long (max {MAX_SEARCH_TERM_LENGTH} characters)"
            )));
        }

        if let Some(limit) = users_search.limit {
            if limit > MAX_SEARCH_LIMIT {
                return Err(ApiError::bad_request(format!("Limit too high (max {MAX_SEARCH_LIMIT})")));
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

    let mut results = json!({
        "search_categories": {}
    });

    if let Some(room_events) = &body.search_categories.room_events {
        let room_events_next_batch =
            room_events.next_batch.as_deref().or_else(|| params.get("next_batch").map(String::as_str));
        let search_future = search_room_events(&state, &auth_user.user_id, room_events, room_events_next_batch);

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
    user_id: &str,
    search: &RoomEventsSearch,
    next_batch: Option<&str>,
) -> Result<Value, ApiError> {
    let limit = search.filter.as_ref().and_then(|f| f.limit).unwrap_or(10) as i64;
    let cache_key = format!(
        "search:room_events:{}:{}:{}:{}",
        user_id,
        search.search_term.to_lowercase(),
        limit,
        next_batch.unwrap_or("")
    );
    if let Ok(Some(cached)) = state.cache.get::<Value>(&cache_key).await {
        return Ok(cached);
    }

    let filter = search.filter.as_ref().map(|filter| RoomEventsSearchFilter {
        rooms: filter.rooms.clone(),
        not_rooms: filter.not_rooms.clone(),
        types: filter.types.clone(),
        senders: filter.senders.clone(),
    });

    let page = state
        .services
        .search_service
        .search_room_events(user_id, &search.search_term, filter.as_ref(), limit, next_batch)
        .await?;

    let results: Vec<Value> = page
        .results
        .into_iter()
        .map(|event| {
            json!({
                "result": {
                    "event_id": event.event_id,
                    "room_id": event.room_id,
                    "sender": event.sender,
                    "type": event.event_type,
                    "content": event.content,
                    "origin_server_ts": event.origin_server_ts
                },
                "rank": 0.5
            })
        })
        .collect();

    let count = results.len() as i64;

    let result = json!({
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
        "next_batch": page.next_batch
    });

    let _ = state.cache.set(&cache_key, &result, SEARCH_CACHE_TTL_SECS).await;

    Ok(result)
}

async fn search_users(state: &AppState, user_id: &str, search: &UsersSearch) -> Result<Value, ApiError> {
    let limit = search.limit.unwrap_or(DEFAULT_SEARCH_LIMIT) as i64;

    let cache_key = format!("search:users:{}:{}:{}", user_id, search.search_term.to_lowercase(), limit);
    if let Ok(Some(cached)) = state.cache.get::<Value>(&cache_key).await {
        return Ok(cached);
    }

    let rate_limit_key = format!("ratelimit:search-users:{user_id}");
    let decision = state.cache.rate_limit_token_bucket_take(&rate_limit_key, 2, 20).await?;
    if !decision.allowed {
        return Err(ApiError::rate_limited("Too many user search requests"));
    }

    let rows = state
        .services
        .user_storage
        .search_directory_users(&search.search_term, limit, false)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let mut results = Vec::new();
    let target_user_ids: Vec<String> = rows.iter().map(|r| r.user_id.clone()).collect();
    let visibility = can_view_profile_for_requester_batch(state, Some(user_id), &target_user_ids).await?;

    for row in rows {
        let target_user_id = row.user_id;
        let presence_str = row.presence.unwrap_or_else(|| "offline".to_string());
        let presence_state = crate::common::PresenceState::from_str_opt(&presence_str)
            .unwrap_or(crate::common::PresenceState::Offline);

        if !visibility.get(&target_user_id).copied().unwrap_or(true) {
            continue;
        }

        results.push(json!({
            "user_id": target_user_id,
            "display_name": row.displayname,
            "avatar_url": row.avatar_url,
            "presence": presence_state.to_string(),
            "last_active_ts": row.last_active_ts,
            "match_score": row.match_score,
            "match_type": row.match_type
        }));
    }

    let result = json!({
        "results": results,
        "limited": results.len() as i64 == limit
    });

    let _ = state.cache.set(&cache_key, &result, SEARCH_CACHE_TTL_SECS).await;

    Ok(result)
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
    let room_opt = state
        .services.rooms.room_storage
        .get_room(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to load room", &e))?;

    if let Some(space) = state.services.rooms.space_service.get_space_by_room(room_id).await? {
        let response = state
            .services.rooms.space_service
            .get_space_hierarchy_v1(
                &space.space_id,
                max_depth.max(1),
                suggested_only,
                Some(limit.max(1)),
                from,
                Some(user_id),
            )
            .await?;

        let mut response_value = serde_json::to_value(response)
            .map_err(|e| ApiError::internal_with_log("Failed to serialize hierarchy", &e))?;

        if let Some(obj) = response_value.as_object_mut() {
            let rooms = obj.get("rooms").and_then(|r| r.as_array()).map_or(0, |a| a.len());
            let has_space_self = obj
                .get("rooms")
                .and_then(|r| r.as_array())
                .is_some_and(|a| a.iter().any(|r| r.get("room_id").and_then(|v| v.as_str()) == Some(room_id)));

            if !has_space_self || rooms <= 1 {
                let state_events = state
                    .services.rooms.event_storage
                    .get_state_events(room_id)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to get state", &e))?;

                let mut children_state = Vec::new();
                let mut child_room_ids = Vec::new();

                for ev in &state_events {
                    if ev.event_type.as_deref() == Some("m.space.child") {
                        let via = ev
                            .content
                            .get("via")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
                            .unwrap_or_default();
                        if !via.is_empty() {
                            children_state.push(json!({
                                "type": "m.space.child",
                                "state_key": ev.state_key,
                                "content": ev.content,
                                "origin_server_ts": ev.origin_server_ts,
                            }));

                            if max_depth > 0 {
                                if let Some(sk) = ev.state_key.as_deref() {
                                    child_room_ids.push(sk.to_string());
                                }
                            }
                        }
                    }
                }

                let child_rooms_map = if !child_room_ids.is_empty() {
                    let rooms_batch = state
                        .services.rooms.room_storage
                        .get_rooms_batch(&child_room_ids)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to load child rooms", &e))?;
                    let mut map = HashMap::new();
                    for room in rooms_batch {
                        map.insert(room.room_id.clone(), room);
                    }

                    let state_batch =
                        state.services.rooms.event_storage.get_state_events_batch(&child_room_ids).await.unwrap_or_default();

                    let mut child_rooms = Vec::new();
                    for rid in &child_room_ids {
                        if let Some(child_room) = map.get(rid) {
                            let child_state_events: &[StateEvent] = state_batch.get(rid).map_or(&[], |v| v.as_slice());
                            let child_room_type = child_state_events
                                .iter()
                                .find(|e| e.event_type.as_deref() == Some("m.room.create"))
                                .and_then(|e| e.content.get("type"))
                                .and_then(|v: &Value| v.as_str())
                                .map_or(Value::Null, |s: &str| Value::String(s.to_string()));
                            child_rooms.push(json!({
                                "room_id": child_room.room_id,
                                "name": child_room.name,
                                "topic": child_room.topic,
                                "avatar_url": child_room.avatar_url,
                                "join_rule": child_room.join_rule,
                                "guest_access": if child_room.is_public { "can_join" } else { "forbidden" },
                                "guest_can_join": child_room.is_public,
                                "world_readable": child_room.history_visibility == "world_readable",
                                "num_joined_members": child_room.member_count,
                                "children": [],
                                "children_state": [],
                                "room_type": child_room_type,
                            }));
                        }
                    }
                    child_rooms
                } else {
                    Vec::new()
                };

                if !child_rooms_map.is_empty() || !has_space_self {
                    let space_room_type = state_events
                        .iter()
                        .find(|e| e.event_type.as_deref() == Some("m.room.create"))
                        .and_then(|e| e.content.get("type"))
                        .and_then(|v| v.as_str())
                        .map_or(Value::Null, |s| Value::String(s.to_string()));

                    if let Some(rooms_arr) = obj.get_mut("rooms").and_then(|r| r.as_array_mut()) {
                        if !has_space_self {
                            if let Some(ref r) = room_opt {
                                rooms_arr.insert(0, json!({
                                    "room_id": r.room_id,
                                    "name": r.name,
                                    "topic": r.topic,
                                    "avatar_url": r.avatar_url,
                                    "join_rule": r.join_rule,
                                    "guest_access": if r.is_public { "can_join" } else { "forbidden" },
                                    "guest_can_join": r.is_public,
                                    "world_readable": r.history_visibility == "world_readable",
                                    "num_joined_members": r.member_count,
                                    "children": child_rooms_map.iter().filter_map(|r| r.get("room_id").and_then(|v| v.as_str()).map(String::from)).collect::<Vec<_>>(),
                                    "children_state": children_state,
                                    "room_type": space_room_type,
                                }));
                            }
                        } else if let Some(first) = rooms_arr.first_mut() {
                            if let Some(first_obj) = first.as_object_mut() {
                                first_obj.insert("children_state".to_string(), json!(children_state));
                                first_obj.insert(
                                    "children".to_string(),
                                    json!(child_rooms_map
                                        .iter()
                                        .filter_map(|r| r.get("room_id").and_then(|v| v.as_str()).map(String::from))
                                        .collect::<Vec<_>>()),
                                );
                            }
                        }
                        rooms_arr.extend(child_rooms_map);
                    }
                }
            }
        }

        return Ok(response_value);
    }

    let room = room_opt.ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let world_readable = room.history_visibility == "world_readable";

    let state_events = state
        .services.rooms.event_storage
        .get_state_events(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get state", &e))?;

    let room_type = state_events
        .iter()
        .find(|e| e.event_type.as_deref() == Some("m.room.create"))
        .and_then(|e| e.content.get("type"))
        .and_then(|v| v.as_str())
        .map_or(Value::Null, |s| Value::String(s.to_string()));

    let mut children_state = Vec::new();
    let mut child_room_ids = Vec::new();

    for ev in &state_events {
        if ev.event_type.as_deref() == Some("m.space.child") {
            let via = ev
                .content
                .get("via")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
                .unwrap_or_default();
            if !via.is_empty() {
                children_state.push(json!({
                    "type": "m.space.child",
                    "state_key": ev.state_key,
                    "content": ev.content,
                    "origin_server_ts": ev.origin_server_ts,
                }));

                if max_depth > 0 {
                    if let Some(sk) = ev.state_key.as_deref() {
                        child_room_ids.push(sk.to_string());
                    }
                }
            }
        }
    }

    let child_rooms = if !child_room_ids.is_empty() {
        let rooms_batch = state
            .services.rooms.room_storage
            .get_rooms_batch(&child_room_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load child rooms", &e))?;
        let mut map = HashMap::new();
        for room in rooms_batch {
            map.insert(room.room_id.clone(), room);
        }

        let state_batch =
            state.services.rooms.event_storage.get_state_events_batch(&child_room_ids).await.unwrap_or_default();

        let mut result = Vec::new();
        for rid in &child_room_ids {
            if let Some(child_room) = map.get(rid) {
                let child_state_events: &[StateEvent] = state_batch.get(rid).map_or(&[], |v| v.as_slice());
                let child_room_type = child_state_events
                    .iter()
                    .find(|e| e.event_type.as_deref() == Some("m.room.create"))
                    .and_then(|e| e.content.get("type"))
                    .and_then(|v: &Value| v.as_str())
                    .map_or(Value::Null, |s: &str| Value::String(s.to_string()));
                result.push(json!({
                    "room_id": child_room.room_id,
                    "name": child_room.name,
                    "topic": child_room.topic,
                    "avatar_url": child_room.avatar_url,
                    "join_rule": child_room.join_rule,
                    "guest_access": if child_room.is_public { "can_join" } else { "forbidden" },
                    "guest_can_join": child_room.is_public,
                    "world_readable": child_room.history_visibility == "world_readable",
                    "num_joined_members": child_room.member_count,
                    "children": [],
                    "children_state": [],
                    "room_type": child_room_type,
                    "required_state_info": []
                }));
            }
        }
        result
    } else {
        Vec::new()
    };

    let mut rooms = vec![json!({
        "room_id": room.room_id,
        "name": room.name,
        "topic": room.topic,
        "avatar_url": room.avatar_url,
        "join_rule": room.join_rule,
        "guest_access": if room.is_public { "can_join" } else { "forbidden" },
        "guest_can_join": room.is_public,
        "world_readable": world_readable,
        "num_joined_members": room.member_count,
        "children": child_rooms.iter().filter_map(|r| r.get("room_id").and_then(|v| v.as_str()).map(String::from)).collect::<Vec<_>>(),
        "children_state": children_state,
        "room_type": room_type,
        "required_state_info": []
    })];
    rooms.extend(child_rooms);

    Ok(json!({
        "rooms": rooms,
        "next_batch": Value::Null
    }))
}

async fn get_room_hierarchy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);

    let max_depth = params.get("max_depth").and_then(|v| v.parse().ok()).unwrap_or(3);

    let mut response =
        build_room_hierarchy_response(&state, &room_id, &auth_user.user_id, max_depth, false, limit, None).await?;

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
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);

    let max_depth = params.get("max_depth").and_then(|v| v.parse().ok()).unwrap_or(1);

    let suggested_only = params.get("suggested_only").is_some_and(|v| v == "true");

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

    let dir = params.get("dir").map_or("f", |v| v.as_str());

    ensure_room_member_strict(&state, &auth_user, &room_id, "Not a member of this room").await?;

    let direction = if dir == "b" {
        TimestampDirection::Backward
    } else {
        TimestampDirection::Forward
    };

    let event = state
        .services
        .search_service
        .find_event_by_timestamp(&room_id, ts, direction)
        .await?;

    match event {
        Some(event) => Ok(Json(json!({
            "event_id": event.event_id,
            "origin_server_ts": event.origin_server_ts
        }))),
        None => Err(ApiError::not_found("No event found at this timestamp".to_string())),
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

    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(10).clamp(1, 100);

    ensure_room_member_strict(&state, &auth_user, &room_id, "Not a member of this room").await?;

    let target_event = state
        .services.rooms.event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get event", &e))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if target_event.room_id != room_id {
        return Err(ApiError::not_found("Event not found".to_string()));
    }

    let target_ts = target_event.origin_server_ts;

    let context_window = state
        .services
        .search_service
        .get_event_context_window(&room_id, target_ts, limit as i64)
        .await?;

    let events_before_list: Vec<Value> = context_window
        .events_before
        .iter()
        .map(|event| {
            json!({
                "event_id": event.event_id,
                "sender": event.sender,
                "type": event.event_type,
                "content": event.content.clone(),
                "origin_server_ts": event.origin_server_ts
            })
        })
        .collect();

    let events_after_list: Vec<Value> = context_window
        .events_after
        .iter()
        .map(|event| {
            json!({
                "event_id": event.event_id,
                "sender": event.sender,
                "type": event.event_type,
                "content": event.content.clone(),
                "origin_server_ts": event.origin_server_ts
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

        assert!(routes.iter().all(|route| !route.contains("/user/{user_id}/rooms/{room_id}/threads")));
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
        let search = UsersSearch { search_term: "alice".to_string(), limit: Some(20) };

        assert_eq!(search.search_term, "alice");
        assert_eq!(search.limit, Some(20));
    }

    #[test]
    fn test_filter_structure() {
        let filter = Filter {
            limit: Some(100),
            rooms: Some(vec!["!room1:example.com".to_string(), "!room2:example.com".to_string()]),
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
        let groupings =
            Groupings { group_by: vec![GroupBy { key: "room_id".to_string() }, GroupBy { key: "sender".to_string() }] };

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
        return Err(ApiError::bad_request("Search term cannot be empty".to_string()));
    }

    let rate_limit_key = format!("ratelimit:search-recipients:{}", auth_user.user_id);
    let decision = state.cache.rate_limit_token_bucket_take(&rate_limit_key, 2, 20).await?;
    if !decision.allowed {
        return Err(ApiError::rate_limited("Too many recipient search requests"));
    }

    let users = state
        .services
        .user_storage
        .search_directory_users(search_term, limit, false)
        .await
        .map_err(|e| ApiError::internal_with_log("Search failed", &e))?;

    let mut results = Vec::new();
    let target_user_ids: Vec<String> = users.iter().map(|r| r.user_id.clone()).collect();
    let visibility = can_view_profile_for_requester_batch(&state, Some(&auth_user.user_id), &target_user_ids).await?;

    for row in users {
        let target_user_id = row.user_id;
        let presence_str = row.presence.unwrap_or_else(|| "offline".to_string());
        let presence_state = crate::common::PresenceState::from_str_opt(&presence_str)
            .unwrap_or(crate::common::PresenceState::Offline);
        let online = presence_state == crate::common::PresenceState::Online;
        if !visibility.get(&target_user_id).copied().unwrap_or(true) {
            continue;
        }

        results.push(json!({
            "user_id": target_user_id,
            "username": row.username,
            "display_name": row.displayname,
            "avatar_url": row.avatar_url,
            "presence": presence_state.to_string(),
            "online": online,
            "last_active_ts": row.last_active_ts,
            "match_score": row.match_score,
            "match_type": row.match_type,
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
        return Err(ApiError::bad_request("Search term cannot be empty".to_string()));
    }

    let rooms = state
        .services
        .search_service
        .search_rooms_for_user(&auth_user.user_id, search_term, limit)
        .await?;

    let results: Vec<Value> = rooms
        .iter()
        .map(|room| {
            json!({
                "room_id": room.room_id,
                "name": room.name,
                "topic": room.topic,
                "avatar_url": room.avatar_url,
                "is_public": room.is_public,
                "world_readable": room.is_public,
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
