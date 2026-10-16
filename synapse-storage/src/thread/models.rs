//! Thread data models.
//!
//! All persisted-entity structs and query parameter structs for the thread
//! storage domain.

use serde::{Deserialize, Serialize};

/// The `ThreadRoot` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadRoot {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `thread_id` field.
    pub thread_id: Option<String>,
    /// The `reply_count` field.
    pub reply_count: Option<i64>,
    /// The `last_reply_event_id` field.
    pub last_reply_event_id: Option<String>,
    /// The `last_reply_sender` field.
    pub last_reply_sender: Option<String>,
    /// The `last_reply_ts` field.
    pub last_reply_ts: Option<i64>,
    /// The `participants` field.
    pub participants: Option<serde_json::Value>,
    /// The `is_fetched` field.
    pub is_fetched: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

/// The `ThreadReply` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadReply {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `in_reply_to_event_id` field.
    pub in_reply_to_event_id: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// The `is_edited` field.
    pub is_edited: bool,
    /// The `is_redacted` field.
    pub is_redacted: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `ThreadSubscription` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadSubscription {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `notification_level` field.
    pub notification_level: String,
    /// The `is_muted` field.
    pub is_muted: bool,
    /// The `is_pinned` field.
    pub is_pinned: bool,
    /// The `subscribed_ts` field.
    pub subscribed_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `ThreadReadReceipt` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadReadReceipt {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `last_read_event_id` field.
    pub last_read_event_id: Option<String>,
    /// The `last_read_ts` field.
    pub last_read_ts: i64,
    /// The `unread_count` field.
    pub unread_count: i32,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `ThreadRelation` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadRelation {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `relates_to_event_id` field.
    pub relates_to_event_id: String,
    /// The `relation_type` field.
    pub relation_type: String,
    /// The `thread_id` field.
    pub thread_id: Option<String>,
    /// The `is_falling_back` field.
    pub is_falling_back: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `ThreadSummary` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadSummary {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `root_sender` field.
    pub root_sender: String,
    /// The `root_content` field.
    pub root_content: serde_json::Value,
    /// The `root_origin_server_ts` field.
    pub root_origin_server_ts: i64,
    /// The `latest_event_id` field.
    pub latest_event_id: Option<String>,
    /// The `latest_sender` field.
    pub latest_sender: Option<String>,
    /// The `latest_content` field.
    pub latest_content: Option<serde_json::Value>,
    /// The `latest_origin_server_ts` field.
    pub latest_origin_server_ts: Option<i64>,
    /// The `reply_count` field.
    pub reply_count: i32,
    /// The `participants` field.
    pub participants: serde_json::Value,
    /// The `is_frozen` field.
    pub is_frozen: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// 搜索结果的 **keyset 游标**：`(search_relevance, sort_ts, thread_id)`。
///
/// 搜索按**相关性**排序（`search_relevance DESC, sort_ts DESC, thread_id ASC`），
/// 所以翻页必须带上**完整排序键**才能稳定续读 —— 只带 `thread_id`（像
/// `list_thread_roots` 那样）会漏掉"相关性更低但 thread_id 更大"的整段结果。
/// 与 `/search` 的 `PostgresSearchCursor { rank, origin_server_ts, event_id }` 同源。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadSearchCursor {
    /// The `search_relevance` field.
    pub search_relevance: f64,
    /// The `sort_ts` field.
    pub sort_ts: i64,
    /// The `thread_id` field.
    pub thread_id: String,
}

/// `search_threads` 的返回行：`ThreadSummary` 的全部列 + **两个排序键**列。
///
/// 为什么不直接复用 `ThreadSummary`：`query_as!` 展开成了**结构体字面量**
/// （`sqlx` 的 `quote_query_as` 按查询列名逐字段构造，不走 `FromRow`），
/// 因此投影列与结构体字段必须严格一一对应 —— 多选出来的排序键必须显式声明。
/// 两个键都**不进对外形状**（handler 只回 `ThreadSummary`），用 [`Self::split`] 拆开。
#[derive(Debug, Clone)]
pub struct ThreadSearchRow {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `root_sender` field.
    pub root_sender: String,
    /// The `root_content` field.
    pub root_content: serde_json::Value,
    /// The `root_origin_server_ts` field.
    pub root_origin_server_ts: i64,
    /// The `latest_event_id` field.
    pub latest_event_id: Option<String>,
    /// The `latest_sender` field.
    pub latest_sender: Option<String>,
    /// The `latest_content` field.
    pub latest_content: Option<serde_json::Value>,
    /// The `latest_origin_server_ts` field.
    pub latest_origin_server_ts: Option<i64>,
    /// The `reply_count` field.
    pub reply_count: i32,
    /// The `participants` field.
    pub participants: serde_json::Value,
    /// The `is_frozen` field.
    pub is_frozen: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
    /// 相关性分数 `GREATEST(ts_rank_cd(...), similarity(...))`。
    ///
    /// ⚠️ 必须在 SQL 里显式 `::double precision`：`ts_rank_cd` 与 `similarity` 实测都返回
    /// `real`（float4），若不提升类型，`query_as!` 会把它映射成 `f32`，游标精度也随之退化。
    pub search_relevance: f64,
    /// 排序时间戳 `COALESCE(最新回复 ts, 根事件 ts, 根记录 ts)`（仅排序/游标用）。
    pub sort_ts: i64,
}

impl ThreadSearchRow {
    /// 拆成（对外形状, 本行的排序键）。
    pub fn split(self) -> (ThreadSummary, ThreadSearchCursor) {
        let cursor = ThreadSearchCursor {
            search_relevance: self.search_relevance,
            sort_ts: self.sort_ts,
            thread_id: self.thread_id.clone(),
        };
        let summary = ThreadSummary {
            id: self.id,
            room_id: self.room_id,
            thread_id: self.thread_id,
            root_event_id: self.root_event_id,
            root_sender: self.root_sender,
            root_content: self.root_content,
            root_origin_server_ts: self.root_origin_server_ts,
            latest_event_id: self.latest_event_id,
            latest_sender: self.latest_sender,
            latest_content: self.latest_content,
            latest_origin_server_ts: self.latest_origin_server_ts,
            reply_count: self.reply_count,
            participants: self.participants,
            is_frozen: self.is_frozen,
            created_ts: self.created_ts,
            updated_ts: self.updated_ts,
        };
        (summary, cursor)
    }
}

/// The `ThreadStatistics` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadStatistics {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `total_replies` field.
    pub total_replies: i32,
    /// The `total_participants` field.
    pub total_participants: i32,
    /// The `total_edits` field.
    pub total_edits: i32,
    /// The `total_redactions` field.
    pub total_redactions: i32,
    /// The `first_reply_ts` field.
    pub first_reply_ts: Option<i64>,
    /// The `last_reply_ts` field.
    pub last_reply_ts: Option<i64>,
    /// The `avg_reply_time_ms` field.
    pub avg_reply_time_ms: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `CreateThreadRootParams` struct.
#[derive(Debug, Clone)]
pub struct CreateThreadRootParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `thread_id` field.
    pub thread_id: Option<String>,
}

/// The `CreateThreadReplyParams` struct.
#[derive(Debug, Clone)]
pub struct CreateThreadReplyParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `thread_id` field.
    pub thread_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `root_event_id` field.
    pub root_event_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `in_reply_to_event_id` field.
    pub in_reply_to_event_id: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
}

/// The `ThreadListParams` struct.
#[derive(Debug, Clone)]
pub struct ThreadListParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `limit` field.
    pub limit: Option<i32>,
    /// The `from` field.
    pub from: Option<String>,
    /// The `include_all` field.
    pub include_all: bool,
}

/// `search_threads` 的参数。
///
/// 与 [`ThreadListParams`] 的区别：搜索**按相关性排序**，游标必须是完整的排序键
/// （见 [`ThreadSearchCursor`]），不能像列表那样只用 `thread_id`。
#[derive(Debug, Clone)]
pub struct ThreadSearchParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `query` field.
    pub query: String,
    /// The `limit` field.
    pub limit: Option<i32>,
    /// 上一页最后一行的排序键；`None` = 第一页。
    ///
    /// 三个字段由构造方保证**要么同时给、要么同时不给**（SQL 用 `$4 IS NULL` 判"无游标"）。
    pub cursor: Option<ThreadSearchCursor>,
}

/// The `ThreadWithReplies` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadWithReplies {
    /// The `root` field.
    pub root: ThreadRoot,
    /// The `replies` field.
    pub replies: Vec<ThreadReply>,
    /// The `reply_count` field.
    pub reply_count: i32,
    /// The `participants` field.
    pub participants: Vec<String>,
}
