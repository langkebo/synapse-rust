use crate::common::ApiError;
use crate::web::routes::account_compat::can_view_profile_for_requester_batch;
use crate::web::routes::context::RoomContext;
use crate::web::routes::AuthenticatedUser;
use axum::extract::{Json, Query, State};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use synapse_services::search_service::RoomEventsSearchFilter;

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

fn default_order_by() -> String {
    "rank".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SearchRequest {
    pub search_categories: SearchCategories,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SearchCategories {
    #[serde(default)]
    pub room_events: Option<RoomEventsSearch>,
    #[serde(default)]
    pub users: Option<UsersSearch>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RoomEventsSearch {
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
pub(crate) struct UsersSearch {
    pub search_term: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Filter {
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
pub(crate) struct Groupings {
    #[serde(default)]
    pub group_by: Vec<GroupBy>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct GroupBy {
    #[serde(rename = "key")]
    pub key: String,
}

/// Parsed search query parameters extracted from request body and query string.
#[allow(dead_code)]
pub(crate) struct ParsedSearchQuery {
    pub search_term: String,
    pub limit: usize,
    pub order_by: String,
    pub include_profile: bool,
}

/// Extract and validate search parameters from the query string and request body.
#[allow(dead_code)]
pub(crate) fn parse_search_params(
    params: &HashMap<String, String>,
    body: &SearchRequest,
) -> Result<ParsedSearchQuery, ApiError> {
    let room_events = body
        .search_categories
        .room_events
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("Missing room_events search category"))?;

    let search_term = room_events.search_term.trim().to_string();
    if search_term.is_empty() {
        return Err(ApiError::bad_request("Search term cannot be empty"));
    }
    if search_term.len() > MAX_SEARCH_TERM_LENGTH {
        return Err(ApiError::bad_request(format!("Search term too long (max {MAX_SEARCH_TERM_LENGTH} characters)")));
    }

    let limit = room_events.filter.as_ref().and_then(|f| f.limit).unwrap_or(DEFAULT_SEARCH_LIMIT).min(MAX_SEARCH_LIMIT)
        as usize;

    let order_by = room_events.order_by.clone();

    let include_profile = params.get("include_profile").is_some_and(|v| v == "true");

    Ok(ParsedSearchQuery { search_term, limit, order_by, include_profile })
}

pub(crate) fn validate_search_request(body: &SearchRequest) -> Result<(), ApiError> {
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

pub(crate) async fn search(
    State(ctx): State<RoomContext>,
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
        let search_future = search_room_events(&ctx, &auth_user.user_id, room_events, room_events_next_batch);

        let room_results = timeout(Duration::from_secs(SEARCH_TIMEOUT_SECS), search_future)
            .await
            .map_err(|e| ApiError::internal_with_log("Search request timed out", &e))??;

        results["search_categories"]["room_events"] = room_results;
    }

    if let Some(users_search) = &body.search_categories.users {
        let search_future = search_users(&ctx, &auth_user.user_id, users_search);

        let user_results = timeout(Duration::from_secs(SEARCH_TIMEOUT_SECS), search_future)
            .await
            .map_err(|e| ApiError::internal_with_log("User search request timed out", &e))??;

        results["search_categories"]["users"] = user_results;
    }

    Ok(Json(results))
}

async fn search_room_events(
    ctx: &RoomContext,
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
    if let Ok(Some(cached)) = ctx.cache.get::<Value>(&cache_key).await {
        return Ok(cached);
    }

    let filter = search.filter.as_ref().map(|filter| RoomEventsSearchFilter {
        rooms: filter.rooms.clone(),
        not_rooms: filter.not_rooms.clone(),
        types: filter.types.clone(),
        senders: filter.senders.clone(),
    });

    let page =
        ctx.search_service.search_room_events(user_id, &search.search_term, filter.as_ref(), limit, next_batch).await?;

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

    if let Err(e) = ctx.cache.set(&cache_key, &result, SEARCH_CACHE_TTL_SECS).await {
        ::tracing::warn!("Failed to cache search room results: {e}");
    }

    Ok(result)
}

async fn search_users(ctx: &RoomContext, user_id: &str, search: &UsersSearch) -> Result<Value, ApiError> {
    let limit = search.limit.unwrap_or(DEFAULT_SEARCH_LIMIT) as i64;

    let cache_key = format!("search:users:{}:{}:{}", user_id, search.search_term.to_lowercase(), limit);
    if let Ok(Some(cached)) = ctx.cache.get::<Value>(&cache_key).await {
        return Ok(cached);
    }

    let rate_limit_key = format!("ratelimit:search-users:{user_id}");
    let decision = ctx.cache.rate_limit_token_bucket_take(&rate_limit_key, 2, 20).await?;
    if !decision.allowed {
        return Err(ApiError::rate_limited("Too many user search requests"));
    }

    let rows = ctx.account_identity_service.search_directory_users(&search.search_term, limit, false).await?;

    let mut results = Vec::new();
    let target_user_ids: Vec<String> = rows.iter().map(|r| r.user_id.clone()).collect();
    let visibility =
        can_view_profile_for_requester_batch(&ctx.account_identity_service, Some(user_id), &target_user_ids).await?;

    for row in rows {
        let target_user_id = row.user_id;
        let presence_str = row.presence.unwrap_or_else(|| "offline".to_string());
        let presence_state =
            crate::common::PresenceState::from_str_opt(&presence_str).unwrap_or(crate::common::PresenceState::Offline);

        if !visibility.get(&target_user_id).copied().unwrap_or(false) {
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

    if let Err(e) = ctx.cache.set(&cache_key, &result, SEARCH_CACHE_TTL_SECS).await {
        ::tracing::warn!("Failed to cache search user results: {e}");
    }

    Ok(result)
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchRecipientsRequest {
    pub search_term: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

pub(crate) async fn search_recipients(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<SearchRecipientsRequest>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.limit.unwrap_or(10).min(50) as i64;
    let search_term = body.search_term.trim();

    if search_term.is_empty() {
        return Err(ApiError::bad_request("Search term cannot be empty".to_string()));
    }

    let rate_limit_key = format!("ratelimit:search-recipients:{}", auth_user.user_id);
    let decision = ctx.cache.rate_limit_token_bucket_take(&rate_limit_key, 2, 20).await?;
    if !decision.allowed {
        return Err(ApiError::rate_limited("Too many recipient search requests"));
    }

    let users = ctx.account_identity_service.search_directory_users(search_term, limit, false).await?;

    let mut results = Vec::new();
    let target_user_ids: Vec<String> = users.iter().map(|r| r.user_id.clone()).collect();
    let visibility =
        can_view_profile_for_requester_batch(&ctx.account_identity_service, Some(&auth_user.user_id), &target_user_ids)
            .await?;

    for row in users {
        let target_user_id = row.user_id;
        let presence_str = row.presence.unwrap_or_else(|| "offline".to_string());
        let presence_state =
            crate::common::PresenceState::from_str_opt(&presence_str).unwrap_or(crate::common::PresenceState::Offline);
        let online = presence_state == crate::common::PresenceState::Online;
        if !visibility.get(&target_user_id).copied().unwrap_or(false) {
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
pub(crate) struct SearchRoomsRequest {
    pub search_term: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

pub(crate) async fn search_rooms(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<SearchRoomsRequest>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.limit.unwrap_or(10).min(50) as i64;
    let search_term = body.search_term.trim();

    if search_term.is_empty() {
        return Err(ApiError::bad_request("Search term cannot be empty".to_string()));
    }

    let rooms = ctx.search_service.search_rooms_for_user(&auth_user.user_id, search_term, limit).await?;

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
    fn test_parse_search_params_extracts_valid_params() {
        let body = SearchRequest {
            search_categories: SearchCategories {
                room_events: Some(RoomEventsSearch {
                    search_term: "test".to_string(),
                    keys: vec![],
                    filter: Some(Filter {
                        limit: Some(25),
                        rooms: None,
                        not_rooms: None,
                        types: None,
                        not_types: None,
                        senders: None,
                        not_senders: None,
                    }),
                    groupings: None,
                    order_by: "recent".to_string(),
                    next_batch: None,
                }),
                users: None,
            },
        };

        let params: HashMap<String, String> = [("include_profile".to_string(), "true".to_string())].into();

        let result = parse_search_params(&params, &body).unwrap();
        assert_eq!(result.search_term, "test");
        assert_eq!(result.limit, 25);
        assert_eq!(result.order_by, "recent");
        assert!(result.include_profile);
    }

    #[test]
    fn test_parse_search_params_rejects_empty_term() {
        let body = SearchRequest {
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

        let params = HashMap::new();
        let result = parse_search_params(&params, &body);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_search_params_missing_room_events() {
        let body = SearchRequest { search_categories: SearchCategories { room_events: None, users: None } };

        let params = HashMap::new();
        let result = parse_search_params(&params, &body);
        assert!(result.is_err());
    }
}
