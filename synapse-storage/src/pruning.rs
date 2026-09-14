//! Background pruning helpers for long-running database stability.
//!
//! Several tables in the homeserver schema are append-only or accumulate
//! stale rows over time. Without periodic pruning they grow indefinitely,
//! causing disk bloat on long-running instances. The functions in this
//! module perform simple `DELETE` operations against those tables and are
//! intended to be invoked by a scheduled background task (see
//! `src/server.rs`).

use sqlx::PgPool;
use synapse_common::current_timestamp_millis;

/// Default retention period for device list change history (30 days).
///
/// Older entries in `device_lists_changes` are pruned to prevent the
/// append-only device list change tracking table from growing without
/// bound.
pub const DEVICE_LIST_CHANGES_RETENTION_DAYS: i64 = 30;

/// Retention period for the device list sync stream (30 days).
///
/// `device_lists_stream` is the append-only stream that clients read
/// during `/sync` to get incremental device list updates. Entries older
/// than this are pruned; clients that have not synced within this window
/// will receive a full device list resync instead of a delta.
pub const DEVICE_LIST_STREAM_RETENTION_DAYS: i64 = 30;

/// Retention period for sent device list outbound pokes (7 days).
///
/// `device_lists_outbound_pokes` tracks pending federation notifications.
/// Entries where `sent_ts IS NOT NULL` have been delivered and are safe to
/// prune after this period. Unsent entries (`sent_ts IS NULL`) are never
/// pruned to avoid losing pending delivery attempts.
pub const DEVICE_LIST_OUTBOUND_POKES_RETENTION_DAYS: i64 = 7;

/// Retention period for one-time keys (7 days). Keys that have been used
/// or are older than this are pruned.
pub const ONE_TIME_KEYS_RETENTION_DAYS: i64 = 7;

/// Presence records whose `last_active_ts` is older than this threshold
/// are considered stale and pruned.
///
/// There is no explicit `presence_timeout` configuration option in the
/// homeserver config, so a conservative default is used. Records for users
/// who have not been seen for this long are removed; their presence will
/// be recomputed if they return.
pub const PRESENCE_PRUNE_TIMEOUT_MS: i64 = 7 * 24 * 60 * 60 * 1000; // 7 days

/// Retention period for to-device transaction dedup records (24 hours).
///
/// Matches `TRANSACTION_MAX_AGE_MS` in the to-device service. Entries
/// older than this are no longer useful for dedup and are pruned.
pub const TO_DEVICE_TRANSACTIONS_RETENTION_MS: i64 = 24 * 60 * 60 * 1000; // 24 hours

/// Retention period for federation queue entries (7 days).
///
/// Sent or permanently failed transactions older than this are pruned.
/// Active/retry entries are never pruned.
pub const FEDERATION_QUEUE_RETENTION_DAYS: i64 = 7;

/// Prune old device list change entries.
///
/// Deletes rows from `device_lists_changes` whose `created_ts` is older
/// than `retention_days` days. Returns the number of rows deleted.
pub async fn prune_old_device_list_changes(pool: &PgPool, retention_days: i64) -> Result<u64, sqlx::Error> {
    let cutoff = current_timestamp_millis() - (retention_days * 86400 * 1000);
    let result =
        sqlx::query("DELETE FROM device_lists_changes WHERE created_ts < $1").bind(cutoff).execute(pool).await?;
    Ok(result.rows_affected())
}

/// Prune old entries from the device list sync stream.
///
/// Deletes rows from `device_lists_stream` whose `created_ts` is older
/// than [`DEVICE_LIST_STREAM_RETENTION_DAYS`]. This is the append-only
/// stream that clients read during `/sync`; entries older than the
/// retention window are no longer needed for incremental sync.
///
/// Returns the number of rows deleted.
pub async fn prune_old_device_lists_stream(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let cutoff = current_timestamp_millis() - (DEVICE_LIST_STREAM_RETENTION_DAYS * 86400 * 1000);
    let result =
        sqlx::query("DELETE FROM device_lists_stream WHERE created_ts < $1").bind(cutoff).execute(pool).await?;
    Ok(result.rows_affected())
}

