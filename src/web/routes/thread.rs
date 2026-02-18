use crate::common::error::ApiError;
use crate::services::thread_service::{
    CreateReplyRequest, CreateThreadRequest, GetThreadRequest, ListThreadsRequest,
    MarkReadRequest, SubscribeRequest, ThreadDetailResponse, ThreadListResponse,
    UnreadThreadsResponse,
};
use crate::web::routes::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct CreateThreadBody {
    #[allow(dead_code)]
    room_id: String,
    root_event_id: String,
    content: serde_json::Value,
    origin_server_ts: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateReplyBody {
    #[allow(dead_code)]
    thread_id: String,
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
    thread_id: String,
    root_event_id: String,
    room_id: String,
    sender: String,
    content: serde_json::Value,
    origin_server_ts: i64,
    reply_count: i32,
    is_frozen: bool,
}

impl From<crate::storage::thread::ThreadRoot> for ThreadResponse {
    fn from(root: crate::storage::thread::ThreadRoot) -> Self {
        Self {
            thread_id: root.thread_id,
            root_event_id: root.root_event_id,
            room_id: root.room_id,
            sender: root.sender,
            content: root.content,
            origin_server_ts: root.origin_server_ts,
            reply_count: root.reply_count,
            is_frozen: root.is_frozen,
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
        .route("/rooms/{room_id}/threads", post(create_thread))
        .route("/rooms/{room_id}/threads", get(list_threads))
        .route("/rooms/{room_id}/threads/search", get(search_threads))
        .route("/rooms/{room_id}/threads/unread", get(get_unread_threads))
        .route("/rooms/{room_id}/threads/{thread_id}", get(get_thread))
        .route("/rooms/{room_id}/threads/{thread_id}", delete(delete_thread))
        .route("/rooms/{room_id}/threads/{thread_id}/freeze", post(freeze_thread))
        .route("/rooms/{room_id}/threads/{thread_id}/unfreeze", post(unfreeze_thread))
        .route("/rooms/{room_id}/threads/{thread_id}/replies", post(add_reply))
        .route("/rooms/{room_id}/threads/{thread_id}/replies", get(get_replies))
        .route("/rooms/{room_id}/threads/{thread_id}/subscribe", post(subscribe_thread))
        .route("/rooms/{room_id}/threads/{thread_id}/unsubscribe", post(unsubscribe_thread))
        .route("/rooms/{room_id}/threads/{thread_id}/mute", post(mute_thread))
        .route("/rooms/{room_id}/threads/{thread_id}/read", post(mark_read))
        .route("/rooms/{room_id}/threads/{thread_id}/stats", get(get_stats))
        .route("/rooms/{room_id}/replies/{event_id}/redact", post(redact_reply))
        .with_state(state)
}

async fn create_thread(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    user_id: axum::extract::Extension<String>,
    Json(body): Json<CreateThreadBody>,
) -> Result<Json<ThreadResponse>, ApiError> {
    let user_id = user_id.0;
    
    let request = CreateThreadRequest {
        room_id,
        root_event_id: body.root_event_id,
        content: body.content,
        origin_server_ts: body.origin_server_ts.unwrap_or_else(|| {
            chrono::Utc::now().timestamp_millis()
        }),
    };

    let thread = state.services.thread_service.create_thread(&user_id, request).await?;
    
    Ok(Json(ThreadResponse::from(thread)))
}

async fn list_threads(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ThreadListResponse>, ApiError> {
    let request = ListThreadsRequest {
        room_id,
        limit: query.limit,
        from: query.from,
        include_all: query.include_all.unwrap_or(false),
    };

    let response = state.services.thread_service.list_threads(request).await?;
    Ok(Json(response))
}

async fn get_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    Query(query): Query<ThreadQuery>,
    user_id: Option<axum::extract::Extension<String>>,
) -> Result<Json<ThreadDetailResponse>, ApiError> {
    let request = GetThreadRequest {
        room_id,
        thread_id,
        include_replies: query.include_replies.unwrap_or(true),
        reply_limit: query.reply_limit,
    };

    let user_id_str = user_id.as_ref().map(|ext| ext.0.as_str());
    let response = state.services.thread_service.get_thread(request, user_id_str).await?;
    Ok(Json(response))
}

async fn delete_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
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

    state.services.thread_service.delete_thread(&room_id, &thread_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn freeze_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
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

    state.services.thread_service.freeze_thread(&room_id, &thread_id).await?;
    Ok(StatusCode::OK)
}

async fn unfreeze_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
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

    state.services.thread_service.unfreeze_thread(&room_id, &thread_id).await?;
    Ok(StatusCode::OK)
}

async fn add_reply(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    user_id: axum::extract::Extension<String>,
    Json(body): Json<CreateReplyBody>,
) -> Result<Json<ReplyResponse>, ApiError> {
    let user_id = user_id.0;
    
    let request = CreateReplyRequest {
        room_id,
        thread_id,
        event_id: body.event_id,
        root_event_id: body.root_event_id,
        content: body.content,
        in_reply_to_event_id: body.in_reply_to_event_id,
        origin_server_ts: body.origin_server_ts.unwrap_or_else(|| {
            chrono::Utc::now().timestamp_millis()
        }),
    };

    let reply = state.services.thread_service.add_reply(&user_id, request).await?;
    Ok(Json(ReplyResponse::from(reply)))
}

async fn get_replies(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<ReplyResponse>>, ApiError> {
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
    user_id: axum::extract::Extension<String>,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<crate::storage::thread::ThreadSubscription>, ApiError> {
    let user_id = user_id.0;
    
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
    user_id: axum::extract::Extension<String>,
) -> Result<StatusCode, ApiError> {
    let user_id = user_id.0;
    
    state.services.thread_service.unsubscribe(&room_id, &thread_id, &user_id).await?;
    Ok(StatusCode::OK)
}

async fn mute_thread(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    user_id: axum::extract::Extension<String>,
) -> Result<Json<crate::storage::thread::ThreadSubscription>, ApiError> {
    let user_id = user_id.0;
    
    let subscription = state.services.thread_service.mute_thread(&room_id, &thread_id, &user_id).await?;
    Ok(Json(subscription))
}

async fn mark_read(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
    user_id: axum::extract::Extension<String>,
    Json(body): Json<MarkReadBody>,
) -> Result<Json<crate::storage::thread::ThreadReadReceipt>, ApiError> {
    let user_id = user_id.0;
    
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
    user_id: axum::extract::Extension<String>,
) -> Result<Json<UnreadThreadsResponse>, ApiError> {
    let user_id = user_id.0;
    
    let response = state.services.thread_service.get_unread_threads(&user_id, Some(&room_id)).await?;
    Ok(Json(response))
}

async fn search_threads(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<crate::storage::thread::ThreadSummary>>, ApiError> {
    let results = state.services.thread_service.search_threads(&room_id, &query.q, query.limit).await?;
    Ok(Json(results))
}

async fn get_stats(
    State(state): State<AppState>,
    Path((room_id, thread_id)): Path<(String, String)>,
) -> Result<Json<Option<crate::storage::thread::ThreadStatistics>>, ApiError> {
    let stats = state.services.thread_service.get_thread_statistics(&room_id, &thread_id).await?;
    Ok(Json(stats))
}

async fn redact_reply(
    State(state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.services.thread_service.redact_reply(&room_id, &event_id).await?;
    Ok(StatusCode::OK)
}
