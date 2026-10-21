use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_common::MAX_PAGINATION_LIMIT;
use synapse_storage::thread::{
    CreateThreadReplyParams, CreateThreadRootParams, ThreadListParams, ThreadSearchCursor, ThreadSearchParams,
    ThreadStoreApi,
};
pub use synapse_storage::thread::{
    ThreadReadReceipt, ThreadReply, ThreadRoot, ThreadStatistics, ThreadSubscription, ThreadSummary,
};
use tracing::{debug, info, warn};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

/// The `CreateThreadRequest` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateThreadRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
}

/// The `CreateReplyRequest` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateReplyRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `in_reply_to_event_id` field.
    pub in_reply_to_event_id: Option<String>,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
}

/// The `GetThreadRequest` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetThreadRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `include_replies` field.
    pub include_replies: bool,
    /// The `reply_limit` field.
    pub reply_limit: Option<i32>,
}

/// The `ListThreadsRequest` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListThreadsRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `limit` field.
    pub limit: Option<i32>,
    /// The `from` field.
    pub from: Option<String>,
    /// `include=participated` 时的过滤用户（`None` = `include=all`，不限制参与者）。
    ///
    /// 语义：只返回该用户参与过的话题（发过根事件或回复）。过滤必须留在 SQL 里，
    /// 否则 `limit` 会先截断再过滤，分页结果与 `total` 都会失真。
    pub participated_by: Option<String>,
}

/// The `SearchThreadsRequest` struct.
///
/// 与 `ListThreadsRequest` 的关键差别：搜索**按相关性排序**，所以 `from` 是"排序键游标"
/// （见 `encode_thread_search_cursor` / `decode_thread_search_cursor`），
/// 语义与列表的 `thread_id` 游标**不同、不可互换**。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchThreadsRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `query` field.
    pub query: String,
    /// The `limit` field.
    pub limit: Option<i32>,
    /// The `from` field.
    pub from: Option<String>,
}

/// 编码搜索分页游标：`base64url_no_pad("{search_relevance}|{sort_ts}|{thread_id}")`。
///
/// 与 `/search` 的 `encode_postgres_search_cursor` 同法（同样是"排序键三元组"、同样
/// base64url 不透明令牌）。`f64` 走 `Display` 输出 —— Rust 保证是**最短且可精确往返**的
/// 十进制形式，解析回来与写入前的位模式一致，所以 SQL 里 `search_relevance = $4` 是精确比较。
fn encode_thread_search_cursor(cursor: &ThreadSearchCursor) -> String {
    let raw = format!("{}|{}|{}", cursor.search_relevance, cursor.sort_ts, cursor.thread_id);
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

/// 解码搜索分页游标；格式不符 / 任一字段不可解析即 `None`。
///
/// ⚠️ 调用方必须把它转成 **400**，不可当成"没有游标" —— 那会静默返回第一页，
/// 而客户端以为这是第 N 页的续读（"静默返回错的一页"比报错难查得多）。
fn decode_thread_search_cursor(cursor: &str) -> Option<ThreadSearchCursor> {
    let decoded = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (relevance, rest) = decoded.split_once('|')?;
    let (sort_ts, thread_id) = rest.split_once('|')?;
    if thread_id.is_empty() {
        return None;
    }
    Some(ThreadSearchCursor {
        search_relevance: relevance.parse().ok()?,
        sort_ts: sort_ts.parse().ok()?,
        thread_id: thread_id.to_string(),
    })
}

/// The `SubscribeRequest` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubscribeRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `notification_level` field.
    pub notification_level: String,
}

/// The `MarkReadRequest` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarkReadRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
}

/// The `ThreadListResponse` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThreadListResponse {
    /// The `threads` field.
    pub threads: Vec<ThreadSummary>,
    /// The `next_batch` field.
    pub next_batch: Option<String>,
    /// The `total` field.
    pub total: i32,
}

/// The `ThreadDetailResponse` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThreadDetailResponse {
    /// The `root` field.
    pub root: ThreadRoot,
    /// The `replies` field.
    pub replies: Vec<ThreadReply>,
    /// The `reply_count` field.
    pub reply_count: i32,
    /// The `participants` field.
    pub participants: Vec<String>,
    /// The `summary` field.
    pub summary: Option<ThreadSummary>,
    /// The `user_receipt` field.
    pub user_receipt: Option<ThreadReadReceipt>,
    /// The `user_subscription` field.
    pub user_subscription: Option<ThreadSubscription>,
}

/// The `UnreadThreadsResponse` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnreadThreadsResponse {
    /// The `threads` field.
    pub threads: Vec<ThreadReadReceipt>,
    /// The `total_unread` field.
    pub total_unread: i32,
    /// The `total_threads` field.
    pub total_threads: i32,
}

/// The `SubscribedThreadsResponse` struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubscribedThreadsResponse {
    /// The `threads` field.
    pub threads: Vec<ThreadSummary>,
    /// The `subscribed` field.
    pub subscribed: Vec<ThreadSubscription>,
    /// `thread_id` keyset cursor for the next page; `None` when the page is
    /// the last one. Matches the cursor contract used by `list_threads`.
    pub next_batch: Option<String>,
}

