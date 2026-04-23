use crate::common::error::ApiError;
use crate::services::thread_service::{
    CreateReplyRequest, CreateThreadRequest, GetThreadRequest, ListThreadsRequest, MarkReadRequest,
    SubscribeRequest, SubscribedThreadsResponse, ThreadDetailResponse, ThreadListResponse,
    UnreadThreadsResponse,
};
use crate::web::routes::{ensure_room_member_strict, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
struct CreateThreadBody {
    #[serde(default)]
    room_id: Option<String>,
    root_event_id: String,
    #[serde(rename = "content")]
    _content: serde_json::Value,
    #[serde(rename = "origin_server_ts")]
    _origin_server_ts: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateReplyBody {
    #[serde(default)]
    #[serde(rename = "thread_id")]
    _thread_id: Option<String>,
    event_id: String,
    root_event_id: String,
    content: serde_json::Value,
    in_reply_to_event_id: Option<String>,
    origin_server_ts: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SubscribeBody {
    notification_level: String,
}

#[derive(Debug, Deserialize)]
struct MarkReadBody {
    event_id: String,
    origin_server_ts: i64,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit: Option<i32>,
    from: Option<String>,
    include_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ThreadQuery {
    include_replies: Option<bool>,
    reply_limit: Option<i32>,
}

#[derive(Debug, Serialize)]
struct ThreadResponse {
    thread_id: Option<String>,
    root_event_id: String,
    room_id: String,
    sender: String,
    reply_count: i64,
    last_reply_event_id: Option<String>,
    last_reply_sender: Option<String>,
    last_reply_ts: Option<i64>,
    participants: Option<serde_json::Value>,
    is_fetched: bool,
    created_ts: i64,
}

impl From<crate::storage::thread::ThreadRoot> for ThreadResponse {
    fn from(root: crate::storage::thread::ThreadRoot) -> Self {
        Self {
            thread_id: root.thread_id,
            root_event_id: root.root_event_id,
            room_id: root.room_id,
            sender: root.sender,
            reply_count: root.reply_count,
            last_reply_event_id: root.last_reply_event_id,
            last_reply_sender: root.last_reply_sender,
            last_reply_ts: root.last_reply_ts,
            participants: root.participants,
            is_fetched: root.is_fetched,
            created_ts: root.created_ts,
        }
    }
}

#[derive(Debug, Serialize)]
struct ReplyResponse {
    event_id: String,
    thread_id: String,
    room_id: String,
    sender: String,
    content: serde_json::Value,
    origin_server_ts: i64,
    in_reply_to_event_id: Option<String>,
    is_edited: bool,
    is_redacted: bool,
}

impl From<crate::storage::thread::ThreadReply> for ReplyResponse {
    fn from(reply: crate::storage::thread::ThreadReply) -> Self {
        Self {
            event_id: reply.event_id,
            thread_id: reply.thread_id,
            room_id: reply.room_id,
            sender: reply.sender,
            content: reply.content,
            origin_server_ts: reply.origin_server_ts,
            in_reply_to_event_id: reply.in_reply_to_event_id,
            is_edited: reply.is_edited,
            is_redacted: reply.is_redacted,
        }
    }
}

pub fn create_thread_routes(state: AppState) -> Router<AppState> {
    Router::new()
        // Global threads endpoints (v1)
        .route("/_matrix/client/v1/threads", get(list_threads_global))
        .route("/_matrix/client/v1/threads", post(create_thread_global))
        .route(
            "/_matrix/client/v1/threads/subscribed",
            get(get_subscribed_threads),
        )
        .route(
            "/_matrix/client/v1/threads/unread",
            get(get_unread_threads_global),
        )
        .route(
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads",
            get(list_threads_legacy_search),
        )
        // Room-level threads (v1)
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads",
            post(create_thread),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads",
            get(list_threads),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/search",
            get(search_threads),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/unread",
            get(get_unread_threads),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}",
            get(get_thread),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}",
            delete(delete_thread),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/freeze",
            post(freeze_thread),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unfreeze",
            post(unfreeze_thread),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies",
            post(add_reply),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies",
            get(get_replies),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/subscribe",
            post(subscribe_thread),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unsubscribe",
            post(unsubscribe_thread),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/mute",
            post(mute_thread),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/read",
            post(mark_read),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/stats",
            get(get_stats),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/replies/{event_id}/redact",
            post(redact_reply),
        )
        .with_state(state)
}

fn build_legacy_threads_response(response: ThreadListResponse) -> Value {
    let chunk = response
        .threads
        .into_iter()
        .map(|thread| {
            json!({
                "event_id": thread.root_event_id,
                "sender": thread.root_sender,
                "content": thread.root_content,
                "origin_server_ts": thread.root_origin_server_ts
            })
        })
        .collect::<Vec<_>>();

    json!({
        "chunk": chunk,
        "next_batch": response.next_batch
    })
}

async fn ensure_thread_room_access(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict(
        state,
        auth_user,
        room_id,
        "You must be a member of this room to access thread data",
    )
    .await
}

async fn ensure_thread_management_access(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict(
        state,
        auth_user,
        room_id,
        "You must be a member of this room to manage threads",
    )
    .await?;

    let is_creator = state
        .services
        .room_service
        .is_room_creator(room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room creator: {}", e)))?;

    if !is_creator {
        return Err(ApiError::forbidden(
            "Only room admins can manage threads".to_string(),
        ));
    }

    Ok(())
}

fn ensure_thread_user_matches(
    auth_user: &AuthenticatedUser,
    requested_user_id: &str,
) -> Result<(), ApiError> {
    if requested_user_id != auth_user.user_id {
        return Err(ApiError::forbidden(
            "You can only query thread data for your own user".to_string(),
        ));
    }

    Ok(())
}

async fn list_visible_threads(
    state: &AppState,
    user_id: &str,
    limit: Option<i32>,
    from: Option<&str>,
) -> Result<ThreadListResponse, ApiError> {
    let room_ids = state
        .services
        .room_storage
        .get_user_rooms(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list user rooms: {}", e)))?;

    let mut threads = Vec::new();
    for room_id in room_ids {
        let mut response = state
            .services
            .thread_service
            .list_threads(ListThreadsRequest {
                room_id,
                limit: None,
                from: None,
                include_all: true,
            })
            .await?;
        threads.append(&mut response.threads);
    }

    threads.sort_by(|a, b| {
        b.updated_ts
            .cmp(&a.updated_ts)
            .then_with(|| b.created_ts.cmp(&a.created_ts))
            .then_with(|| a.thread_id.cmp(&b.thread_id))
    });

    let start = from
        .and_then(|token| threads.iter().position(|thread| thread.thread_id == token))
        .map(|idx| idx + 1)
        .unwrap_or(0);

    let total = threads.len() as i32;
    let page_size = limit.unwrap_or(20).max(1) as usize;
    let page: Vec<_> = threads.into_iter().skip(start).take(page_size).collect();
    let next_batch = if start + page.len() < total as usize {
        page.last().map(|thread| thread.thread_id.clone())
    } else {
        None
    };

    Ok(ThreadListResponse {
        threads: page,
        next_batch,
        total,
    })
}

async fn create_thread(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateThreadBody>,
) -> Result<Json<ThreadResponse>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let user_id = auth_user.user_id;
    let request = CreateThreadRequest {
        room_id,
        root_event_id: body.root_event_id,
    };

    let thread = state
        .services
        .thread_service
        .create_thread(&user_id, request)
        .await?;

    Ok(Json(ThreadResponse::from(thread)))
}

async fn list_threads(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<ListQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<ThreadListResponse>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let request = ListThreadsRequest {
        room_id,
        limit: query.limit,
        from: query.from,
        include_all: query.include_all.unwrap_or(false),
    };

    let response = state.services.thread_service.list_threads(request).await?;
    Ok(Json(response))
}

async fn list_threads_legacy_search(
    State(state): State<AppState>,
    Path((user_id, room_id)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    ensure_thread_user_matches(&auth_user, &user_id)?;
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let request = ListThreadsRequest {
        room_id,
        limit: query.limit,
        from: query.from,
        include_all: query.include_all.unwrap_or(false),
    };

    let response = state.services.thread_service.list_threads(request).await?;
    Ok(Json(build_legacy_threads_response(response)))
}

async fn get_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    Query(query): Query<ThreadQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<ThreadDetailResponse>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let request = GetThreadRequest {
        room_id,
        thread_id,
        include_replies: query.include_replies.unwrap_or(true),
        reply_limit: query.reply_limit,
    };

    let response = state
        .services
        .thread_service
        .get_thread(request, Some(&auth_user.user_id))
        .await?;
    Ok(Json(response))
}

async fn delete_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    ensure_thread_management_access(&state, &auth_user, &room_id).await?;

    let thread = state
        .services
        .thread_storage
        .get_thread_root(&room_id, &thread_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if thread.is_none() {
        return Err(ApiError::not_found(format!(
            "Thread '{}' not found",
            thread_id
        )));
    }

    state
        .services
        .thread_service
        .delete_thread(&room_id, &thread_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn freeze_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    ensure_thread_management_access(&state, &auth_user, &room_id).await?;

    let thread = state
        .services
        .thread_storage
        .get_thread_root(&room_id, &thread_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if thread.is_none() {
        return Err(ApiError::not_found(format!(
            "Thread '{}' not found",
            thread_id
        )));
    }

    state
        .services
        .thread_service
        .freeze_thread(&room_id, &thread_id)
        .await?;
    Ok(StatusCode::OK)
}

async fn unfreeze_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    ensure_thread_management_access(&state, &auth_user, &room_id).await?;

    let thread = state
        .services
        .thread_storage
        .get_thread_root(&room_id, &thread_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if thread.is_none() {
        return Err(ApiError::not_found(format!(
            "Thread '{}' not found",
            thread_id
        )));
    }

    state
        .services
        .thread_service
        .unfreeze_thread(&room_id, &thread_id)
        .await?;
    Ok(StatusCode::OK)
}

async fn add_reply(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateReplyBody>,
) -> Result<Json<ReplyResponse>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let user_id = auth_user.user_id;

    let request = CreateReplyRequest {
        room_id,
        thread_id,
        event_id: body.event_id,
        root_event_id: body.root_event_id,
        content: body.content,
        in_reply_to_event_id: body.in_reply_to_event_id,
        origin_server_ts: body
            .origin_server_ts
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
    };

    let reply = state
        .services
        .thread_service
        .add_reply(&user_id, request)
        .await?;
    Ok(Json(ReplyResponse::from(reply)))
}

async fn get_replies(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Vec<ReplyResponse>>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let storage = &state.services.thread_storage;

    let replies = storage
        .get_thread_replies(&room_id, &thread_id, query.limit, query.from)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get replies: {}", e)))?;

    Ok(Json(replies.into_iter().map(ReplyResponse::from).collect()))
}

async fn subscribe_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<crate::storage::thread::ThreadSubscription>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let user_id = auth_user.user_id;

    let request = SubscribeRequest {
        room_id,
        thread_id,
        user_id,
        notification_level: body.notification_level,
    };

    let subscription = state.services.thread_service.subscribe(request).await?;
    Ok(Json(subscription))
}

async fn unsubscribe_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let user_id = auth_user.user_id;

    state
        .services
        .thread_service
        .unsubscribe(&room_id, &thread_id, &user_id)
        .await?;
    Ok(StatusCode::OK)
}

async fn mute_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<Json<crate::storage::thread::ThreadSubscription>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let user_id = auth_user.user_id;

    let subscription = state
        .services
        .thread_service
        .mute_thread(&room_id, &thread_id, &user_id)
        .await?;
    Ok(Json(subscription))
}

