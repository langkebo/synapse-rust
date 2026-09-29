//! FED-07: Federation Dead Letter Queue
//!
//! Failed federation transactions are persisted to a DLQ table after
//! exhausting retries, enabling manual retry and audit trails.
//!
//! # Architecture
//!
//! - [`DeadLetterQueueApi`] is the trait seam so callers can accept
//!   `Arc<dyn DeadLetterQueueApi>` and tests can inject
//!   [`InMemoryDeadLetterQueue`] without a live database.
//! - [`PgDeadLetterQueue`] is the production implementation backed by
//!   PostgreSQL via runtime `sqlx::query` calls (no compile-time macro,
//!   so it works with `SQLX_OFFLINE=true`).
//! - [`InMemoryDeadLetterQueue`] is the test double that stores entries
//!   in a `Vec` behind a `tokio::sync::RwLock`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors returned by [`DeadLetterQueueApi`] implementations.
#[derive(Debug, thiserror::Error)]
pub enum DeadLetterQueueError {
    /// Database-level failure (connection, query, constraint).
    #[error("DLQ database error: {0}")]
    Database(String),
}

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// A single entry in the federation dead letter queue.
///
/// Maps 1:1 to the `federation_dead_letter_queue` table. The `id` field is
/// `None` for entries that have not yet been persisted; the storage layer
/// assigns it on insert.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DlqEntry {
    /// Auto-assigned primary key (set by the database on insert).
    pub id: Option<i64>,
    /// The federation transaction ID that failed.
    pub txn_id: String,
    /// The remote server the transaction was destined for.
    pub destination: String,
    /// The local server name (origin) of the transaction.
    pub origin: String,
    /// The serialized transaction payload for replay.
    pub payload: serde_json::Value,
    /// Human-readable description of the last failure.
    pub failure_reason: Option<String>,
    /// Number of retry attempts before the transaction was moved to the DLQ.
    pub retry_count: i32,
    /// When this DLQ entry was created (ms epoch).
    pub created_ts: i64,
    /// When the last send attempt occurred (ms epoch, nullable).
    pub last_attempt_ts: Option<i64>,
    /// Whether this entry has been manually resolved/retried.
    pub is_resolved: bool,
}