/// The `ThreadService` struct.
#[derive(Clone)]
pub struct ThreadService {
    storage: Arc<dyn ThreadStoreApi>,
}

impl ThreadService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn ThreadStoreApi>) -> Self {
        Self { storage }
    }

    /// See [`get_thread_root`].
    pub async fn get_thread_root(&self, room_id: &str, thread_id: &str) -> Result<Option<ThreadRoot>, ApiError> {
        self.storage
            .get_thread_root(room_id, thread_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get thread root", e))
    }

    /// See [`get_thread_root_by_event`].
    pub async fn get_thread_root_by_event(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<Option<ThreadRoot>, ApiError> {
        self.storage
            .get_thread_root_by_event(room_id, event_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get thread root", e))
    }

    /// See [`get_thread_replies`].
    pub async fn get_thread_replies(
        &self,
        room_id: &str,
        thread_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadReply>, ApiError> {
        self.storage
            .get_thread_replies(room_id, thread_id, limit, from)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get replies", e))
    }

    /// See [`get_thread_replies_page`].
    ///
    /// 返回 `None` 表示 `from` 锚点在该线程内不存在，调用方应回 400 `M_INVALID_PARAM`。
    pub async fn get_thread_replies_page(
        &self,
        room_id: &str,
        thread_id: &str,
        direction: &str,
        limit: Option<i32>,
        from: Option<&str>,
    ) -> Result<Option<Vec<ThreadReply>>, ApiError> {
        self.storage
            .get_thread_replies_page(room_id, thread_id, direction, limit, from)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get replies", e))
    }

    /// See [`get_thread_participants`].
    pub async fn get_thread_participants(&self, room_id: &str, thread_id: &str) -> Result<Vec<String>, ApiError> {
        self.storage
            .get_thread_participants(room_id, thread_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get participants", e))
    }

    /// See [`create_thread`].
    pub async fn create_thread(&self, sender: &str, request: CreateThreadRequest) -> Result<ThreadRoot, ApiError> {
        info!(
            room_id = %request.room_id,
            root_event_id = %request.root_event_id,
            sender = %sender,
            "Creating new thread"
        );

        let thread_id = format!("${}", uuid::Uuid::new_v4().simple());

        let params = CreateThreadRootParams {
            room_id: request.room_id,
            root_event_id: request.root_event_id,
            sender: sender.to_string(),
            thread_id: Some(thread_id.clone()),
        };

        let thread_root = self.storage.create_thread_root(params).await.map_err(|e| {
            warn!(error = %e, "Failed to create thread root");
            ApiError::internal_with_cause("Failed to create thread", e)
        })?;

        self.storage
            .create_thread_relation(
                &thread_root.room_id,
                &thread_root.root_event_id,
                &thread_root.root_event_id,
                "m.thread",
                thread_root.thread_id.as_deref(),
                false,
            )
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to create thread relation");
                ApiError::internal_with_cause("Failed to create thread relation", e)
            })?;

        debug!(thread_id = %thread_id, "Thread created successfully");
        Ok(thread_root)
    }

    /// See [`add_reply`].
    pub async fn add_reply(&self, sender: &str, request: CreateReplyRequest) -> Result<ThreadReply, ApiError> {
        info!(
            room_id = %request.room_id,
            thread_id = %request.thread_id,
            event_id = %request.event_id,
            sender = %sender,
            "Adding reply to thread"
        );

        let thread_root = self
            .storage
            .get_thread_root(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get thread root", e))?
            .ok_or_else(|| ApiError::not_found("Thread not found"))?;

        if thread_root.is_fetched {
            return Err(ApiError::bad_request("Thread is frozen and cannot accept new replies"));
        }

        let params = CreateThreadReplyParams {
            room_id: request.room_id,
            thread_id: request.thread_id,
            event_id: request.event_id,
            root_event_id: request.root_event_id,
            sender: sender.to_string(),
            in_reply_to_event_id: request.in_reply_to_event_id,
            content: request.content,
            origin_server_ts: request.origin_server_ts,
        };

        let reply = self.storage.create_thread_reply(params).await.map_err(|e| {
            warn!(error = %e, "Failed to create thread reply");
            ApiError::internal_with_cause("Failed to create reply", e)
        })?;

        self.storage
            .create_thread_relation(
                &reply.room_id,
                &reply.event_id,
                &reply.root_event_id,
                "m.thread",
                Some(&reply.thread_id),
                false,
            )
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to create reply relation");
                ApiError::internal_with_cause("Failed to create reply relation", e)
            })?;

        debug!(event_id = %reply.event_id, "Reply added successfully");
        Ok(reply)
    }

    /// See [`get_thread`].
    pub async fn get_thread(
        &self,
        request: GetThreadRequest,
        user_id: Option<&str>,
    ) -> Result<ThreadDetailResponse, ApiError> {
        debug!(
            room_id = %request.room_id,
            thread_id = %request.thread_id,
            "Getting thread details"
        );

        let root = self
            .storage
            .get_thread_root(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get thread root", e))?
            .ok_or_else(|| ApiError::not_found("Thread not found"))?;

        let replies = if request.include_replies {
            self.storage
                .get_thread_replies(&request.room_id, &request.thread_id, request.reply_limit, None)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to get replies", e))?
        } else {
            vec![]
        };

        let reply_count = self
            .storage
            .get_reply_count(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get reply count", e))?;

        let participants = self
            .storage
            .get_thread_participants(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get participants", e))?;

        let summary = self
            .storage
            .get_thread_summary(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get summary", e))?;

        let (user_receipt, user_subscription) = if let Some(uid) = user_id {
            let receipt = self
                .storage
                .get_read_receipt(&request.room_id, &request.thread_id, uid)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to get receipt", e))?;

            let subscription = self
                .storage
                .get_thread_subscription(&request.room_id, &request.thread_id, uid)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to get subscription", e))?;

            (receipt, subscription)
        } else {
            (None, None)
        };

        Ok(ThreadDetailResponse { root, replies, reply_count, participants, summary, user_receipt, user_subscription })
    }

    /// See [`list_threads`].
    pub async fn list_threads(&self, request: ListThreadsRequest) -> Result<ThreadListResponse, ApiError> {
        debug!(
            room_id = %request.room_id,
            limit = ?request.limit,
            "Listing threads"
        );

        let params = ThreadListParams {
            room_id: request.room_id,
            limit: request.limit,
            from: request.from,
            participated_by: request.participated_by,
        };

        let roots = self
            .storage
            .list_thread_roots(params)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to list threads", e))?;

        let summaries = self.hydrate_summaries(&roots).await?;

        let next_batch = roots.last().and_then(|r| r.thread_id.clone());
        let total = summaries.len() as i32;

        Ok(ThreadListResponse { threads: summaries, next_batch, total })
    }

    /// See [`list_all_threads`].
    pub async fn list_all_threads(
        &self,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<ThreadListResponse, ApiError> {
        let roots = self
            .storage
            .list_all_thread_roots(limit, from)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to list global threads", e))?;

        let summaries = self.hydrate_summaries(&roots).await?;

        let next_batch = roots.last().and_then(|r| r.thread_id.clone());
        let total = summaries.len() as i32;

        Ok(ThreadListResponse { threads: summaries, next_batch, total })
    }

    /// 把一页 [`ThreadRoot`] 补全为 [`ThreadSummary`]：**按房间分组做批量摘要查询**
    /// （每页每个房间一条 SQL），再按 `roots` 的原顺序回填。
    ///
    /// 此前是"每个 root 调一次 `get_thread_summary`"的 N+1：`/threads` 每页最多扫
    /// 500 个 root，等于每页多出 500 次往返。批量查询缺失的行（`thread_id` 为 NULL
    /// 的 root 不参与查询）回落到 [`Self::fallback_summary`]，与逐条查询时代一致。
    async fn hydrate_summaries(&self, roots: &[ThreadRoot]) -> Result<Vec<ThreadSummary>, ApiError> {
        let mut ids_by_room: HashMap<&str, Vec<String>> = HashMap::new();
        for root in roots {
            if let Some(thread_id) = root.thread_id.as_deref() {
                ids_by_room.entry(root.room_id.as_str()).or_default().push(thread_id.to_string());
            }
        }

        let mut summaries_by_key: HashMap<(String, String), ThreadSummary> = HashMap::new();
        for (room_id, thread_ids) in ids_by_room {
            let rows = self
                .storage
                .get_thread_summaries(room_id, &thread_ids)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to get thread summaries", e))?;
            for row in rows {
                summaries_by_key.insert((row.room_id.clone(), row.thread_id.clone()), row);
            }
        }

        Ok(roots
            .iter()
            .map(|root| {
                let key = (root.room_id.clone(), root.thread_id.clone().unwrap_or_default());
                summaries_by_key.remove(&key).unwrap_or_else(|| Self::fallback_summary(root))
            })
            .collect())
    }

    /// 摘要查询缺失时的保底构造：只用 `thread_roots` 自带的列，不补 root 事件内容。
    fn fallback_summary(root: &ThreadRoot) -> ThreadSummary {
        ThreadSummary {
            id: root.id,
            room_id: root.room_id.clone(),
            thread_id: root.thread_id.clone().unwrap_or_default(),
            root_event_id: root.root_event_id.clone(),
            root_sender: root.sender.clone(),
            root_content: serde_json::json!({}),
            root_origin_server_ts: root.created_ts,
            latest_event_id: root.last_reply_event_id.clone(),
            latest_sender: root.last_reply_sender.clone(),
            latest_content: None,
            latest_origin_server_ts: root.last_reply_ts,
            reply_count: root.reply_count.unwrap_or(0) as i32,
            participants: root.participants.clone().unwrap_or(serde_json::json!([])),
            is_frozen: root.is_fetched,
            created_ts: root.created_ts,
            updated_ts: root.updated_ts.unwrap_or(root.created_ts),
        }
    }

    /// See [`subscribe`].
    pub async fn subscribe(&self, request: SubscribeRequest) -> Result<ThreadSubscription, ApiError> {
        let thread_root = self
            .storage
            .get_thread_root(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get thread root", e))?
            .ok_or_else(|| ApiError::not_found("Thread not found"))?;

        if thread_root.is_fetched {
            return Err(ApiError::bad_request("Cannot subscribe to a frozen thread"));
        }

        let valid_levels = ["all", "mentions", "none"];
        if !valid_levels.contains(&request.notification_level.as_str()) {
            return Err(ApiError::bad_request("Invalid notification level"));
        }

        self.storage
            .subscribe_to_thread(&request.room_id, &request.thread_id, &request.user_id, &request.notification_level)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to subscribe to thread");
                ApiError::internal_with_cause("Failed to subscribe", e)
            })
    }

    /// See [`unsubscribe`].
    pub async fn unsubscribe(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), ApiError> {
        self.storage.unsubscribe_from_thread(room_id, thread_id, user_id).await.map_err(|e| {
            warn!(error = %e, "Failed to unsubscribe from thread");
            ApiError::internal_with_cause("Failed to unsubscribe", e)
        })
    }

    /// See [`mute_thread`].
    pub async fn mute_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<ThreadSubscription, ApiError> {
        self.storage.mute_thread(room_id, thread_id, user_id).await.map_err(|e| {
            warn!(error = %e, "Failed to mute thread");
            ApiError::internal_with_cause("Failed to mute thread", e)
        })
    }

    /// See [`mark_read`].
    pub async fn mark_read(&self, request: MarkReadRequest) -> Result<ThreadReadReceipt, ApiError> {
        self.storage
            .update_read_receipt(
                &request.room_id,
                &request.thread_id,
                &request.user_id,
                &request.event_id,
                request.origin_server_ts,
            )
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to mark thread as read");
                ApiError::internal_with_cause("Failed to mark as read", e)
            })
    }

    /// See [`get_unread_threads`].
    pub async fn get_unread_threads(
        &self,
        user_id: &str,
        room_id: Option<&str>,
    ) -> Result<UnreadThreadsResponse, ApiError> {
        let threads = self.storage.get_threads_with_unread(user_id, room_id).await.map_err(|e| {
            warn!(error = %e, "Failed to get unread threads");
            ApiError::internal_with_cause("Failed to get unread threads", e)
        })?;

        let total_unread = threads.len() as i32;
        let total_threads = threads.len() as i32;

        Ok(UnreadThreadsResponse { threads, total_unread, total_threads })
    }

    /// See [`get_subscribed_threads`].
    pub async fn get_subscribed_threads(
        &self,
        user_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<SubscribedThreadsResponse, ApiError> {
        // Over-fetch by 1 to detect whether more pages exist. The extra row
        // is dropped before returning so callers always see a `limit`-sized
        // page (except for the final page which may be shorter).
        let fetch_limit = limit.unwrap_or(50) + 1;
        let subscriptions = self
            .storage
            .get_user_thread_subscriptions(user_id, Some(fetch_limit), from)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get subscriptions", e))?;

        let (page, has_more) = if subscriptions.len() as i32 > fetch_limit - 1 {
            let mut page = subscriptions;
            let extra = page.pop();
            debug_assert!(extra.is_some(), "over-fetch sentinel must exist when has_more is true");
            (page, true)
        } else {
            (subscriptions, false)
        };

        let next_batch = if has_more { page.last().map(|s| s.thread_id.clone()) } else { None };

        let mut ids_by_room: HashMap<&str, Vec<String>> = HashMap::new();
        for subscription in &page {
            ids_by_room.entry(subscription.room_id.as_str()).or_default().push(subscription.thread_id.clone());
        }

        let mut summaries_by_key: HashMap<(String, String), ThreadSummary> = HashMap::new();
        for (room_id, thread_ids) in ids_by_room {
            let rows = self
                .storage
                .get_thread_summaries(room_id, &thread_ids)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to get subscribed threads", e))?;
            for row in rows {
                summaries_by_key.insert((row.room_id.clone(), row.thread_id.clone()), row);
            }
        }

        // 与逐条查询语义一致：摘要缺失的订阅只保留在 `subscribed`，不进 `threads`。
        let threads = page
            .iter()
            .filter_map(|subscription| {
                summaries_by_key.remove(&(subscription.room_id.clone(), subscription.thread_id.clone()))
            })
            .collect();

        Ok(SubscribedThreadsResponse { threads, subscribed: page, next_batch })
    }

    /// See [`delete_thread`].
    pub async fn delete_thread(&self, room_id: &str, thread_id: &str) -> Result<(), ApiError> {
        self.storage.delete_thread(room_id, thread_id).await.map_err(|e| {
            warn!(error = %e, "Failed to delete thread");
            ApiError::internal_with_cause("Failed to delete thread", e)
        })
    }

    /// See [`get_thread_statistics`].
    pub async fn get_thread_statistics(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<synapse_storage::thread::ThreadStatistics>, ApiError> {
        self.storage.get_thread_statistics(room_id, thread_id).await.map_err(|e| {
            warn!(error = %e, "Failed to get thread statistics");
            ApiError::internal_with_cause("Failed to get statistics", e)
        })
    }

    /// See [`search_threads`].
    ///
    /// 返回 `ThreadListResponse`（与 `list_threads` **同形**）：`threads` 是本页、
    /// `next_batch` 是续读游标（不透明）。
    ///
    /// ⚠️ 2026-10-16 契约修复：此前返回**裸数组** `Vec<ThreadSummary>` 且不认 `from`
    /// ⇒ 对接的 SDK 两头都错（发了一个后端不认的 `from`，又按信封读裸数组 ⇒
    /// `result.threads` 恒为 `undefined`，该方法完全不可用）。
    ///
    /// 分页用 **keyset**（不是 offset）：按相关性排序的结果集用位置分页会在"翻页途中
    /// 有新回复"时跳行/重复，而 keyset 只依赖上一页最后一行的排序键。取 `limit + 1` 行判
    /// `has_more`（与 `/search`、`get_subscribed_threads` 同法），避免"恰好取满"时多发一个
    /// 无用的 `next_batch`。
    pub async fn search_threads(&self, request: SearchThreadsRequest) -> Result<ThreadListResponse, ApiError> {
        // 页长夹在 [1, MAX_PAGINATION_LIMIT]：上界是必须的 —— 下面要取 `limit + 1` 判 `has_more`，
        // 不设上界时极大 limit 既撑爆内存、又让 `i32` 加法有机会溢出。
        let limit = request.limit.unwrap_or(20).clamp(1, MAX_PAGINATION_LIMIT as i32);

        let cursor = match request.from.as_deref() {
            None => None,
            // 显式 400（不降级成"第一页"）：静默返回错的一页比报错难查得多。
            Some(token) => Some(
                decode_thread_search_cursor(token).ok_or_else(|| ApiError::bad_request("Invalid pagination cursor"))?,
            ),
        };

        let mut rows = self
            .storage
            .search_threads(ThreadSearchParams {
                room_id: request.room_id,
                query: request.query,
                limit: Some(limit + 1),
                cursor,
            })
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to search threads");
                ApiError::internal_with_cause("Failed to search threads", e)
            })?;

        let has_more = rows.len() as i32 > limit;
        if has_more {
            rows.truncate(limit as usize);
        }

        // `next_batch` 取**本页最后一行**的排序键；没有下一页时必须是 `None`
        // （客户端以它的有无判定是否继续翻页）。
        let next_batch = if has_more {
            rows.last().map(|row| {
                encode_thread_search_cursor(&ThreadSearchCursor {
                    search_relevance: row.search_relevance,
                    sort_ts: row.sort_ts,
                    thread_id: row.thread_id.clone(),
                })
            })
        } else {
            None
        };

        let threads: Vec<ThreadSummary> = rows.into_iter().map(|row| row.split().0).collect();
        // 与同族 `list_threads` 口径一致：`total` 是**本页条数**，不是全量命中数
        // （全量计数要额外一次 `COUNT(*)`，本族端点都不做）。
        let total = threads.len() as i32;

        Ok(ThreadListResponse { threads, next_batch, total })
    }

    /// See [`freeze_thread`].
    pub async fn freeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), ApiError> {
        self.storage.freeze_thread(room_id, thread_id).await.map_err(|e| {
            warn!(error = %e, "Failed to freeze thread");
            ApiError::internal_with_cause("Failed to freeze thread", e)
        })
    }

    /// See [`unfreeze_thread`].
    pub async fn unfreeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), ApiError> {
        self.storage.unfreeze_thread(room_id, thread_id).await.map_err(|e| {
            warn!(error = %e, "Failed to unfreeze thread");
            ApiError::internal_with_cause("Failed to unfreeze thread", e)
        })
    }

    /// See [`redact_reply`].
    pub async fn redact_reply(&self, room_id: &str, event_id: &str, reason: Option<&str>) -> Result<(), ApiError> {
        self.storage.mark_reply_redacted(room_id, event_id, reason).await.map_err(|e| {
            warn!(error = %e, "Failed to redact reply");
            ApiError::internal_with_cause("Failed to redact reply", e)
        })
    }

    /// See [`edit_reply`].
    pub async fn edit_reply(&self, room_id: &str, event_id: &str) -> Result<(), ApiError> {
        self.storage.mark_reply_edited(room_id, event_id).await.map_err(|e| {
            warn!(error = %e, "Failed to edit reply");
            ApiError::internal_with_cause("Failed to edit reply", e)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    #[test]
    fn test_create_thread_request() {
        let request = super::CreateThreadRequest {
            room_id: "!room:example.com".to_string(),
            root_event_id: "$event:example.com".to_string(),
        };
        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.root_event_id, "$event:example.com");
    }

    #[test]
    fn test_create_reply_request() {
        let request = super::CreateReplyRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            event_id: "$reply:example.com".to_string(),
            root_event_id: "$root:example.com".to_string(),
            content: serde_json::json!({"msgtype": "m.text", "body": "Reply"}),
            in_reply_to_event_id: Some("$prev:example.com".to_string()),
            origin_server_ts: 1234567890,
        };
        assert_eq!(request.thread_id, "$thread:example.com");
        assert!(request.in_reply_to_event_id.is_some());
    }

    #[test]
    fn test_get_thread_request() {
        let request = super::GetThreadRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            include_replies: true,
            reply_limit: Some(100),
        };
        assert!(request.include_replies);
        assert_eq!(request.reply_limit, Some(100));
    }

    #[test]
    fn test_list_threads_request() {
        let request = super::ListThreadsRequest {
            room_id: "!room:example.com".to_string(),
            limit: Some(50),
            from: Some("batch_token".to_string()),
            participated_by: Some("@alice:example.com".to_string()),
        };
        assert_eq!(request.limit, Some(50));
        assert_eq!(request.participated_by.as_deref(), Some("@alice:example.com"));
    }

    #[test]
    fn test_subscribe_request() {
        let request = super::SubscribeRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            notification_level: "all".to_string(),
        };
        assert_eq!(request.notification_level, "all");
    }

    #[test]
    fn test_mark_read_request() {
        let request = super::MarkReadRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_id: "$event:example.com".to_string(),
            origin_server_ts: 1234567890,
        };
        assert_eq!(request.user_id, "@user:example.com");
    }

    #[test]
    fn test_thread_list_response_serialization() {
        let response = super::ThreadListResponse { threads: vec![], next_batch: Some("token".to_string()), total: 0 };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("threads"));
        assert!(json.contains("total"));
    }

    #[test]
    fn test_unread_threads_response() {
        let response = super::UnreadThreadsResponse { threads: vec![], total_unread: 5, total_threads: 10 };
        assert_eq!(response.total_unread, 5);
        assert_eq!(response.total_threads, 10);
    }

    #[test]
    fn test_notification_levels() {
        let valid_levels = ["all", "mentions", "none"];
        for level in valid_levels {
            assert!(matches!(level, "all" | "mentions" | "none"));
        }
    }

    #[test]
    fn test_thread_root_structure() {
        let root = synapse_storage::thread::ThreadRoot {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: Some("$thread:example.com".to_string()),
            root_event_id: "$root:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            reply_count: Some(5),
            last_reply_event_id: Some("$last:example.com".to_string()),
            last_reply_sender: Some("@replier:example.com".to_string()),
            last_reply_ts: Some(1234567890),
            participants: Some(serde_json::json!(["@user:example.com"])),
            is_fetched: false,
            created_ts: 1234567800,
            updated_ts: Some(1234567890),
        };
        assert_eq!(root.reply_count, Some(5));
        assert!(!root.is_fetched);
    }

    #[test]
    fn test_thread_reply_structure() {
        let reply = synapse_storage::thread::ThreadReply {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            event_id: "$reply:example.com".to_string(),
            root_event_id: "$root:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            in_reply_to_event_id: Some("$prev:example.com".to_string()),
            content: serde_json::json!({"msgtype": "m.text"}),
            origin_server_ts: 1234567890,
            is_redacted: false,
            is_edited: false,
            redaction_reason: None,
            created_ts: 1234567890,
        };
        assert!(!reply.is_redacted);
        assert!(!reply.is_edited);
    }

    #[test]
    fn test_thread_subscription_structure() {
        let subscription = synapse_storage::thread::ThreadSubscription {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            notification_level: "all".to_string(),
            is_muted: false,
            is_pinned: false,
            subscribed_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert_eq!(subscription.notification_level, "all");
        assert!(!subscription.is_muted);
    }

    #[test]
    fn test_thread_read_receipt_structure() {
        let receipt = synapse_storage::thread::ThreadReadReceipt {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            last_read_event_id: Some("$event:example.com".to_string()),
            last_read_ts: 1234567890,
            unread_count: 3,
            updated_ts: 1234567890,
        };
        assert_eq!(receipt.unread_count, 3);
    }

    // -- P1 #19634: frozen event behaviour (service layer) --
    //
    // The `ThreadService::freeze_thread` / `unfreeze_thread` operations must
    // actually gate `add_reply` and `subscribe`: a frozen thread rejects new
    // replies and new subscriptions, and unfreezing restores both. These are
    // behavioural contracts that pure-structure tests above do not cover, and
    // they break silently if the storage layer's freeze flag does not
    // propagate onto `ThreadRoot.is_fetched` (the field the service reads).

    fn make_service_with_thread() -> (super::ThreadService, String, String) {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime for thread service test");
        let storage: Arc<dyn synapse_storage::thread::ThreadStoreApi> =
            Arc::new(synapse_storage::test_mocks::InMemoryThreadStore::new());
        let service = super::ThreadService::new(storage);

        let room_id = "!room:example.com".to_string();
        let request =
            super::CreateThreadRequest { room_id: room_id.clone(), root_event_id: "$root:example.com".to_string() };
        let root = rt.block_on(service.create_thread("@creator:example.com", request)).expect("create thread");
        let thread_id = root.thread_id.expect("thread_id assigned at creation");
        (service, room_id, thread_id)
    }

    fn make_reply_request(room_id: &str, thread_id: &str) -> super::CreateReplyRequest {
        super::CreateReplyRequest {
            room_id: room_id.to_string(),
            thread_id: thread_id.to_string(),
            event_id: "$reply:example.com".to_string(),
            root_event_id: "$root:example.com".to_string(),
            content: serde_json::json!({"msgtype": "m.text", "body": "reply"}),
            in_reply_to_event_id: None,
            origin_server_ts: 1_700_000_000_000,
        }
    }

    #[test]
    fn freeze_thread_blocks_new_replies() {
        let (service, room_id, thread_id) = make_service_with_thread();
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

        rt.block_on(service.freeze_thread(&room_id, &thread_id)).expect("freeze should succeed");

        let err = rt
            .block_on(service.add_reply("@replier:example.com", make_reply_request(&room_id, &thread_id)))
            .expect_err("add_reply must be rejected on a frozen thread");
        assert!(
            err.is_bad_request(),
            "expected BadRequest from add_reply on frozen thread, got kind={:?} message={}",
            err.kind,
            err.message
        );
    }

    #[test]
    fn unfreeze_thread_restores_replies() {
        let (service, room_id, thread_id) = make_service_with_thread();
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

        rt.block_on(service.freeze_thread(&room_id, &thread_id)).expect("freeze should succeed");
        rt.block_on(service.unfreeze_thread(&room_id, &thread_id)).expect("unfreeze should succeed");

        let reply = rt
            .block_on(service.add_reply("@replier:example.com", make_reply_request(&room_id, &thread_id)))
            .expect("add_reply must succeed after unfreeze");
        assert_eq!(reply.thread_id, thread_id);
    }

    #[test]
    fn freeze_thread_blocks_new_subscriptions() {
        let (service, room_id, thread_id) = make_service_with_thread();
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

        rt.block_on(service.freeze_thread(&room_id, &thread_id)).expect("freeze should succeed");

        let err = rt
            .block_on(service.subscribe(super::SubscribeRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                user_id: "@subscriber:example.com".to_string(),
                notification_level: "all".to_string(),
            }))
            .expect_err("subscribe must be rejected on a frozen thread");
        assert!(
            err.is_bad_request(),
            "expected BadRequest from subscribe on frozen thread, got kind={:?} message={}",
            err.kind,
            err.message
        );
    }

    #[test]
    fn unfreeze_thread_restores_subscriptions() {
        let (service, room_id, thread_id) = make_service_with_thread();
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

        rt.block_on(service.freeze_thread(&room_id, &thread_id)).expect("freeze should succeed");
        rt.block_on(service.unfreeze_thread(&room_id, &thread_id)).expect("unfreeze should succeed");

        let sub = rt
            .block_on(service.subscribe(super::SubscribeRequest {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                user_id: "@subscriber:example.com".to_string(),
                notification_level: "all".to_string(),
            }))
            .expect("subscribe must succeed after unfreeze");
        assert_eq!(sub.notification_level, "all");
    }

    // -- MSC4155/MSC4156: get_subscribed_threads pagination --

    fn make_paginated_service() -> (super::ThreadService, Arc<synapse_storage::test_mocks::InMemoryThreadStore>) {
        let store = Arc::new(synapse_storage::test_mocks::InMemoryThreadStore::new());
        let storage: Arc<dyn synapse_storage::thread::ThreadStoreApi> = store.clone();
        let service = super::ThreadService::new(storage);
        (service, store)
    }

    /// Seed `count` subscriptions directly into the mock store.
    /// Thread IDs are predictable: `"$t00"`, `"$t01"`, ... `"$t{count-1:02}"`.
    /// The mock's `create_thread_root` also seeds the thread summary automatically.
    async fn seed_subscriptions(store: &synapse_storage::test_mocks::InMemoryThreadStore, count: usize) {
        use synapse_storage::ThreadStoreApi;
        for i in 0..count {
            let tid = format!("$t{i:02}");
            let root = synapse_storage::thread::CreateThreadRootParams {
                room_id: "!room:example.com".to_string(),
                root_event_id: format!("{tid}:root"),
                sender: "@alice:example.com".to_string(),
                thread_id: Some(tid.clone()),
            };
            store.create_thread_root(root).await.expect("create thread root");
            store.subscribe_to_thread("!room:example.com", &tid, "@alice:example.com", "all").await.expect("subscribe");
        }
    }

    #[tokio::test]
    async fn get_subscribed_threads_pagination_emits_next_batch_when_more_pages() {
        let (service, store) = make_paginated_service();
        seed_subscriptions(&store, 5).await; // $t00, $t01, $t02, $t03, $t04

        let page1 = service.get_subscribed_threads("@alice:example.com", Some(2), None).await.expect("page 1");
        assert_eq!(page1.subscribed.len(), 2);
        assert_eq!(page1.subscribed[0].thread_id, "$t00");
        assert_eq!(page1.subscribed[1].thread_id, "$t01");
        assert_eq!(page1.next_batch.as_deref(), Some("$t01"));

        let page2 = service
            .get_subscribed_threads("@alice:example.com", Some(2), Some("$t01".to_string()))
            .await
            .expect("page 2");
        assert_eq!(page2.subscribed.len(), 2);
        assert_eq!(page2.subscribed[0].thread_id, "$t02");
        assert_eq!(page2.subscribed[1].thread_id, "$t03");
        assert_eq!(page2.next_batch.as_deref(), Some("$t03"));
    }

    #[tokio::test]
    async fn get_subscribed_threads_pagination_no_next_batch_on_last_page() {
        let (service, store) = make_paginated_service();
        seed_subscriptions(&store, 3).await;

        let page1 = service.get_subscribed_threads("@alice:example.com", Some(2), None).await.expect("page 1");
        assert_eq!(page1.subscribed.len(), 2);
        assert_eq!(page1.next_batch.as_deref(), Some("$t01"));

        let page2 = service
            .get_subscribed_threads("@alice:example.com", Some(2), Some("$t01".to_string()))
            .await
            .expect("page 2");
        assert_eq!(page2.subscribed.len(), 1);
        assert_eq!(page2.subscribed[0].thread_id, "$t02");
        assert!(page2.next_batch.is_none());
    }

    #[tokio::test]
    async fn get_subscribed_threads_pagination_respects_from_cursor() {
        let (service, store) = make_paginated_service();
        seed_subscriptions(&store, 4).await; // $t00..$t03

        let page = service
            .get_subscribed_threads("@alice:example.com", Some(10), Some("$t00".to_string()))
            .await
            .expect("page");
        assert_eq!(page.subscribed.len(), 3, "from=$t00 must skip $t00");
        assert_eq!(page.subscribed[0].thread_id, "$t01");
        assert_eq!(page.subscribed[1].thread_id, "$t02");
        assert_eq!(page.subscribed[2].thread_id, "$t03");
        assert!(page.next_batch.is_none());
    }

    #[tokio::test]
    async fn get_subscribed_threads_default_limit_when_omitted() {
        let (service, store) = make_paginated_service();
        seed_subscriptions(&store, 55).await;

        let page = service.get_subscribed_threads("@alice:example.com", None, None).await.expect("page");
        assert_eq!(page.subscribed.len(), 50, "default limit is 50");
        assert!(page.next_batch.is_some());

        let last_id = page.subscribed.last().expect("non-empty").thread_id.clone();
        let page2 =
            service.get_subscribed_threads("@alice:example.com", Some(50), Some(last_id)).await.expect("page 2");
        assert_eq!(page2.subscribed.len(), 5);
        assert!(page2.next_batch.is_none());
    }

    // ---- 搜索分页游标的编解码 ----

    #[test]
    fn thread_search_cursor_roundtrip_is_bit_exact() {
        let cursor = super::ThreadSearchCursor {
            search_relevance: 0.060_792_71_f64,
            sort_ts: 1_700_000_000_123,
            thread_id: "$root:example.com".to_string(),
        };

        let decoded = super::decode_thread_search_cursor(&super::encode_thread_search_cursor(&cursor))
            .expect("roundtrip must decode");

        assert_eq!(decoded, cursor);
        // ⚠️ 承重断言：这个 f64 会原样进 SQL 的 `search_relevance = $4`（精确比较），
        // 所以必须**逐位**往返，不能用近似相等来判。
        assert_eq!(
            decoded.search_relevance.to_bits(),
            cursor.search_relevance.to_bits(),
            "f64 经 Display → parse 必须逐位往返"
        );
    }

    #[test]
    fn thread_search_cursor_keeps_thread_id_containing_separator() {
        // thread_id 若含 `|`：只按**前两个**分隔符切分，剩余整段仍是 thread_id（不截断）。
        let cursor =
            super::ThreadSearchCursor { search_relevance: 1.0, sort_ts: 7, thread_id: "$weird|id:x".to_string() };

        let decoded =
            super::decode_thread_search_cursor(&super::encode_thread_search_cursor(&cursor)).expect("must decode");

        assert_eq!(decoded.thread_id, "$weird|id:x");
    }

    #[test]
    fn thread_search_cursor_rejects_malformed_payloads() {
        use base64::Engine as _;
        let enc = |raw: &[u8]| super::URL_SAFE_NO_PAD.encode(raw);

        let cases = [
            ("不是 base64", "not-base64!!".to_string()),
            ("没有分隔符", enc(b"1700000000123")),
            ("thread_id 为空", enc(b"0.5|1700000000123|")),
            ("相关性不是数字", enc(b"abc|1700000000123|$t:x")),
            ("时间戳不是数字", enc(b"0.5|abc|$t:x")),
        ];

        for (label, token) in cases {
            assert!(
                super::decode_thread_search_cursor(&token).is_none(),
                "{label}：必须解码失败 —— 否则会被当成有效游标，静默返回错误的一页"
            );
        }
    }
}