async fn mark_read(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
    Json(body): Json<MarkReadBody>,
) -> Result<Json<crate::storage::thread::ThreadReadReceipt>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let user_id = auth_user.user_id;

    let request = MarkReadRequest {
        room_id,
        thread_id,
        user_id,
        event_id: body.event_id,
        origin_server_ts: body.origin_server_ts,
    };

    let receipt = state.services.thread_service.mark_read(request).await?;
    Ok(Json(receipt))
}

async fn get_unread_threads(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<Json<UnreadThreadsResponse>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let user_id = auth_user.user_id;

    let response = state
        .services
        .thread_service
        .get_unread_threads(&user_id, Some(&room_id))
        .await?;
    Ok(Json(response))
}

async fn search_threads(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<SearchQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Vec<crate::storage::thread::ThreadSummary>>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let results = state
        .services
        .thread_service
        .search_threads(&room_id, &query.q, query.limit)
        .await?;
    Ok(Json(results))
}

async fn get_stats(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Option<crate::storage::thread::ThreadStatistics>>, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let stats = state
        .services
        .thread_service
        .get_thread_statistics(&room_id, &thread_id)
        .await?;
    Ok(Json(stats))
}

async fn redact_reply(
    State(state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    state
        .services
        .thread_service
        .redact_reply(&room_id, &event_id)
        .await?;
    Ok(StatusCode::OK)
}

async fn list_threads_global(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    query: Query<ListQuery>,
) -> Result<Json<ThreadListResponse>, ApiError> {
    let response = list_visible_threads(
        &state,
        &auth_user.user_id,
        query.limit,
        query.from.as_deref(),
    )
    .await?;
    Ok(Json(response))
}

async fn create_thread_global(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateThreadBody>,
) -> Result<Json<ThreadResponse>, ApiError> {
    let room_id = body
        .room_id
        .ok_or_else(|| ApiError::bad_request("room_id is required".to_string()))?;

    ensure_thread_room_access(&state, &auth_user, &room_id).await?;

    let request = CreateThreadRequest {
        room_id,
        root_event_id: body.root_event_id,
    };

    let thread = state
        .services
        .thread_service
        .create_thread(&auth_user.user_id, request)
        .await?;

    Ok(Json(ThreadResponse::from(thread)))
}

async fn get_subscribed_threads(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<SubscribedThreadsResponse>, ApiError> {
    let response = state
        .services
        .thread_service
        .get_subscribed_threads(&auth_user.user_id, Some(50))
        .await?;
    Ok(Json(response))
}

async fn get_unread_threads_global(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<UnreadThreadsResponse>, ApiError> {
    let response = state
        .services
        .thread_service
        .get_unread_threads(&auth_user.user_id, None)
        .await?;
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::thread::ThreadSummary;

    #[test]
    fn test_build_legacy_threads_response_shape() {
        let response = ThreadListResponse {
            threads: vec![ThreadSummary {
                id: 1,
                room_id: "!room:localhost".to_string(),
                thread_id: "$thread".to_string(),
                root_event_id: "$root".to_string(),
                root_sender: "@alice:localhost".to_string(),
                root_content: json!({ "body": "hello" }),
                root_origin_server_ts: 42,
                latest_event_id: None,
                latest_sender: None,
                latest_content: None,
                latest_origin_server_ts: None,
                reply_count: 0,
                participants: json!(["@alice:localhost"]),
                is_frozen: false,
                created_ts: 42,
                updated_ts: 42,
            }],
            next_batch: Some("$thread".to_string()),
            total: 1,
        };

        let legacy = build_legacy_threads_response(response);
        assert_eq!(legacy["chunk"][0]["event_id"], "$root");
        assert_eq!(legacy["chunk"][0]["sender"], "@alice:localhost");
        assert_eq!(legacy["next_batch"], "$thread");
    }

    #[test]
    fn test_legacy_search_thread_route_path_shape() {
        let route = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads";
        assert!(route.starts_with("/_matrix/client/v3/user/"));
        assert!(route.ends_with("/threads"));
    }
}