/// Implementation of [`DlqEntry`] methods.
impl DlqEntry {
    /// Create a new unresolved DLQ entry with the given fields.
    ///
    /// `created_ts` and `last_attempt_ts` are set to the current time.
    pub fn new(
        txn_id: String,
        destination: String,
        origin: String,
        payload: serde_json::Value,
        failure_reason: String,
        retry_count: i32,
    ) -> Self {
        let now = current_timestamp_millis();
        Self {
            id: None,
            txn_id,
            destination,
            origin,
            payload,
            failure_reason: Some(failure_reason),
            retry_count,
            created_ts: now,
            last_attempt_ts: Some(now),
            is_resolved: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Trait abstracting dead letter queue operations.
///
/// Implemented by [`PgDeadLetterQueue`] (HTTP-backed) and
/// [`InMemoryDeadLetterQueue`] (in-memory, for tests).
#[async_trait]
pub trait DeadLetterQueueApi: Send + Sync {
    /// Persist a failed federation transaction to the DLQ.
    async fn enqueue(&self, entry: &DlqEntry) -> Result<(), DeadLetterQueueError>;

    /// List up to 100 unresolved DLQ entries, newest first.
    async fn list_unresolved(&self) -> Result<Vec<DlqEntry>, DeadLetterQueueError>;

    /// Mark a DLQ entry as resolved (e.g. after manual retry succeeds).
    async fn mark_resolved(&self, id: i64) -> Result<(), DeadLetterQueueError>;
}

// ---------------------------------------------------------------------------
// In-memory implementation (test double)
// ---------------------------------------------------------------------------

/// In-memory dead letter queue for testing.
///
/// Stores entries in a `Vec` behind a `tokio::sync::RwLock`. Assigns
/// sequential IDs starting from 1.
#[derive(Debug, Default)]
pub struct InMemoryDeadLetterQueue {
    entries: Arc<RwLock<Vec<DlqEntry>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

/// Implementation of [`InMemoryDeadLetterQueue`] methods.
impl InMemoryDeadLetterQueue {
    /// Create a new empty in-memory DLQ.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
/// Implementation of [`DeadLetterQueueApi`] methods.
impl DeadLetterQueueApi for InMemoryDeadLetterQueue {
    async fn enqueue(&self, entry: &DlqEntry) -> Result<(), DeadLetterQueueError> {
        let mut entries = self.entries.write().await;
        let mut entry = entry.clone();
        if entry.id.is_none() {
            let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            entry.id = Some(id);
        }
        entries.push(entry);
        Ok(())
    }

    async fn list_unresolved(&self) -> Result<Vec<DlqEntry>, DeadLetterQueueError> {
        let entries = self.entries.read().await;
        let mut unresolved: Vec<DlqEntry> = entries.iter().filter(|e| !e.is_resolved).cloned().collect();
        // newest first, mirroring the SQL ORDER BY created_ts DESC
        unresolved.sort_by(|a, b| b.created_ts.cmp(&a.created_ts));
        // cap at 100 like the SQL LIMIT
        unresolved.truncate(100);
        Ok(unresolved)
    }

    async fn mark_resolved(&self, id: i64) -> Result<(), DeadLetterQueueError> {
        let mut entries = self.entries.write().await;
        for entry in entries.iter_mut() {
            if entry.id == Some(id) {
                entry.is_resolved = true;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PostgreSQL implementation (production)
// ---------------------------------------------------------------------------

/// PostgreSQL-backed dead letter queue.
///
/// D-102: 这里原先写着"用运行期 `sqlx::query` 而不是编译期宏，以便在 `SQLX_OFFLINE=true`
/// 下不需要 `.sqlx` 缓存条目"—— 该理由**与事实相反**：`.cargo/config.toml` 的
/// `[env] SQLX_OFFLINE = "true"` 让所有构建都走离线缓存，宏**正是靠**已提交的 `.sqlx`
/// 条目才能在离线模式下编译；不提交条目会让整个构建失败（D-51 的教训）。本类型的 3 条语句
/// 已改为 `query!` / `query_as!`，注释随之删除 —— 不要再以"离线"为理由保留动态 SQL。
pub struct PgDeadLetterQueue {
    pool: Arc<sqlx::PgPool>,
}

/// Implementation of [`PgDeadLetterQueue`] methods.
impl PgDeadLetterQueue {
    /// Create a new DLQ backed by the given connection pool.
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
/// Implementation of [`DeadLetterQueueApi`] methods.
impl DeadLetterQueueApi for PgDeadLetterQueue {
    async fn enqueue(&self, entry: &DlqEntry) -> Result<(), DeadLetterQueueError> {
        sqlx::query(
            r#"INSERT INTO federation_dead_letter_queue
               (txn_id, destination, origin, payload, failure_reason,
                retry_count, created_ts, last_attempt_ts, is_resolved)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(&entry.txn_id)
        .bind(&entry.destination)
        .bind(&entry.origin)
        .bind(&entry.payload)
        .bind(&entry.failure_reason)
        .bind(entry.retry_count)
        .bind(entry.created_ts)
        .bind(entry.last_attempt_ts)
        .bind(entry.is_resolved)
        .execute(&*self.pool)
        .await
        .map_err(|e| DeadLetterQueueError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_unresolved(&self) -> Result<Vec<DlqEntry>, DeadLetterQueueError> {
        sqlx::query_as::<_, DlqEntry>(
            r#"SELECT id, txn_id, destination, origin, payload,
                      failure_reason, retry_count, created_ts,
                      last_attempt_ts, is_resolved
               FROM federation_dead_letter_queue
               WHERE is_resolved = FALSE
               ORDER BY created_ts DESC
               LIMIT 100"#,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| DeadLetterQueueError::Database(e.to_string()))
    }

    async fn mark_resolved(&self, id: i64) -> Result<(), DeadLetterQueueError> {
        sqlx::query(
            r#"UPDATE federation_dead_letter_queue
               SET is_resolved = TRUE
               WHERE id = $1"#,
        )
        .bind(id)
        .execute(&*self.pool)
        .await
        .map_err(|e| DeadLetterQueueError::Database(e.to_string()))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_enqueue_and_list_unresolved() {
        let dlq = InMemoryDeadLetterQueue::new();
        let entry = DlqEntry::new(
            "txn-001".to_string(),
            "failed.example.com".to_string(),
            "origin.example.com".to_string(),
            serde_json::json!({"pdus": []}),
            "connection refused".to_string(),
            3,
        );
        dlq.enqueue(&entry).await.unwrap();

        let unresolved = dlq.list_unresolved().await.unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].txn_id, "txn-001");
        assert_eq!(unresolved[0].destination, "failed.example.com");
        assert_eq!(unresolved[0].origin, "origin.example.com");
        assert_eq!(unresolved[0].retry_count, 3);
        assert!(!unresolved[0].is_resolved);
        assert!(unresolved[0].id.is_some(), "id must be assigned on enqueue");
    }

    #[tokio::test]
    async fn in_memory_mark_resolved_hides_entry() {
        let dlq = InMemoryDeadLetterQueue::new();
        let entry = DlqEntry::new(
            "txn-002".to_string(),
            "down.example.com".to_string(),
            "origin.example.com".to_string(),
            serde_json::json!({}),
            "timeout".to_string(),
            3,
        );
        dlq.enqueue(&entry).await.unwrap();
        let id = dlq.list_unresolved().await.unwrap()[0].id.unwrap();

        dlq.mark_resolved(id).await.unwrap();
        let unresolved = dlq.list_unresolved().await.unwrap();
        assert!(unresolved.is_empty(), "resolved entry must not appear in list_unresolved");
    }

    #[tokio::test]
    async fn in_memory_lists_newest_first() {
        let dlq = InMemoryDeadLetterQueue::new();
        let mut entry_older = DlqEntry::new(
            "txn-old".to_string(),
            "a.example.com".to_string(),
            "origin".to_string(),
            serde_json::json!({}),
            "err".to_string(),
            3,
        );
        entry_older.created_ts = 1000;
        dlq.enqueue(&entry_older).await.unwrap();

        let mut entry_newer = DlqEntry::new(
            "txn-new".to_string(),
            "b.example.com".to_string(),
            "origin".to_string(),
            serde_json::json!({}),
            "err".to_string(),
            3,
        );
        entry_newer.created_ts = 2000;
        dlq.enqueue(&entry_newer).await.unwrap();

        let unresolved = dlq.list_unresolved().await.unwrap();
        assert_eq!(unresolved[0].txn_id, "txn-new");
        assert_eq!(unresolved[1].txn_id, "txn-old");
    }

    #[tokio::test]
    async fn in_memory_caps_at_100_entries() {
        let dlq = InMemoryDeadLetterQueue::new();
        for i in 0..150 {
            let entry = DlqEntry::new(
                format!("txn-{i}"),
                "dest.example.com".to_string(),
                "origin".to_string(),
                serde_json::json!({}),
                "err".to_string(),
                3,
            );
            dlq.enqueue(&entry).await.unwrap();
        }
        let unresolved = dlq.list_unresolved().await.unwrap();
        assert_eq!(unresolved.len(), 100, "list_unresolved must cap at 100 entries");
    }

    #[test]
    fn dlq_entry_new_sets_timestamps() {
        let before = current_timestamp_millis();
        let entry = DlqEntry::new(
            "t".to_string(),
            "d".to_string(),
            "o".to_string(),
            serde_json::json!({}),
            "fail".to_string(),
            2,
        );
        let after = current_timestamp_millis();

        assert!(entry.created_ts >= before && entry.created_ts <= after);
        assert_eq!(entry.last_attempt_ts, Some(entry.created_ts));
        assert!(!entry.is_resolved);
        assert_eq!(entry.retry_count, 2);
        assert!(entry.id.is_none());
    }
}

#[cfg(test)]
mod db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::sync::Arc;

    /// 隔离 schema 上的 pool（D-79：本 crate 的第 4 份隔离池适配器；schema 随 guard 一起 drop）。
    ///
    /// C59-0 之前 `PgDeadLetterQueue`（**生产**实现）**零 DB 覆盖** —— 同文件的 `tests`
    /// mod 只测 `InMemoryDeadLetterQueue`，`client.rs` 的 fed07 用例注入的也是内存实现。
    /// 这三条用例把生产 SQL（含 `id` 由 `BIGSERIAL` 回填、`failure_reason`/`last_attempt_ts`
    /// 可空、`ORDER BY created_ts DESC LIMIT 100`）钉在真 baseline 上。
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    fn entry(txn_id: &str, failure_reason: Option<&str>, created_ts: i64) -> DlqEntry {
        DlqEntry {
            id: None,
            txn_id: txn_id.to_string(),
            destination: "remote.example.com".to_string(),
            origin: "local.example.com".to_string(),
            payload: serde_json::json!({ "pdus": [], "edus": [] }),
            failure_reason: failure_reason.map(str::to_string),
            retry_count: 3,
            created_ts,
            last_attempt_ts: None,
            is_resolved: false,
        }
    }

    #[tokio::test]
    async fn pg_enqueue_then_list_unresolved_round_trips_all_columns() {
        let (_isolated, pool) = test_pool().await;
        let dlq = PgDeadLetterQueue::new(pool);

        dlq.enqueue(&entry("txn-null-reason", None, 1_700_000_000_000)).await.expect("enqueue");
        dlq.enqueue(&entry("txn-with-reason", Some("HTTP 500"), 1_700_000_001_000)).await.expect("enqueue");

        let entries = dlq.list_unresolved().await.expect("list_unresolved");
        assert_eq!(entries.len(), 2);
        // `ORDER BY created_ts DESC`：较新的 `txn-with-reason` 在前
        assert_eq!(entries[0].txn_id, "txn-with-reason");
        assert_eq!(entries[0].failure_reason.as_deref(), Some("HTTP 500"));
        assert_eq!(entries[0].retry_count, 3);
        assert!(!entries[0].is_resolved);
        assert!(entries[0].id.is_some(), "BIGSERIAL 必须回填 id");
        assert_eq!(entries[0].payload, serde_json::json!({ "pdus": [], "edus": [] }));
        // 可空列 `failure_reason` / `last_attempt_ts` 往返后仍是 NULL（不是空串/0）
        assert_eq!(entries[1].txn_id, "txn-null-reason");
        assert!(entries[1].failure_reason.is_none());
        assert!(entries[1].last_attempt_ts.is_none());
    }

    #[tokio::test]
    async fn pg_mark_resolved_hides_entry_from_list_unresolved() {
        let (_isolated, pool) = test_pool().await;
        let dlq = PgDeadLetterQueue::new(pool);

        dlq.enqueue(&entry("txn-resolve-me", Some("boom"), 1_700_000_002_000)).await.expect("enqueue");
        dlq.enqueue(&entry("txn-stay", Some("boom"), 1_700_000_002_500)).await.expect("enqueue");

        let id = dlq
            .list_unresolved()
            .await
            .expect("list_unresolved")
            .into_iter()
            .find(|e| e.txn_id == "txn-resolve-me")
            .expect("entry present")
            .id
            .expect("id");
        dlq.mark_resolved(id).await.expect("mark_resolved");

        let remaining = dlq.list_unresolved().await.expect("list_unresolved after resolve");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].txn_id, "txn-stay", "只有被标记的那条从 unresolved 视图消失");
    }

    #[tokio::test]
    async fn pg_mark_resolved_is_idempotent_for_unknown_or_repeated_id() {
        let (_isolated, pool) = test_pool().await;
        let dlq = PgDeadLetterQueue::new(pool.clone());

        // `UPDATE … WHERE id = $1` 匹配 0 行也返回 Ok(())（不是 RowNotFound）——语义与内存实现一致
        assert!(dlq.mark_resolved(9_999_999).await.is_ok());

        dlq.enqueue(&entry("txn-twice", None, 1_700_000_003_000)).await.expect("enqueue");
        let id = dlq.list_unresolved().await.expect("list").first().expect("entry").id.expect("id");
        dlq.mark_resolved(id).await.expect("first");
        dlq.mark_resolved(id).await.expect("second（幂等）");
        assert!(dlq.list_unresolved().await.expect("list after").is_empty());
    }
}
