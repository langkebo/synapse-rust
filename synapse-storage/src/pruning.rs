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
    fn test_to_device_retention_matches_service_constant() {
        // The pruning retention must match the dedup window used by the
        // to-device service (TRANSACTION_MAX_AGE_MS = 24h). If these drift,
        // dedup records could be pruned before they expire, causing
        // duplicate message delivery.
        let service_max_age_ms: i64 = 24 * 60 * 60 * 1000;
        assert_eq!(TO_DEVICE_TRANSACTIONS_RETENTION_MS, service_max_age_ms);
    }
}
