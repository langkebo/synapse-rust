//! Background pruning helpers for long-running database stability.
//!
//! Several tables in the homeserver schema are append-only or accumulate
//! stale rows over time. Without periodic pruning they grow indefinitely,
//! causing disk bloat on long-running instances. The functions in this
//! module perform simple `DELETE` operations against those tables and are
//! intended to be invoked by a scheduled background task (see
//! `src/server.rs`).

use sqlx::PgPool;

/// Default retention period for device list change history (30 days).
///
/// Older entries in `device_lists_changes` are pruned to prevent the
/// append-only device list change tracking table from growing without
/// bound.
pub const DEVICE_LIST_CHANGES_RETENTION_DAYS: i64 = 30;

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

/// Prune old device list change entries.
///
/// Deletes rows from `device_lists_changes` whose `created_ts` is older
/// than `retention_days` days. Returns the number of rows deleted.
pub async fn prune_old_device_list_changes(pool: &PgPool, retention_days: i64) -> Result<u64, sqlx::Error> {
    let cutoff = chrono::Utc::now().timestamp_millis() - (retention_days * 86400 * 1000);
    let result =
        sqlx::query("DELETE FROM device_lists_changes WHERE created_ts < $1").bind(cutoff).execute(pool).await?;
    Ok(result.rows_affected())
}

/// Prune expired presence records.
///
/// Deletes rows from `presence` where `last_active_ts` is older than
/// [`PRESENCE_PRUNE_TIMEOUT_MS`]. Returns the number of rows deleted.
pub async fn prune_expired_presence(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let cutoff = chrono::Utc::now().timestamp_millis() - PRESENCE_PRUNE_TIMEOUT_MS;
    let result = sqlx::query("DELETE FROM presence WHERE last_active_ts < $1").bind(cutoff).execute(pool).await?;
    Ok(result.rows_affected())
}

/// Prune expired or used one-time keys.
///
/// Deletes rows from `one_time_keys` that have been used (`is_used = true`)
/// or are older than [`ONE_TIME_KEYS_RETENTION_DAYS`] days. Returns the
/// number of rows deleted.
pub async fn prune_expired_one_time_keys(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let cutoff = chrono::Utc::now().timestamp_millis() - (ONE_TIME_KEYS_RETENTION_DAYS * 86400 * 1000);
    let result = sqlx::query("DELETE FROM one_time_keys WHERE is_used = true OR created_ts < $1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
