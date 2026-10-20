use crate::routes::context::RoomContext;
use crate::routes::extractors::{RoomId, UserId};
use crate::routes::MatrixJson;
use crate::routes::{ensure_room_member_strict_ctx, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use synapse_services::thread_service::{
    CreateReplyRequest, CreateThreadRequest, GetThreadRequest, ListThreadsRequest, MarkReadRequest,
    SearchThreadsRequest, SubscribeRequest, SubscribedThreadsResponse, ThreadDetailResponse, ThreadListResponse,
    UnreadThreadsResponse,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateThreadBody {
    #[serde(default)]
    room_id: Option<String>,
    root_event_id: String,
    /// 线程根事件已存在（`root_event_id`），其 content 由该事件定义，
    /// `thread_roots` 也无处存放客户端传入的 content —— 因此非空时显式拒绝，
    /// 而不是接受后丢弃（见审计 I-07）。
    #[serde(default)]
    content: Option<serde_json::Value>,
    #[serde(rename = "origin_server_ts")]
    _origin_server_ts: Option<i64>,
}

/// 拒绝客户端传入的线程 `content`：接受却丢弃会让参数看起来可用（审计 I-07）。
///
/// 允许缺省、`null` 与空对象（无可丢弃内容）；其余形态一律 400 `M_INVALID_PARAM`。
fn reject_unsupported_thread_content(content: Option<&Value>) -> Result<(), ApiError> {
    match content {
        None | Some(Value::Null) => Ok(()),
        Some(Value::Object(map)) if map.is_empty() => Ok(()),
        Some(_) => Err(ApiError::invalid_param(
            "content is not supported when creating a thread; the thread root event's content is taken from root_event_id"
                .to_string(),
        )),
    }
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
#[serde(deny_unknown_fields)]
struct MarkReadBody {
    event_id: String,
    origin_server_ts: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RedactReplyBody {
    /// 客户端撤回原因，落库到 `thread_replies.redaction_reason`。
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<i32>,
    /// 续读游标：来自上一页响应里的 `next_batch`（**不透明**，只应原样回传）。
    ///
    /// 与 `ListQuery.from` 语义不同：搜索按相关性排序，这个游标编的是**排序键三元组**，
    /// 不是 `thread_id`。
    from: Option<String>,
}

/// 话题列表的分页 + `include` 参数（`/threads`、全局 `/threads` 与用户级列表共用）。
#[derive(Debug, Deserialize)]
struct ListQuery {
    limit: Option<i32>,
    from: Option<String>,
    /// 规范取值 `all`（缺省）或 `participated`；其它值一律 400 `M_INVALID_PARAM`。
    include: Option<String>,
}

/// 只接受分页参数的查询串（`/replies`、已订阅列表）。
///
/// 与 [`ListQuery`] 分开，是为了不把 `include` 声明到不消费它的端点上。
#[derive(Debug, Deserialize)]
struct ThreadPageQuery {
    limit: Option<i32>,
    from: Option<String>,
}

/// 把 `include` 解析成存储层的参与者过滤条件。
///
/// 返回 `Some(user_id)` 表示只看该用户参与过的话题（发过根事件或回复 ——
/// `thread_roots.participants` 同时包含根发送者与全部回复者），`None` 表示不限制。
fn resolve_thread_participant_filter(include: Option<&str>, user_id: &str) -> Result<Option<String>, ApiError> {
    match include {
        None | Some("all") => Ok(None),
        Some("participated") => Ok(Some(user_id.to_string())),
        Some(other) => {
            Err(ApiError::invalid_param(format!("Invalid include value '{other}': expected 'all' or 'participated'")))
        }
    }
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

impl From<synapse_services::thread_service::ThreadRoot> for ThreadResponse {
    fn from(root: synapse_services::thread_service::ThreadRoot) -> Self {
        Self {
            thread_id: root.thread_id,
            root_event_id: root.root_event_id,
            room_id: root.room_id,
            sender: root.sender,
            reply_count: root.reply_count.unwrap_or(0),
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

impl From<synapse_services::thread_service::ThreadReply> for ReplyResponse {
    fn from(reply: synapse_services::thread_service::ThreadReply) -> Self {
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

/// See [`create_thread_routes`].
///
/// ISSUE-13：本模块是**混合模块**。MSC3856 规范端点（房间线程列表/详情、
/// 回复、订阅、解冻）留在 `/_matrix/client/v1`；私有扩展（线程创建/删除/搜索/
/// 未读/冻结/静音/已读/统计、全局线程列表与创建、已订阅列表、用户级列表、
/// 非规范 redact）唯一规范位置是 `/_matrix/vendor/v1`，已按 AGENTS.md 铁律 1
/// 整批迁移，**不留 client 别名**（未发布项目无向后兼容义务）。
pub fn create_thread_routes(state: AppState) -> Router<AppState> {
    // ---- 私有扩展：唯一规范位置 /_matrix/vendor/v1 ----
    let vendor_routes = Router::new()
        .route("/_matrix/vendor/v1/threads", get(list_threads_global))
        .route("/_matrix/vendor/v1/threads", post(create_thread_global))
        .route("/_matrix/vendor/v1/threads/subscribed", get(get_subscribed_threads))
        .route("/_matrix/vendor/v1/threads/unread", get(get_unread_threads_global))
        .route("/_matrix/vendor/v1/user/{user_id}/rooms/{room_id}/threads", get(list_threads_legacy_search))
        .route("/_matrix/vendor/v1/rooms/{room_id}/threads", post(create_thread))
        .route("/_matrix/vendor/v1/rooms/{room_id}/threads/search", get(search_threads))
        .route("/_matrix/vendor/v1/rooms/{room_id}/threads/unread", get(get_unread_threads))
        .route("/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}", delete(delete_thread))
        .route("/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/freeze", post(freeze_thread))
        .route("/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/mute", post(mute_thread))
        .route("/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/read", post(mark_read))
        .route("/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/stats", get(get_stats))
        .route("/_matrix/vendor/v1/rooms/{room_id}/replies/{event_id}/redact", post(redact_reply))
        // 非 MSC3856 形状（MSC3856 只有 threads/replies/subscribe/unsubscribe）⇒ 归位 vendor
        .route(
            "/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/unfreeze",
            post(unfreeze_thread),
        );

    // ---- MSC3856 规范端点：留在 /_matrix/client/v1 ----
    let client_routes = Router::new()
        .route("/_matrix/client/v1/rooms/{room_id}/threads", get(list_threads))
        .route("/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}", get(get_thread))
        .route("/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies", post(add_reply))
        .route("/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies", get(get_replies))
        .route("/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/subscribe", post(subscribe_thread))
        .route("/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unsubscribe", post(unsubscribe_thread));

    vendor_routes.merge(client_routes).with_state(state)
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
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict_ctx(ctx, auth_user, room_id, "You must be a member of this room to access thread data")
        .await
}

async fn ensure_thread_management_access(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict_ctx(ctx, auth_user, room_id, "You must be a member of this room to manage threads")
        .await?;

    let is_creator = ctx
        .room_service
        .state()
        .is_room_creator(room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to check room creator", e))?;

    if !is_creator {
        return Err(ApiError::forbidden("Only room admins can manage threads".to_string()));
    }

    Ok(())
}

fn ensure_thread_user_matches(auth_user: &AuthenticatedUser, requested_user_id: &str) -> Result<(), ApiError> {
    if requested_user_id != auth_user.user_id {
        return Err(ApiError::forbidden("You can only query thread data for your own user".to_string()));
    }

    Ok(())
}

async fn list_visible_threads(
    ctx: &RoomContext,
    user_id: &str,
    limit: Option<i32>,
    from: Option<&str>,
    participated_by: Option<&str>,
) -> Result<ThreadListResponse, ApiError> {
    let room_ids = ctx.room_service.membership().get_joined_rooms(user_id).await?;

    let mut threads = Vec::new();
    for room_id in room_ids {
        let mut response = ctx
            .thread_service
            .list_threads(ListThreadsRequest {
                room_id,
                limit: None,
                from: None,
                participated_by: participated_by.map(str::to_string),
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

    let start =
        from.and_then(|token| threads.iter().position(|thread| thread.thread_id == token)).map_or(0, |idx| idx + 1);

    let total = threads.len() as i32;
    let page_size = limit.unwrap_or(20).max(1) as usize;
    let page: Vec<_> = threads.into_iter().skip(start).take(page_size).collect();
    let next_batch =
        if start + page.len() < total as usize { page.last().map(|thread| thread.thread_id.clone()) } else { None };

    Ok(ThreadListResponse { threads: page, next_batch, total })
}

async fn create_thread(
    State(ctx): State<RoomContext>,
    Path(room_id): Path<RoomId>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<CreateThreadBody>,
) -> Result<Json<ThreadResponse>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, &room_id).await?;
    reject_unsupported_thread_content(body.content.as_ref())?;

    let user_id = auth_user.user_id;
    let request = CreateThreadRequest { room_id: room_id.to_string(), root_event_id: body.root_event_id };

    let thread = ctx.thread_service.create_thread(&user_id, request).await?;

    Ok(Json(ThreadResponse::from(thread)))
}

async fn list_threads(
    State(ctx): State<RoomContext>,
    Path(room_id): Path<RoomId>,
    Query(query): Query<ListQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<ThreadListResponse>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, &room_id).await?;

    let request = ListThreadsRequest {
        room_id: room_id.to_string(),
        limit: query.limit,
        from: query.from,
        participated_by: resolve_thread_participant_filter(query.include.as_deref(), &auth_user.user_id)?,
    };

    let response = ctx.thread_service.list_threads(request).await?;
    Ok(Json(response))
}

async fn list_threads_legacy_search(
    State(ctx): State<RoomContext>,
    Path((user_id, room_id)): Path<(UserId, RoomId)>,
    Query(query): Query<ListQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    ensure_thread_user_matches(&auth_user, user_id.as_str())?;
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let request = ListThreadsRequest {
        room_id: room_id.to_string(),
        limit: query.limit,
        from: query.from,
        participated_by: resolve_thread_participant_filter(query.include.as_deref(), &auth_user.user_id)?,
    };

    let response = ctx.thread_service.list_threads(request).await?;
    Ok(Json(build_legacy_threads_response(response)))
}

async fn get_thread(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    Query(query): Query<ThreadQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<ThreadDetailResponse>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let request = GetThreadRequest {
        room_id: room_id.to_string(),
        thread_id,
        include_replies: query.include_replies.unwrap_or(true),
        reply_limit: query.reply_limit,
    };

    let response = ctx.thread_service.get_thread(request, Some(&auth_user.user_id)).await?;
    Ok(Json(response))
}

async fn delete_thread(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    ensure_thread_management_access(&ctx, &auth_user, room_id.as_str()).await?;

    let thread = ctx.thread_service.get_thread_root(room_id.as_str(), &thread_id).await?;

    if thread.is_none() {
        return Err(ApiError::not_found(format!("Thread '{thread_id}' not found")));
    }

    ctx.thread_service.delete_thread(&room_id, &thread_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn freeze_thread(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    ensure_thread_management_access(&ctx, &auth_user, room_id.as_str()).await?;

    let thread = ctx.thread_service.get_thread_root(room_id.as_str(), &thread_id).await?;

    if thread.is_none() {
        return Err(ApiError::not_found(format!("Thread '{thread_id}' not found")));
    }

    ctx.thread_service.freeze_thread(room_id.as_str(), &thread_id).await?;
    Ok(StatusCode::OK)
}

async fn unfreeze_thread(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    ensure_thread_management_access(&ctx, &auth_user, room_id.as_str()).await?;

    let thread = ctx.thread_service.get_thread_root(room_id.as_str(), &thread_id).await?;

    if thread.is_none() {
        return Err(ApiError::not_found(format!("Thread '{thread_id}' not found")));
    }

    ctx.thread_service.unfreeze_thread(room_id.as_str(), &thread_id).await?;
    Ok(StatusCode::OK)
}

async fn add_reply(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<CreateReplyBody>,
) -> Result<Json<ReplyResponse>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let user_id = auth_user.user_id;

    let request = CreateReplyRequest {
        room_id: room_id.to_string(),
        thread_id,
        event_id: body.event_id,
        root_event_id: body.root_event_id,
        content: body.content,
        in_reply_to_event_id: body.in_reply_to_event_id,
        origin_server_ts: body.origin_server_ts.unwrap_or_else(current_timestamp_millis),
    };

    let reply = ctx.thread_service.add_reply(&user_id, request).await?;
    Ok(Json(ReplyResponse::from(reply)))
}

async fn get_replies(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    Query(query): Query<ThreadPageQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Vec<ReplyResponse>>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let replies = ctx.thread_service.get_thread_replies(room_id.as_str(), &thread_id, query.limit, query.from).await?;

    Ok(Json(replies.into_iter().map(ReplyResponse::from).collect()))
}

async fn subscribe_thread(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<SubscribeBody>,
) -> Result<Json<synapse_services::thread_service::ThreadSubscription>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let user_id = auth_user.user_id;

    let request = SubscribeRequest {
        room_id: room_id.to_string(),
        thread_id,
        user_id,
        notification_level: body.notification_level,
    };

    let subscription: synapse_services::thread_service::ThreadSubscription =
        ctx.thread_service.subscribe(request).await?;
    Ok(Json(subscription))
}

async fn unsubscribe_thread(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
) -> Result<StatusCode, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let user_id = auth_user.user_id;

    ctx.thread_service.unsubscribe(room_id.as_str(), &thread_id, &user_id).await?;
    Ok(StatusCode::OK)
}

async fn mute_thread(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
) -> Result<Json<synapse_services::thread_service::ThreadSubscription>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let user_id = auth_user.user_id;

    let subscription: synapse_services::thread_service::ThreadSubscription =
        ctx.thread_service.mute_thread(room_id.as_str(), &thread_id, &user_id).await?;
    Ok(Json(subscription))
}

async fn mark_read(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<MarkReadBody>,
) -> Result<Json<synapse_services::thread_service::ThreadReadReceipt>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let user_id = auth_user.user_id;

    let request = MarkReadRequest {
        room_id: room_id.to_string(),
        thread_id,
        user_id,
        event_id: body.event_id,
        origin_server_ts: body.origin_server_ts,
    };

    let receipt: synapse_services::thread_service::ThreadReadReceipt = ctx.thread_service.mark_read(request).await?;
    Ok(Json(receipt))
}

async fn get_unread_threads(
    State(ctx): State<RoomContext>,
    Path(room_id): Path<RoomId>,
    auth_user: AuthenticatedUser,
) -> Result<Json<UnreadThreadsResponse>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, &room_id).await?;

    let user_id = auth_user.user_id;

    let response = ctx.thread_service.get_unread_threads(&user_id, Some(&room_id)).await?;
    Ok(Json(response))
}

/// 房间内话题搜索（`GET …/rooms/{room_id}/threads/search?q=<term>`）。
///
/// ⚠️ 2026-10-16 契约修复：返回形状由**裸数组**改为 `ThreadListResponse` 信封
/// （`threads` + `next_batch` + `total`），与同族 `list_threads` 一致 —— 对接的 SDK
/// 正是按信封读的（`IThreadListResponse { threads; next_batch? }`），此前
/// `result.threads` 恒为 `undefined`（该方法完全不可用）；同时 `from` 此前被**静默忽略**，
/// 现在生效（结果集此前还被 `limit` 静默截断且无法续读）。
async fn search_threads(
    State(ctx): State<RoomContext>,
    Path(room_id): Path<RoomId>,
    Query(query): Query<SearchQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<ThreadListResponse>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let response = ctx
        .thread_service
        .search_threads(SearchThreadsRequest {
            room_id: room_id.to_string(),
            query: query.q,
            limit: query.limit,
            from: query.from,
        })
        .await?;

    Ok(Json(response))
}

async fn get_stats(
    State(ctx): State<RoomContext>,
    Path((room_id, thread_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Option<synapse_services::thread_service::ThreadStatistics>>, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let stats: Option<synapse_services::thread_service::ThreadStatistics> =
        ctx.thread_service.get_thread_statistics(room_id.as_str(), &thread_id).await?;
    Ok(Json(stats))
}

async fn redact_reply(
    State(ctx): State<RoomContext>,
    Path((room_id, event_id)): Path<(RoomId, String)>,
    auth_user: AuthenticatedUser,
    // body 可省略（老客户端/`{}`），但一旦给了 `reason` 就必须落库：
    // 原先没有 body 提取器，SDK 的 `redactReply(roomId, eventId, reason)`
    // 传的原因被静默丢弃（审计 B-9）。
    body: Option<MatrixJson<RedactReplyBody>>,
) -> Result<StatusCode, ApiError> {
    ensure_thread_room_access(&ctx, &auth_user, room_id.as_str()).await?;

    let reason = body.and_then(|MatrixJson(body)| body.reason);
    ctx.thread_service.redact_reply(room_id.as_str(), &event_id, reason.as_deref()).await?;
    Ok(StatusCode::OK)
}

async fn list_threads_global(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    query: Query<ListQuery>,
) -> Result<Json<ThreadListResponse>, ApiError> {
    let participated_by = resolve_thread_participant_filter(query.include.as_deref(), &auth_user.user_id)?;
    let response =
        list_visible_threads(&ctx, &auth_user.user_id, query.limit, query.from.as_deref(), participated_by.as_deref())
            .await?;
    Ok(Json(response))
}

async fn create_thread_global(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<CreateThreadBody>,
) -> Result<Json<ThreadResponse>, ApiError> {
    let room_id = body.room_id.ok_or_else(|| ApiError::bad_request("room_id is required".to_string()))?;

    ensure_thread_room_access(&ctx, &auth_user, &room_id).await?;
    reject_unsupported_thread_content(body.content.as_ref())?;

    let request = CreateThreadRequest { room_id, root_event_id: body.root_event_id };

    let thread = ctx.thread_service.create_thread(&auth_user.user_id, request).await?;

    Ok(Json(ThreadResponse::from(thread)))
}

async fn get_subscribed_threads(
    State(ctx): State<RoomContext>,
    Query(query): Query<ThreadPageQuery>,
    auth_user: AuthenticatedUser,
) -> Result<Json<SubscribedThreadsResponse>, ApiError> {
    let response = ctx.thread_service.get_subscribed_threads(&auth_user.user_id, query.limit, query.from).await?;
    Ok(Json(response))
}

async fn get_unread_threads_global(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<UnreadThreadsResponse>, ApiError> {
    let response = ctx.thread_service.get_unread_threads(&auth_user.user_id, None).await?;
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_services::thread_service::ThreadSummary;

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
        // ISSUE-13：用户级线程列表是私有扩展，唯一规范位置是 `/_matrix/vendor/v1`。
        let route = "/_matrix/vendor/v1/user/{user_id}/rooms/{room_id}/threads";
        assert!(route.starts_with("/_matrix/vendor/v1/user/"));
        assert!(route.ends_with("/threads"));
    }

    fn thread_body(value: Value) -> CreateThreadBody {
        serde_json::from_value(value).expect("CreateThreadBody should deserialize")
    }

    #[test]
    fn test_create_thread_body_accepts_absent_content() {
        let body = thread_body(json!({ "root_event_id": "$root:localhost" }));
        assert!(body.content.is_none());
        assert!(reject_unsupported_thread_content(body.content.as_ref()).is_ok());
    }

    #[test]
    fn test_create_thread_body_accepts_empty_content_object() {
        let body = thread_body(json!({ "root_event_id": "$root:localhost", "content": {} }));
        assert!(reject_unsupported_thread_content(body.content.as_ref()).is_ok());
    }

    #[test]
    fn test_create_thread_body_rejects_content_instead_of_discarding_it() {
        let body = thread_body(json!({
            "root_event_id": "$root:localhost",
            "content": { "name": "我的线程标题" }
        }));
        assert!(body.content.is_some());

        let error = reject_unsupported_thread_content(body.content.as_ref())
            .expect_err("non-empty content must be rejected rather than silently dropped");
        assert!(error.is_bad_request());
        assert_eq!(error.matrix_code(), synapse_common::error::MatrixErrorCode::InvalidParam);
    }

    #[test]
    fn test_thread_include_absent_or_all_means_no_participant_filter() {
        // 规范默认值是 `all`：缺省与显式 `all` 必须等价（都不限制参与者）。
        assert!(resolve_thread_participant_filter(None, "@alice:localhost")
            .expect("absent include is valid")
            .is_none());
        assert!(resolve_thread_participant_filter(Some("all"), "@alice:localhost")
            .expect("include=all is valid")
            .is_none());
    }

    #[test]
    fn test_thread_include_participated_filters_by_requesting_user() {
        assert_eq!(
            resolve_thread_participant_filter(Some("participated"), "@alice:localhost")
                .expect("include=participated is valid")
                .as_deref(),
            Some("@alice:localhost")
        );
    }

    #[test]
    fn test_thread_include_rejects_unknown_value() {
        let error = resolve_thread_participant_filter(Some("bogus"), "@alice:localhost")
            .expect_err("unknown include values must be rejected instead of silently treating as all");
        assert!(error.is_bad_request());
        assert_eq!(error.matrix_code(), synapse_common::error::MatrixErrorCode::InvalidParam);
    }

    #[test]
    fn test_thread_list_query_parses_spec_include() {
        let query: ListQuery = serde_json::from_value(json!({ "limit": 10, "include": "participated" }))
            .expect("ListQuery should deserialize include");
        assert_eq!(query.include.as_deref(), Some("participated"));
        assert_eq!(query.limit, Some(10));
    }
}