/// Prune sent device list outbound pokes.
///
/// Deletes rows from `device_lists_outbound_pokes` that have been sent
/// (`sent_ts IS NOT NULL`) and are older than
/// [`DEVICE_LIST_OUTBOUND_POKES_RETENTION_DAYS`]. Unsent entries are
/// never pruned to avoid losing pending federation deliveries.
///
/// Returns the number of rows deleted.
pub async fn prune_sent_device_lists_outbound_pokes(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let cutoff = current_timestamp_millis() - (DEVICE_LIST_OUTBOUND_POKES_RETENTION_DAYS * 86400 * 1000);
    let result = sqlx::query("DELETE FROM device_lists_outbound_pokes WHERE sent_ts IS NOT NULL AND created_ts < $1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Prune expired presence records.
///
/// Deletes rows from `presence` where `last_active_ts` is older than
/// [`PRESENCE_PRUNE_TIMEOUT_MS`]. Returns the number of rows deleted.
pub async fn prune_expired_presence(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let cutoff = current_timestamp_millis() - PRESENCE_PRUNE_TIMEOUT_MS;
    let result = sqlx::query("DELETE FROM presence WHERE last_active_ts < $1").bind(cutoff).execute(pool).await?;
    Ok(result.rows_affected())
}

/// Prune expired or used one-time keys.
///
/// Deletes rows from `one_time_keys` that have been used (`is_used = true`)
/// or are older than [`ONE_TIME_KEYS_RETENTION_DAYS`] days. Returns the
/// number of rows deleted.
pub async fn prune_expired_one_time_keys(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let cutoff = current_timestamp_millis() - (ONE_TIME_KEYS_RETENTION_DAYS * 86400 * 1000);
    let result = sqlx::query("DELETE FROM one_time_keys WHERE is_used = true OR created_ts < $1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Prune old to-device transaction dedup records.
///
/// Deletes rows from `to_device_transactions` older than
/// [`TO_DEVICE_TRANSACTIONS_RETENTION_MS`]. These records are only used
/// for short-term dedup (24h) and accumulate without bound if not pruned.
///
/// Returns the number of rows deleted.
pub async fn prune_old_to_device_transactions(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let cutoff = current_timestamp_millis() - TO_DEVICE_TRANSACTIONS_RETENTION_MS;
    let result =
        sqlx::query("DELETE FROM to_device_transactions WHERE created_ts < $1").bind(cutoff).execute(pool).await?;
    Ok(result.rows_affected())
}

/// Prune expired token blacklist entries.
///
/// Deletes rows from `token_blacklist` where `expires_at` is non-null and
/// has passed. Entries without an expiry are retained (they represent
/// permanent revocations).
///
/// Returns the number of rows deleted.
pub async fn prune_expired_token_blacklist(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let now = current_timestamp_millis();
    let result =
        sqlx::query("DELETE FROM token_blacklist WHERE expires_at IS NOT NULL AND expires_at > 0 AND expires_at < $1")
            .bind(now)
            .execute(pool)
            .await?;
    Ok(result.rows_affected())
}

/// Prune old federation queue entries.
///
/// Deletes rows from `federation_queue` that are in terminal states
/// (`sent` or `failed`) and older than [`FEDERATION_QUEUE_RETENTION_DAYS`].
/// Active/retry entries are never pruned to avoid losing pending deliveries.
///
/// Returns the number of rows deleted.
pub async fn prune_old_federation_queue(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let cutoff = current_timestamp_millis() - (FEDERATION_QUEUE_RETENTION_DAYS * 86400 * 1000);
    let result = sqlx::query("DELETE FROM federation_queue WHERE status IN ('sent', 'failed') AND created_ts < $1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_constants_are_sensible() {
        // Device list changes: 30 days
        assert_eq!(DEVICE_LIST_CHANGES_RETENTION_DAYS, 30);
        // Device list stream: 30 days (matches changes retention)
        assert_eq!(DEVICE_LIST_STREAM_RETENTION_DAYS, 30);
        // Device list outbound pokes (sent): 7 days
        assert_eq!(DEVICE_LIST_OUTBOUND_POKES_RETENTION_DAYS, 7);
        // One-time keys: 7 days
        assert_eq!(ONE_TIME_KEYS_RETENTION_DAYS, 7);
        // Presence: 7 days in milliseconds
        assert_eq!(PRESENCE_PRUNE_TIMEOUT_MS, 7 * 24 * 60 * 60 * 1000);
        // To-device transactions: 24 hours in milliseconds
        assert_eq!(TO_DEVICE_TRANSACTIONS_RETENTION_MS, 24 * 60 * 60 * 1000);
        // Federation queue: 7 days
        assert_eq!(FEDERATION_QUEUE_RETENTION_DAYS, 7);
    }

    #[test]
    fn test_device_list_stream_retention_matches_changes() {
        // The stream and changes tables serve the same sync window, so their
        // retention periods must match. If they drift, clients syncing after
        // a long absence could see dangling stream_ids that reference pruned
        // change records.
        assert_eq!(DEVICE_LIST_STREAM_RETENTION_DAYS, DEVICE_LIST_CHANGES_RETENTION_DAYS);
    }

    #[test]
    fn test_outbound_pokes_retention_is_shorter_than_stream() {
        // Outbound pokes are per-destination delivery trackers; once sent they
        // are safe to prune sooner than the sync stream because they are not
        // read by clients.
        const { assert!(DEVICE_LIST_OUTBOUND_POKES_RETENTION_DAYS < DEVICE_LIST_STREAM_RETENTION_DAYS) };
    }

    #[test]
    fn test_to_device_retention_matches_service_constant() {
        // The pruning retention must match the dedup window used by the
        // to-device service (TRANSACTION_MAX_AGE_MS = 24h). If these drift,
        // dedup records could be pruned before they expire, causing
        // duplicate message delivery.
        let service_max_age_ms: i64 = 24 * 60 * 60 * 1000;
        assert_eq!(TO_DEVICE_TRANSACTIONS_RETENTION_MS, service_max_age_ms);
    }
}

// ============================================================================
// Database-behaviour tests for the `prune_*` DELETE functions.
//
// These are the coverage gap flagged in docs/audit/AUDIT_SUMMARY_2026-09-12.md:
// the module previously only asserted retention *constants* were consistent,
// but never exercised the actual `DELETE ... WHERE ts < cutoff` behaviour that
// runs on a background schedule. A silent regression (wrong column, inverted
// comparison, over-broad WHERE) would have shipped undetected.
//
// Each test uses an isolated, empty schema (matching oidc_session_storage.rs
// convention) and creates a minimal projection of the target table holding only
// the columns the corresponding DELETE references — so we are not coupled to the
// full v11 migration graph and there are no FK→users dependencies to seed.
//
// Run: cargo test -p synapse-storage --features test-utils pruning::db_tests
// ============================================================================
#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::Row;
    use std::sync::Arc;

    /// Return an isolated empty-schema pool, or `None` (test self-skips with a
    /// warning) when the test database is unreachable — mirrors the existing
    /// DB-test convention so CI without a DB does not hard-fail.
    async fn test_pool() -> Option<Arc<PgPool>> {
        match crate::test_utils::prepare_empty_isolated_test_pool().await {
            Ok(guard) => Some(guard.pool()),
            Err(error) => {
                tracing::warn!("Skipping pruning DB test (no test DB): {error}");
                None
            }
        }
    }

    async fn count(pool: &Arc<PgPool>, table: &str) -> i64 {
        sqlx::query(&format!("SELECT COUNT(*) AS c FROM {table}"))
            .fetch_one(&**pool)
            .await
            .map(|r| r.get::<i64, _>("c"))
            .unwrap_or(-1)
    }

    /// device_lists_changes: only created_ts drives pruning.
    async fn seed_changes(pool: &Arc<PgPool>, ts: i64, n: i64) {
        sqlx::query("CREATE TABLE device_lists_changes (id SERIAL PRIMARY KEY, user_id TEXT NOT NULL, created_ts BIGINT NOT NULL)")
            .execute(&**pool).await.expect("create device_lists_changes");
        for i in 0..n {
            sqlx::query("INSERT INTO device_lists_changes (user_id, created_ts) VALUES ($1, $2)")
                .bind(format!("@u{i}:localhost"))
                .bind(ts)
                .execute(&**pool)
                .await
                .expect("seed changes");
        }
    }

    #[tokio::test]
    async fn prune_device_list_changes_respects_retention_window() {
        let Some(pool) = test_pool().await else { return };
        let now = current_timestamp_millis();
        let day_ms = 86_400_000;
        // 3 rows older than the 30-day window, 2 rows inside it.
        seed_changes(&pool, now - 40 * day_ms, 3).await;
        let cutoff_inside = now - 10 * day_ms;
        for i in 0..2 {
            sqlx::query("INSERT INTO device_lists_changes (user_id, created_ts) VALUES ($1, $2)")
                .bind(format!("@recent{i}:localhost"))
                .bind(cutoff_inside)
                .execute(&*pool)
                .await
                .unwrap();
        }

        let deleted = prune_old_device_list_changes(&pool, DEVICE_LIST_CHANGES_RETENTION_DAYS).await.unwrap();
        assert_eq!(deleted, 3, "only pre-cutoff rows deleted");
        assert_eq!(count(&pool, "device_lists_changes").await, 2);
    }

    /// device_lists_stream: same 30-day window, created_ts only.
    #[tokio::test]
    async fn prune_device_lists_stream_deletes_only_old() {
        let Some(pool) = test_pool().await else { return };
        sqlx::query("CREATE TABLE device_lists_stream (stream_id BIGSERIAL PRIMARY KEY, user_id TEXT NOT NULL, created_ts BIGINT NOT NULL)")
            .execute(&*pool).await.unwrap();
        let day_ms = 86_400_000;
        let now = current_timestamp_millis();
        sqlx::query("INSERT INTO device_lists_stream (user_id, created_ts) SELECT $1, $2 FROM generate_series(1,5)")
            .bind("@old:localhost")
            .bind(now - 31 * day_ms)
            .execute(&*pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO device_lists_stream (user_id, created_ts) SELECT $1, $2 FROM generate_series(1,4)")
            .bind("@new:localhost")
            .bind(now - 5 * day_ms)
            .execute(&*pool)
            .await
            .unwrap();

        let deleted = prune_old_device_lists_stream(&pool).await.unwrap();
        assert_eq!(deleted, 5);
        assert_eq!(count(&pool, "device_lists_stream").await, 4);
    }

    /// device_lists_outbound_pokes: unsent (sent_ts IS NULL) must NEVER be pruned.
    #[tokio::test]
    async fn prune_outbound_pokes_preserves_unsent() {
        let Some(pool) = test_pool().await else { return };
        sqlx::query("CREATE TABLE device_lists_outbound_pokes (destination TEXT, user_id TEXT, stream_id BIGINT, sent_ts BIGINT, created_ts BIGINT)")
            .execute(&*pool).await.unwrap();
        let day_ms = 86_400_000;
        let now = current_timestamp_millis();
        let old = now - 10 * day_ms;
        // 4 sent + old (prunable), 3 unsent + old (MUST survive), 1 sent + recent (survives).
        sqlx::query("INSERT INTO device_lists_outbound_pokes (destination, user_id, stream_id, sent_ts, created_ts) VALUES ('hs', 'u', 1, $1, $2), ('hs', 'v', 2, $1, $2), ('hs', 'w', 3, $1, $2), ('hs', 'x', 4, $1, $2)")
            .bind(old).bind(old).execute(&*pool).await.unwrap();
        sqlx::query("INSERT INTO device_lists_outbound_pokes (destination, user_id, stream_id, sent_ts, created_ts) VALUES ('hs', 'a', 5, NULL, $1), ('hs', 'b', 6, NULL, $1), ('hs', 'c', 7, NULL, $1)")
            .bind(old).execute(&*pool).await.unwrap();
        sqlx::query("INSERT INTO device_lists_outbound_pokes (destination, user_id, stream_id, sent_ts, created_ts) VALUES ('hs', 'd', 8, $1, $2)")
            .bind(now).bind(now).execute(&*pool).await.unwrap();

        let deleted = prune_sent_device_lists_outbound_pokes(&pool).await.unwrap();
        assert_eq!(deleted, 4, "only sent+old rows pruned");
        let remaining_unsent =
            sqlx::query("SELECT COUNT(*) AS c FROM device_lists_outbound_pokes WHERE sent_ts IS NULL")
                .fetch_one(&*pool)
                .await
                .unwrap()
                .get::<i64, _>("c");
        assert_eq!(remaining_unsent, 3, "pending deliveries must not be dropped");
    }

    /// presence: stale last_active_ts pruned; recent kept.
    #[tokio::test]
    async fn prune_presence_removes_stale() {
        let Some(pool) = test_pool().await else { return };
        sqlx::query("CREATE TABLE presence (user_id TEXT PRIMARY KEY, last_active_ts BIGINT)")
            .execute(&*pool)
            .await
            .unwrap();
        let day_ms = 86_400_000;
        let now = current_timestamp_millis();
        sqlx::query(
            "INSERT INTO presence (user_id, last_active_ts) VALUES ('stale1', $1), ('stale2', $1), ('fresh', $2)",
        )
        .bind(now - 8 * day_ms)
        .bind(now - day_ms)
        .execute(&*pool)
        .await
        .unwrap();

        let deleted = prune_expired_presence(&pool).await.unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(count(&pool, "presence").await, 1);
    }

    /// one_time_keys: pruned when is_used OR older than window (either arm).
    #[tokio::test]
    async fn prune_one_time_keys_used_or_old() {
        let Some(pool) = test_pool().await else { return };
        sqlx::query("CREATE TABLE one_time_keys (id SERIAL PRIMARY KEY, user_id TEXT, device_id TEXT, algorithm TEXT, key_id TEXT, key_data TEXT, is_used BOOLEAN, created_ts BIGINT)")
            .execute(&*pool).await.unwrap();
        let day_ms = 86_400_000;
        let now = current_timestamp_millis();
        let old = now - 8 * day_ms;
        let fresh = now - day_ms;
        // used+fresh (prune), unused+old (prune), unused+fresh (keep).
        sqlx::query("INSERT INTO one_time_keys (user_id, device_id, algorithm, key_id, key_data, is_used, created_ts) VALUES ('u','d','a','k1','x',TRUE,$1), ('u','d','a','k2','x',FALSE,$2), ('u','d','a','k3','x',FALSE,$3)")
            .bind(fresh).bind(old).bind(fresh).execute(&*pool).await.unwrap();

        let deleted = prune_expired_one_time_keys(&pool).await.unwrap();
        assert_eq!(deleted, 2, "used OR old both trigger deletion");
        assert_eq!(count(&pool, "one_time_keys").await, 1);
    }

    /// to_device_transactions: 24h window, created_ts only.
    #[tokio::test]
    async fn prune_to_device_transactions_old_only() {
        let Some(pool) = test_pool().await else { return };
        sqlx::query("CREATE TABLE to_device_transactions (id BIGSERIAL PRIMARY KEY, transaction_id TEXT, message_id TEXT, sender_user_id TEXT, sender_device_id TEXT, created_ts BIGINT)")
            .execute(&*pool).await.unwrap();
        let hr = 3_600_000;
        let now = current_timestamp_millis();
        sqlx::query("INSERT INTO to_device_transactions (sender_user_id, sender_device_id, created_ts) VALUES ('u','d',$1),('u','d',$1),('u','d',$2)")
            .bind(now - 25 * hr).bind(now - hr).execute(&*pool).await.unwrap();

        let deleted = prune_old_to_device_transactions(&pool).await.unwrap();
        assert_eq!(deleted, 2, ">24h rows pruned");
        assert_eq!(count(&pool, "to_device_transactions").await, 1);
    }

    /// token_blacklist: permanent revocations (expires_at IS NULL) are kept.
    #[tokio::test]
    async fn prune_token_blacklist_keeps_permanent() {
        let Some(pool) = test_pool().await else { return };
        sqlx::query("CREATE TABLE token_blacklist (token TEXT, expires_at BIGINT)").execute(&*pool).await.unwrap();
        let now = current_timestamp_millis();
        // expired (prune), permanent NULL (keep), zero-sentinel (keep), future (keep).
        sqlx::query("INSERT INTO token_blacklist (token, expires_at) VALUES ('expired', $1), ('perm', NULL), ('zero', 0), ('future', $2)")
            .bind(now - 1000).bind(now + 100_000).execute(&*pool).await.unwrap();

        let deleted = prune_expired_token_blacklist(&pool).await.unwrap();
        assert_eq!(deleted, 1, "only past-expiry token pruned");
        assert_eq!(count(&pool, "token_blacklist").await, 3);
    }

    /// federation_queue: terminal states (sent/failed) pruned; pending kept.
    #[tokio::test]
    async fn prune_federation_queue_terminal_only() {
        let Some(pool) = test_pool().await else { return };
        sqlx::query("CREATE TABLE federation_queue (id BIGSERIAL PRIMARY KEY, destination TEXT, status TEXT, created_ts BIGINT)")
            .execute(&*pool).await.unwrap();
        let day_ms = 86_400_000;
        let now = current_timestamp_millis();
        let old = now - 10 * day_ms;
        // sent+old & failed+old (prune), pending+old (keep, active delivery), sent+recent (keep).
        sqlx::query("INSERT INTO federation_queue (destination, status, created_ts) VALUES ('hs','sent',$1),('hs','failed',$1),('hs','pending',$1),('hs','sent',$2)")
            .bind(old).bind(now).execute(&*pool).await.unwrap();

        let deleted = prune_old_federation_queue(&pool).await.unwrap();
        assert_eq!(deleted, 2, "only terminal+old pruned; pending deliveries preserved");
        assert_eq!(count(&pool, "federation_queue").await, 2);
    }
}
