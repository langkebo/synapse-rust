use crate::common::ApiError;
use crate::web::routes::{
    account_compat::can_view_profile_for_requester_batch, validate_user_id, AppState, AuthenticatedUser,
};
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const DEFAULT_FRIEND_LIST_LIMIT: usize = 20;

pub fn create_friend_router(state: AppState) -> Router<AppState> {
    Router::new()
        // v3 路径
        .route("/_matrix/client/v3/friends", get(get_friends))
        .route("/_matrix/client/v3/friends", post(send_friend_request))
        .route(
            "/_matrix/client/v3/friends/search",
            get(search_friend_directory).post(search_friend_directory),
        )
        .route(
            "/_matrix/client/v3/friends/requests/incoming",
            get(get_incoming_requests),
        )
        .route(
            "/_matrix/client/v3/friends/requests/outgoing",
            get(get_outgoing_requests),
        )
        // v1 和 r0 路径 - 主路由
        .route("/_matrix/client/v1/friends", get(get_friends))
        .route("/_matrix/client/v1/friends", post(send_friend_request))
        .route(
            "/_matrix/client/v1/friends/search",
            get(search_friend_directory).post(search_friend_directory),
        )
        .route("/_matrix/client/r0/friendships", get(get_friends))
        .route("/_matrix/client/r0/friendships", post(send_friend_request))
        .route(
            "/_matrix/client/r0/friends/search",
            get(search_friend_directory),
        )
        // 好友请求
        .route(
            "/_matrix/client/v1/friends/request",
            post(send_friend_request),
        )
        .route(
            "/_matrix/client/v1/friends/request/received",
            get(get_received_requests),
        )
        .route(
            "/_matrix/client/v1/friends/request/{user_id}/accept",
            post(accept_friend_request),
        )
        .route(
            "/_matrix/client/v1/friends/request/{user_id}/reject",
            post(reject_friend_request),
        )
        .route(
            "/_matrix/client/v1/friends/request/{user_id}/cancel",
            post(cancel_friend_request),
        )
        // r0 兼容路由
        .route(
            "/_matrix/client/r0/friends/request",
            post(send_friend_request),
        )
        .route(
            "/_matrix/client/r0/friends/request/received",
            get(get_received_requests),
        )
        .route(
            "/_matrix/client/r0/friends/request/{user_id}/accept",
            post(accept_friend_request),
        )
        .route(
            "/_matrix/client/r0/friends/request/{user_id}/reject",
            post(reject_friend_request),
        )
        .route(
            "/_matrix/client/r0/friends/request/{user_id}/cancel",
            post(cancel_friend_request),
        )
        .route(
            "/_matrix/client/v1/friends/requests/incoming",
            get(get_incoming_requests),
        )
        .route(
            "/_matrix/client/v1/friends/requests/outgoing",
            get(get_outgoing_requests),
        )
        .route(
            "/_matrix/client/r0/friends/requests/incoming",
            get(get_incoming_requests),
        )
        .route(
            "/_matrix/client/r0/friends/requests/outgoing",
            get(get_outgoing_requests),
        )
        .route(
            "/_matrix/client/v1/friends/check/{user_id}",
            get(check_friendship),
        )
        .route(
            "/_matrix/client/v3/friends/check/{user_id}",
            get(check_friendship),
        )
        .route(
            "/_matrix/client/r0/friends/check/{user_id}",
            get(check_friendship),
        )
        .route(
            "/_matrix/client/v1/friends/suggestions",
            get(get_friend_suggestions),
        )
        .route(
            "/_matrix/client/r0/friends/suggestions",
            get(get_friend_suggestions),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}",
            delete(remove_friend),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}",
            delete(remove_friend),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/note",
            put(update_friend_note),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/note",
            put(update_friend_note),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/status",
            get(get_friend_status),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/status",
            put(update_friend_status),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/status",
            get(get_friend_status),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/status",
            put(update_friend_status),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/info",
            get(get_friend_info),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/info",
            get(get_friend_info),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/displayname",
            put(update_friend_displayname),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/displayname",
            put(update_friend_displayname),
        )
        // 好友分组
        .route("/_matrix/client/v1/friends/groups", get(get_friend_groups))
        .route(
            "/_matrix/client/v1/friends/groups",
            post(create_friend_group),
        )
        .route("/_matrix/client/r0/friends/groups", get(get_friend_groups))
        .route(
            "/_matrix/client/r0/friends/groups",
            post(create_friend_group),
        )
        .route(
            "/_matrix/client/v1/friends/groups/{group_id}",
            delete(delete_friend_group),
        )
        .route(
            "/_matrix/client/r0/friends/groups/{group_id}",
            delete(delete_friend_group),
        )
        .route(
            "/_matrix/client/v1/friends/groups/{group_id}/name",
            put(rename_friend_group),
        )
        .route(
            "/_matrix/client/r0/friends/groups/{group_id}/name",
            put(rename_friend_group),
        )
        .route(
            "/_matrix/client/v1/friends/groups/{group_id}/add/{user_id}",
            post(add_friend_to_group),
        )
        .route(
            "/_matrix/client/r0/friends/groups/{group_id}/add/{user_id}",
            post(add_friend_to_group),
        )
        .route(
            "/_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}",
            delete(remove_friend_from_group),
        )
        .route(
            "/_matrix/client/r0/friends/groups/{group_id}/remove/{user_id}",
            delete(remove_friend_from_group),
        )
        .route(
            "/_matrix/client/v1/friends/groups/{group_id}/friends",
            get(get_friends_in_group),
        )
        .route(
            "/_matrix/client/r0/friends/groups/{group_id}/friends",
            get(get_friends_in_group),
        )
        .route(
            "/_matrix/client/v1/friends/{user_id}/groups",
            get(get_groups_for_user),
        )
        .route(
            "/_matrix/client/r0/friends/{user_id}/groups",
            get(get_groups_for_user),
        )
        .route(
            "/_matrix/client/v1/friends/dm/{user_id}",
            get(get_friend_dm).post(create_friend_dm),
        )
        .route(
            "/_matrix/client/r0/friends/dm/{user_id}",
            get(get_friend_dm).post(create_friend_dm),
        )
        .with_state(state)
}

pub fn friend_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    [
        (Method::GET, "/_matrix/client/v3/friends"),
        (Method::POST, "/_matrix/client/v3/friends"),
        (Method::GET, "/_matrix/client/v3/friends/search"),
        (Method::POST, "/_matrix/client/v3/friends/search"),
        (Method::GET, "/_matrix/client/v3/friends/requests/incoming"),
        (Method::GET, "/_matrix/client/v3/friends/requests/outgoing"),
        (Method::GET, "/_matrix/client/v1/friends"),
        (Method::POST, "/_matrix/client/v1/friends"),
        (Method::GET, "/_matrix/client/v1/friends/search"),
        (Method::POST, "/_matrix/client/v1/friends/search"),
        (Method::GET, "/_matrix/client/r0/friendships"),
        (Method::POST, "/_matrix/client/r0/friendships"),
        (Method::GET, "/_matrix/client/r0/friends/search"),
        (Method::POST, "/_matrix/client/v1/friends/request"),
        (Method::GET, "/_matrix/client/v1/friends/request/received"),
        (Method::POST, "/_matrix/client/v1/friends/request/{user_id}/accept"),
        (Method::POST, "/_matrix/client/v1/friends/request/{user_id}/reject"),
        (Method::POST, "/_matrix/client/v1/friends/request/{user_id}/cancel"),
        (Method::POST, "/_matrix/client/r0/friends/request"),
        (Method::GET, "/_matrix/client/r0/friends/request/received"),
        (Method::POST, "/_matrix/client/r0/friends/request/{user_id}/accept"),
        (Method::POST, "/_matrix/client/r0/friends/request/{user_id}/reject"),
        (Method::POST, "/_matrix/client/r0/friends/request/{user_id}/cancel"),
        (Method::GET, "/_matrix/client/v1/friends/requests/incoming"),
        (Method::GET, "/_matrix/client/v1/friends/requests/outgoing"),
        (Method::GET, "/_matrix/client/r0/friends/requests/incoming"),
        (Method::GET, "/_matrix/client/r0/friends/requests/outgoing"),
        (Method::GET, "/_matrix/client/v1/friends/check/{user_id}"),
        (Method::GET, "/_matrix/client/r0/friends/check/{user_id}"),
        (Method::GET, "/_matrix/client/v1/friends/suggestions"),
        (Method::GET, "/_matrix/client/r0/friends/suggestions"),
        (Method::DELETE, "/_matrix/client/v1/friends/{user_id}"),
        (Method::DELETE, "/_matrix/client/r0/friends/{user_id}"),
        (Method::PUT, "/_matrix/client/v1/friends/{user_id}/note"),
        (Method::PUT, "/_matrix/client/r0/friends/{user_id}/note"),
        (Method::GET, "/_matrix/client/v1/friends/{user_id}/status"),
        (Method::PUT, "/_matrix/client/v1/friends/{user_id}/status"),
        (Method::GET, "/_matrix/client/r0/friends/{user_id}/status"),
        (Method::PUT, "/_matrix/client/r0/friends/{user_id}/status"),
        (Method::GET, "/_matrix/client/v1/friends/{user_id}/info"),
        (Method::GET, "/_matrix/client/r0/friends/{user_id}/info"),
        (Method::PUT, "/_matrix/client/v1/friends/{user_id}/displayname"),
        (Method::PUT, "/_matrix/client/r0/friends/{user_id}/displayname"),
        (Method::GET, "/_matrix/client/v1/friends/groups"),
        (Method::POST, "/_matrix/client/v1/friends/groups"),
        (Method::GET, "/_matrix/client/r0/friends/groups"),
        (Method::POST, "/_matrix/client/r0/friends/groups"),
        (Method::DELETE, "/_matrix/client/v1/friends/groups/{group_id}"),
        (Method::DELETE, "/_matrix/client/r0/friends/groups/{group_id}"),
        (Method::PUT, "/_matrix/client/v1/friends/groups/{group_id}/name"),
        (Method::PUT, "/_matrix/client/r0/friends/groups/{group_id}/name"),
        (Method::POST, "/_matrix/client/v1/friends/groups/{group_id}/add/{user_id}"),
        (Method::POST, "/_matrix/client/r0/friends/groups/{group_id}/add/{user_id}"),
        (Method::DELETE, "/_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}"),
        (Method::DELETE, "/_matrix/client/r0/friends/groups/{group_id}/remove/{user_id}"),
        (Method::GET, "/_matrix/client/v1/friends/groups/{group_id}/friends"),
        (Method::GET, "/_matrix/client/r0/friends/groups/{group_id}/friends"),
        (Method::GET, "/_matrix/client/v1/friends/{user_id}/groups"),
        (Method::GET, "/_matrix/client/r0/friends/{user_id}/groups"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "friend_room"))
    .collect()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddFriendRequest {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateNoteRequest {
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateDisplaynameRequest {
    #[serde(rename = "displayname")]
    pub display_name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct FriendListQueryParams {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub sort_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FriendSearchQuery {
    #[serde(default, alias = "query")]
    pub q: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

fn resolve_friend_search_term(query: &FriendSearchQuery, body: Option<&Value>) -> Option<String> {
    body.and_then(|payload| {
        payload
            .get("q")
            .or_else(|| payload.get("query"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    })
    .or_else(|| query.q.as_deref().map(str::trim).map(ToOwned::to_owned))
    .filter(|s| !s.is_empty())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FriendRequest {
    pub user_id: String,
    #[serde(rename = "displayname")]
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub message: Option<String>,
    pub timestamp: i64,
    pub status: FriendRequestStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FriendRequestStatus {
    Pending,
    Accepted,
    Rejected,
    Cancelled,
}

async fn get_friends(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(params): Query<FriendListQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let page = state
        .services
        .friend_room_service
        .get_friends_page(
            &auth_user.user_id,
            crate::services::friend_room_service::FriendListRequest {
                limit: params.limit.unwrap_or(50).clamp(1, 200),
                offset: params.offset.unwrap_or(0).clamp(0, 10000),
                sort_by: params.sort_by.unwrap_or_else(|| "alphabet".to_string()),
            },
        )
        .await?;
    let items = serde_json::to_value(&page.items)
        .map_err(|e| ApiError::internal_with_log("Failed to serialize friend list", &e))?;

    Ok(Json(json!({
        "friends": items,
        "items": items,
        "total": page.total,
        "limit": page.limit,
        "offset": page.offset,
        "next_offset": page.next_offset,
        "room_id": page.room_id,
        "version": page.version,
        "cached": page.cached,
        "generated_ts": page.generated_ts
    })))
}

async fn search_friend_directory(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(query): Query<FriendSearchQuery>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let body_value = body.as_ref().map(|Json(b)| b);
    let search_term = resolve_friend_search_term(&query, body_value);
    let Some(search_term) = search_term else {
        return Err(ApiError::bad_request("Search term cannot be empty"));
    };

    let exact_only = body
        .as_ref()
        .and_then(|Json(b)| b.get("mode").and_then(|v| v.as_str()))
        .map_or_else(|| matches!(query.mode.as_deref(), Some("exact")), |m: &str| m == "exact");
    let search_limit = body
        .as_ref()
        .and_then(|Json(b)| b.get("limit").and_then(|v| v.as_i64()))
        .unwrap_or_else(|| query.limit.unwrap_or(DEFAULT_FRIEND_LIST_LIMIT) as i64) as usize;
    let rate_limit_key = format!("ratelimit:friend-search:{}", auth_user.user_id);
    let decision = state.cache.rate_limit_token_bucket_take(&rate_limit_key, 2, 20).await?;
    if !decision.allowed {
        return Err(ApiError::rate_limited("Too many friend search requests"));
    }

    let mut results = state
        .services
        .user_storage
        .search_directory_users(&search_term, search_limit as i64, exact_only)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to search users", &e))?;

    let target_user_ids: Vec<String> =
        results.iter().filter(|r| r.user_id != auth_user.user_id).map(|r| r.user_id.clone()).collect();
    let visibility = can_view_profile_for_requester_batch(&state, Some(&auth_user.user_id), &target_user_ids).await?;

    let mut visible = Vec::new();
    for result in results.drain(..) {
        if result.user_id == auth_user.user_id {
            continue;
        }
        if !visibility.get(&result.user_id).copied().unwrap_or(true) {
            continue;
        }
        let presence = result.presence.unwrap_or_else(|| "offline".to_string());
        let online = presence == "online";

        visible.push(json!({
            "user_id": result.user_id,
            "username": result.username,
            "displayname": result.displayname,
            "avatar_url": result.avatar_url,
            "presence": presence,
            "online": online,
            "last_active_ts": result.last_active_ts,
            "last_seen_ts": result.last_active_ts,
            "created_ts": result.created_ts,
            "match_score": result.match_score,
            "match_type": result.match_type
        }));
    }
    let count = visible.len();
    let limit = search_limit;

    Ok(Json(json!({
        "results": visible,
        "count": count,
        "mode": if exact_only { "exact" } else { "fuzzy" },
        "limited": count == limit,
        "retry_after_seconds": if decision.allowed { 0 } else { decision.retry_after_seconds }
    })))
}

async fn send_friend_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<AddFriendRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&body.user_id)?;

    if body.user_id == auth_user.user_id {
        return Err(ApiError::bad_request("Cannot send friend request to yourself".to_string()));
    }

    let request_id = state
        .services
        .friend_room_service
        .send_friend_request(&auth_user.user_id, &body.user_id, body.message.as_deref())
        .await?;

    Ok(Json(json!({
        "request_id": request_id,
        "status": "pending"
    })))
}

async fn accept_friend_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(requester_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&requester_id)?;

    let room_id = state.services.friend_room_service.accept_friend_request(&auth_user.user_id, &requester_id).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "status": "accepted"
    })))
}

async fn reject_friend_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(requester_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&requester_id)?;

    state.services.friend_room_service.reject_friend_request(&auth_user.user_id, &requester_id).await?;

    Ok(Json(json!({ "status": "rejected" })))
}

async fn cancel_friend_request(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(target_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&target_id)?;

    state.services.friend_room_service.cancel_friend_request(&auth_user.user_id, &target_id).await?;

    Ok(Json(json!({ "status": "cancelled" })))
}

async fn get_incoming_requests(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let requests = state.services.friend_room_service.get_incoming_requests(&auth_user.user_id).await?;

    Ok(Json(json!({ "requests": requests })))
}

async fn get_outgoing_requests(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let requests = state.services.friend_room_service.get_outgoing_requests(&auth_user.user_id).await?;

    Ok(Json(json!({ "requests": requests })))
}

async fn remove_friend(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    state.services.friend_room_service.remove_friend(&auth_user.user_id, &friend_id).await?;

    Ok(Json(json!({
        "removed": true,
        "user_id": friend_id,
        "removed_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn update_friend_note(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
    Json(body): Json<UpdateNoteRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    if body.note.len() > 1000 {
        return Err(ApiError::bad_request("Note exceeds maximum length of 1000 characters".to_string()));
    }

    state.services.friend_room_service.update_friend_note(&auth_user.user_id, &friend_id, &body.note).await?;

    Ok(Json(json!({
        "user_id": friend_id,
        "note": body.note,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn update_friend_status(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
    Json(body): Json<UpdateStatusRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    let valid_statuses = ["favorite", "normal", "blocked", "hidden"];
    if !valid_statuses.contains(&body.status.as_str()) {
        return Err(ApiError::bad_request(format!("Invalid status. Valid values: {}", valid_statuses.join(", "))));
    }

    state.services.friend_room_service.update_friend_status(&auth_user.user_id, &friend_id, &body.status).await?;

    Ok(Json(json!({
        "user_id": friend_id,
        "status": body.status,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn get_friend_info(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    let info = state
        .services
        .friend_room_service
        .get_friend_info(&auth_user.user_id, &friend_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Friend {friend_id} not found")))?;

    Ok(Json(info))
}

async fn update_friend_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
    Json(body): Json<UpdateDisplaynameRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    if body.display_name.is_empty() || body.display_name.len() > 256 {
        return Err(ApiError::bad_request("Display name must be between 1 and 256 characters".to_string()));
    }

    state
        .services
        .friend_room_service
        .update_friend_displayname(&auth_user.user_id, &friend_id, &body.display_name)
        .await?;

    Ok(Json(json!({
        "user_id": friend_id,
        "displayname": body.display_name,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn get_received_requests(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<axum::http::Response<axum::body::Body>, ApiError> {
    let requests = state.services.friend_room_service.get_incoming_requests(&auth_user.user_id).await?;

    let body = Json(json!({ "requests": requests }));
    let mut response = body.into_response();
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("deprecation"),
        axum::http::HeaderValue::from_static("true"),
    );
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("link"),
        axum::http::HeaderValue::from_static(
            "</_matrix/client/v3/friends/requests/incoming>; rel=\"successor-version\"",
        ),
    );
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("sunset"),
        axum::http::HeaderValue::from_static("2027-01-01"),
    );
    Ok(response)
}

async fn get_friend_status(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(friend_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&friend_id)?;

    let status = state.services.friend_room_service.get_friend_status(&auth_user.user_id, &friend_id).await?;

    Ok(Json(status))
}

async fn check_friendship(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(target_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&target_id)?;

    let is_friend = state.services.friend_room_service.check_friendship(&auth_user.user_id, &target_id).await?;

    Ok(Json(json!({
        "user_id": target_id,
        "is_friend": is_friend,
        "are_friends": is_friend
    })))
}

#[derive(Debug, Deserialize)]
pub struct FriendSuggestionsQuery {
    pub limit: Option<i64>,
}

async fn get_friend_suggestions(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(query): Query<FriendSuggestionsQuery>,
) -> Result<Json<Value>, ApiError> {
    let suggestions =
        state.services.friend_room_service.get_friend_suggestions(&auth_user.user_id, query.limit).await?;

    Ok(Json(json!({
        "suggestions": suggestions
    })))
}

// 好友分组相关处理函数

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RenameGroupRequest {
    pub name: String,
}

async fn get_friend_groups(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let groups = state.services.friend_room_service.get_friend_groups(&auth_user.user_id).await?;

    Ok(Json(json!({
        "groups": groups
    })))
}

async fn create_friend_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateGroupRequest>,
) -> Result<Json<Value>, ApiError> {
    if body.name.is_empty() || body.name.len() > 50 {
        return Err(ApiError::bad_request("Group name must be between 1 and 50 characters".to_string()));
    }

    let group = state.services.friend_room_service.create_friend_group(&auth_user.user_id, &body.name).await?;

    Ok(Json(group))
}

async fn delete_friend_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(group_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.services.friend_room_service.delete_friend_group(&auth_user.user_id, &group_id).await?;

    Ok(Json(json!({
        "deleted": true,
        "group_id": group_id,
        "deleted_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn rename_friend_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(group_id): Path<String>,
    Json(body): Json<RenameGroupRequest>,
) -> Result<Json<Value>, ApiError> {
    if body.name.is_empty() || body.name.len() > 50 {
        return Err(ApiError::bad_request("Group name must be between 1 and 50 characters".to_string()));
    }

    state.services.friend_room_service.rename_friend_group(&auth_user.user_id, &group_id, &body.name).await?;

    Ok(Json(json!({
        "group_id": group_id,
        "name": body.name,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn add_friend_to_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((group_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    state.services.friend_room_service.add_friend_to_group(&auth_user.user_id, &group_id, &user_id).await?;

    Ok(Json(json!({
        "group_id": group_id,
        "user_id": user_id,
        "added_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn remove_friend_from_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((group_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    state.services.friend_room_service.remove_friend_from_group(&auth_user.user_id, &group_id, &user_id).await?;

    Ok(Json(json!({
        "group_id": group_id,
        "user_id": user_id,
        "removed_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn get_friends_in_group(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(group_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let friends = state.services.friend_room_service.get_friends_in_group(&auth_user.user_id, &group_id).await?;

    Ok(Json(json!({
        "friends": friends
    })))
}

async fn get_groups_for_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let groups = state.services.friend_room_service.get_groups_for_user(&auth_user.user_id, &user_id).await?;

    Ok(Json(json!({
        "groups": groups
    })))
}

async fn get_friend_dm(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    let room_id = state.services.friend_room_service.get_existing_dm_room_id(&auth_user.user_id, &user_id).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": user_id,
    })))
}

async fn create_friend_dm(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let config = crate::services::room::service::CreateRoomConfig {
        visibility: Some("private".to_string()),
        room_alias_name: None,
        name: None,
        topic: None,
        invite_list: Some(vec![user_id.clone()]),
        preset: Some("private_chat".to_string()),
        encryption: None,
        history_visibility: None,
        is_direct: Some(true),
        room_type: None,
        initial_state: None,
        creation_content: None,
        room_version: None,
        power_level_content_override: None,
    };

    let result = state
        .services
        .friend_room_service
        .ensure_direct_room(&auth_user.user_id, &user_id, config, Some(&auth_user.user_id))
        .await?;

    Ok(Json(json!({
        "room_id": result.room_id,
        "user_id": user_id,
        "created": result.created,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_friend_request_serialization() {
        let req = AddFriendRequest { user_id: "@test:example.com".to_string(), message: Some("Hello!".to_string()) };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("@test:example.com"));
    }

    #[test]
    fn test_update_note_request_serialization() {
        let req = UpdateNoteRequest { note: "Best friend".to_string() };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Best friend"));
    }

    #[test]
    fn test_friend_request_status_serialization() {
        let status = FriendRequestStatus::Pending;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"pending\"");

        let status = FriendRequestStatus::Accepted;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"accepted\"");
    }

    #[test]
    fn test_friend_search_query_supports_query_alias() {
        let query: FriendSearchQuery = serde_json::from_value(serde_json::json!({
            "query": "alice",
            "limit": 5
        }))
        .unwrap();
        assert_eq!(query.q.as_deref(), Some("alice"));
        assert_eq!(query.limit, Some(5));
    }

    #[test]
    fn test_resolve_friend_search_term_prefers_body_query_alias() {
        let query = FriendSearchQuery { q: Some("from_query".to_string()), mode: None, limit: None };
        let body = serde_json::json!({
            "query": "from_body"
        });

        assert_eq!(resolve_friend_search_term(&query, Some(&body)), Some("from_body".to_string()));
    }
}
