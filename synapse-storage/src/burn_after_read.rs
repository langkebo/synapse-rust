use async_trait::async_trait;
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// The `BurnSettingsRow` struct.
#[derive(Debug, Clone, FromRow)]
pub struct BurnSettingsRow {
    /// The `user_id` field.
    pub user_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `burn_after_ms` field.
    pub burn_after_ms: i64,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

/// The `BurnPendingRow` struct.
#[derive(Debug, Clone, FromRow)]
pub struct BurnPendingRow {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `delete_ts` field.
    pub delete_ts: i64,
    /// The `is_processed` field.
    pub is_processed: bool,
    /// Number of times this row was attempted (including the current pass).
    /// Incremented by the processor when redact+create succeed but mark_processed fails.
    /// After exceeding the dead-letter threshold, the row is moved to dead-letter state.
    pub retry_count: i32,
    /// Human-readable description of the last error encountered during processing.
    pub last_error: Option<String>,
    /// When TRUE, the scanner skips this row entirely. Set when retry_count >= MAX_RETRY.
    pub is_dead_letter: bool,
}

/// The `BurnLogRow` struct.
#[derive(Debug, Clone, FromRow)]
pub struct BurnLogRow {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `burned_ts` field.
    pub burned_ts: i64,
}

/// The `BurnUserDefaultsRow` struct.
#[derive(Debug, Clone, FromRow)]
pub struct BurnUserDefaultsRow {
    /// The `user_id` field.
    pub user_id: String,
    /// The `default_burn_ms` field.
    pub default_burn_ms: i64,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

/// The `BurnStatsRow` struct.
#[derive(Debug, Clone, FromRow)]
pub struct BurnStatsRow {
    /// The `total_burned` field.
    pub total_burned: i64,
    /// The `total_pending` field.
    pub total_pending: i64,
    /// The `rooms_enabled` field.
    pub rooms_enabled: i64,
}

/// The `BurnAfterReadStorage` struct.
#[derive(Clone)]
pub struct BurnAfterReadStorage {
    pool: Arc<PgPool>,
}

/// Storage-agnostic API for burn-after-read persistence.
///
/// Implemented by [`BurnAfterReadStorage`] (Postgres) and
/// [`crate::test_mocks::InMemoryBurnAfterReadStore`] (in-memory).
#[async_trait]
pub trait BurnAfterReadStoreApi: Send + Sync {
    /// See [`get_settings`].
    async fn get_settings(&self, user_id: &str, room_id: &str) -> Result<Option<BurnSettingsRow>, sqlx::Error>;
    /// See [`set_settings`].
    async fn set_settings(
        &self,
        user_id: &str,
        room_id: &str,
        is_enabled: bool,
        burn_after_ms: i64,
    ) -> Result<BurnSettingsRow, sqlx::Error>;
    /// See [`schedule_burn`].
    async fn schedule_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        delete_ts: i64,
    ) -> Result<BurnPendingRow, sqlx::Error>;
    /// See [`cancel_burn`].
    async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> Result<(), sqlx::Error>;
    /// See [`get_pending_burns`].
    async fn get_pending_burns(&self, user_id: &str, room_id: &str) -> Result<Vec<BurnPendingRow>, sqlx::Error>;
    /// See [`get_expired_burns`].
    /// Excludes dead-letter rows (`is_dead_letter = TRUE`).
    async fn get_expired_burns(&self, now_ms: i64) -> Result<Vec<BurnPendingRow>, sqlx::Error>;
    /// See [`mark_burn_processed`].
    async fn mark_burn_processed(&self, id: i64) -> Result<(), sqlx::Error>;
    /// Atomically mark multiple burn records as processed in a single query.
    /// Succeeds if at least one row was updated; fails only on DB errors.
    async fn mark_burn_processed_batch(&self, ids: &[i64]) -> Result<(), sqlx::Error>;
    /// Increment retry_count and set last_error for a list of burn IDs.
    /// Called when redact+create succeed but mark_processed fails (partial failure).
    async fn increment_retry_count(&self, ids: &[i64], last_error: &str) -> Result<(), sqlx::Error>;
    /// Mark a list of burn IDs as dead letters (is_dead_letter = TRUE).
    /// Called when retry_count >= MAX_RETRY.
    async fn mark_dead_letter(&self, ids: &[i64]) -> Result<(), sqlx::Error>;
    /// See [`log_burned_event`].
    async fn log_burned_event(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        burned_ts: i64,
    ) -> Result<(), sqlx::Error>;
    /// Batch-insert burned event log entries. Uses ON CONFLICT DO NOTHING so
    /// retries are safe even when some rows were already inserted.
    async fn log_burned_event_batch(&self, entries: &[(String, String, String, i64)]) -> Result<(), sqlx::Error>;
    /// See [`get_user_stats`].
    async fn get_user_stats(&self, user_id: &str) -> Result<BurnStatsRow, sqlx::Error>;
    /// See [`get_user_default`].
    async fn get_user_default(&self, user_id: &str) -> Result<Option<BurnUserDefaultsRow>, sqlx::Error>;
    /// See [`set_user_default`].
    async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> Result<(), sqlx::Error>;
}

impl BurnAfterReadStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`get_settings`].
    pub async fn get_settings(&self, user_id: &str, room_id: &str) -> Result<Option<BurnSettingsRow>, sqlx::Error> {
        sqlx::query_as::<_, BurnSettingsRow>(
            r"
            SELECT user_id, room_id, is_enabled, burn_after_ms, created_ts, updated_ts
            FROM burn_after_read_settings
            WHERE user_id = $1 AND room_id = $2
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`set_settings`].
    pub async fn set_settings(
        &self,
        user_id: &str,
        room_id: &str,
        is_enabled: bool,
        burn_after_ms: i64,
    ) -> Result<BurnSettingsRow, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as::<_, BurnSettingsRow>(
            r"
            INSERT INTO burn_after_read_settings (user_id, room_id, is_enabled, burn_after_ms, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (user_id, room_id) DO UPDATE SET
                is_enabled = EXCLUDED.is_enabled,
                burn_after_ms = EXCLUDED.burn_after_ms,
                updated_ts = EXCLUDED.updated_ts
            RETURNING user_id, room_id, is_enabled, burn_after_ms, created_ts, updated_ts
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(is_enabled)
        .bind(burn_after_ms)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`schedule_burn`].
    pub async fn schedule_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        delete_ts: i64,
    ) -> Result<BurnPendingRow, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as::<_, BurnPendingRow>(
            r"
            INSERT INTO burn_after_read_pending (user_id, room_id, event_id, created_ts, delete_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, room_id, event_id) DO UPDATE SET
                delete_ts = EXCLUDED.delete_ts,
                created_ts = EXCLUDED.created_ts
            RETURNING id, user_id, room_id, event_id, created_ts, delete_ts, is_processed,
                      retry_count, last_error, is_dead_letter
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(event_id)
        .bind(now)
        .bind(delete_ts)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`cancel_burn`].
    pub async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE burn_after_read_pending
            SET is_processed = TRUE
            WHERE user_id = $1 AND room_id = $2 AND event_id = $3 AND is_processed = FALSE
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`get_pending_burns`].
    pub async fn get_pending_burns(&self, user_id: &str, room_id: &str) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BurnPendingRow>(
            r"
            SELECT id, user_id, room_id, event_id, created_ts, delete_ts, is_processed,
                   retry_count, last_error, is_dead_letter
            FROM burn_after_read_pending
            WHERE user_id = $1 AND room_id = $2 AND is_processed = FALSE
            ORDER BY delete_ts ASC
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_expired_burns`].
    /// Excludes dead-letter rows to bound retry storms on persistently-failing rows.
    pub async fn get_expired_burns(&self, now_ms: i64) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BurnPendingRow>(
            r"
            SELECT id, user_id, room_id, event_id, created_ts, delete_ts, is_processed,
                   retry_count, last_error, is_dead_letter
            FROM burn_after_read_pending
            WHERE delete_ts <= $1
              AND is_processed = FALSE
              AND is_dead_letter = FALSE
            ORDER BY delete_ts ASC
            ",
        )
        .bind(now_ms)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`mark_burn_processed`].
    pub async fn mark_burn_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE burn_after_read_pending SET is_processed = TRUE WHERE id = $1 AND is_processed = FALSE")
            .bind(id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    /// See [`mark_burn_processed_batch`].
    pub async fn mark_burn_processed_batch(&self, ids: &[i64]) -> Result<(), sqlx::Error> {
        if ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "UPDATE burn_after_read_pending SET is_processed = TRUE WHERE id = ANY($1) AND is_processed = FALSE",
        )
        .bind(ids)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Increment retry_count by 1 and set last_error for a list of burn IDs.
    /// Used when the redact+create steps succeed but a subsequent step (mark_processed
    /// or log_burned) fails: the row is left in unprocessed state for the next sweep
    /// and we want to count how many times we've already retried it.
    pub async fn increment_retry_count(&self, ids: &[i64], last_error: &str) -> Result<(), sqlx::Error> {
        if ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r"
            UPDATE burn_after_read_pending
            SET retry_count = retry_count + 1,
                last_error = $2
            WHERE id = ANY($1) AND is_processed = FALSE
            ",
        )
        .bind(ids)
        .bind(last_error)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Mark a list of burn IDs as dead letters. The scanner will exclude these rows.
    /// Called when retry_count has reached the maximum allowed threshold.
    pub async fn mark_dead_letter(&self, ids: &[i64]) -> Result<(), sqlx::Error> {
        if ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r"
            UPDATE burn_after_read_pending
            SET is_dead_letter = TRUE,
                last_error = COALESCE(last_error, '') || ' [moved to dead-letter]'
            WHERE id = ANY($1) AND is_processed = FALSE
            ",
        )
        .bind(ids)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`log_burned_event`].
    pub async fn log_burned_event(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        burned_ts: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO burn_after_read_log (user_id, room_id, event_id, burned_ts)
            VALUES ($1, $2, $3, $4)
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(event_id)
        .bind(burned_ts)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`log_burned_event_batch`].
    pub async fn log_burned_event_batch(&self, entries: &[(String, String, String, i64)]) -> Result<(), sqlx::Error> {
        if entries.is_empty() {
            return Ok(());
        }
        // Build (user_id, room_id, event_id, burned_ts) tuples for UNNEST.
        let user_ids: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
        let room_ids: Vec<&str> = entries.iter().map(|e| e.1.as_str()).collect();
        let event_ids: Vec<&str> = entries.iter().map(|e| e.2.as_str()).collect();
        let burned_ts: Vec<i64> = entries.iter().map(|e| e.3).collect();
        sqlx::query(
            r"
            INSERT INTO burn_after_read_log (user_id, room_id, event_id, burned_ts)
            SELECT u, r, e, t
            FROM UNNEST($1::text[], $2::text[], $3::text[], $4::bigint[]) AS x(u, r, e, t)
            ON CONFLICT (user_id, event_id) DO NOTHING
            ",
        )
        .bind(&user_ids)
        .bind(&room_ids)
        .bind(&event_ids)
        .bind(&burned_ts)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`get_user_stats`].
    pub async fn get_user_stats(&self, user_id: &str) -> Result<BurnStatsRow, sqlx::Error> {
        let row = sqlx::query_as::<_, BurnStatsRow>(
            r"
            SELECT
                COALESCE((SELECT COUNT(*) FROM burn_after_read_log WHERE user_id = $1), 0) AS total_burned,
                COALESCE((SELECT COUNT(*) FROM burn_after_read_pending WHERE user_id = $1 AND is_processed = FALSE), 0) AS total_pending,
                COALESCE((SELECT COUNT(*) FROM burn_after_read_settings WHERE user_id = $1 AND is_enabled = TRUE), 0) AS rooms_enabled
            ",
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_user_default`].
    pub async fn get_user_default(&self, user_id: &str) -> Result<Option<BurnUserDefaultsRow>, sqlx::Error> {
        let row = sqlx::query_as::<_, BurnUserDefaultsRow>(
            r"
            SELECT user_id, default_burn_ms, created_ts, updated_ts
            FROM burn_after_read_user_defaults
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`set_user_default`].
    pub async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO burn_after_read_user_defaults (user_id, default_burn_ms, created_ts, updated_ts)
            VALUES ($1, $2, $3, $3)
            ON CONFLICT (user_id) DO UPDATE SET
                default_burn_ms = EXCLUDED.default_burn_ms,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(user_id)
        .bind(default_burn_ms)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

// ── Delegation impl for the Postgres BurnAfterReadStorage ────────────

#[async_trait]
impl BurnAfterReadStoreApi for BurnAfterReadStorage {
    async fn get_settings(&self, user_id: &str, room_id: &str) -> Result<Option<BurnSettingsRow>, sqlx::Error> {
        self.get_settings(user_id, room_id).await
    }

    async fn set_settings(
        &self,
        user_id: &str,
        room_id: &str,
        is_enabled: bool,
        burn_after_ms: i64,
    ) -> Result<BurnSettingsRow, sqlx::Error> {
        self.set_settings(user_id, room_id, is_enabled, burn_after_ms).await
    }

    async fn schedule_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        delete_ts: i64,
    ) -> Result<BurnPendingRow, sqlx::Error> {
        self.schedule_burn(user_id, room_id, event_id, delete_ts).await
    }

    async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        self.cancel_burn(user_id, room_id, event_id).await
    }

    async fn get_pending_burns(&self, user_id: &str, room_id: &str) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
        self.get_pending_burns(user_id, room_id).await
    }

    async fn get_expired_burns(&self, now_ms: i64) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
        self.get_expired_burns(now_ms).await
    }

    async fn mark_burn_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        self.mark_burn_processed(id).await
    }

    async fn mark_burn_processed_batch(&self, ids: &[i64]) -> Result<(), sqlx::Error> {
        self.mark_burn_processed_batch(ids).await
    }

    async fn increment_retry_count(&self, ids: &[i64], last_error: &str) -> Result<(), sqlx::Error> {
        self.increment_retry_count(ids, last_error).await
    }

    async fn mark_dead_letter(&self, ids: &[i64]) -> Result<(), sqlx::Error> {
        self.mark_dead_letter(ids).await
    }

    async fn log_burned_event(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        burned_ts: i64,
    ) -> Result<(), sqlx::Error> {
        self.log_burned_event(user_id, room_id, event_id, burned_ts).await
    }

    async fn log_burned_event_batch(&self, entries: &[(String, String, String, i64)]) -> Result<(), sqlx::Error> {
        self.log_burned_event_batch(entries).await
    }

    async fn get_user_stats(&self, user_id: &str) -> Result<BurnStatsRow, sqlx::Error> {
        self.get_user_stats(user_id).await
    }

    async fn get_user_default(&self, user_id: &str) -> Result<Option<BurnUserDefaultsRow>, sqlx::Error> {
        self.get_user_default(user_id).await
    }

    async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> Result<(), sqlx::Error> {
        self.set_user_default(user_id, default_burn_ms).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_burn_settings_row_fields() {
        let row = BurnSettingsRow {
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            is_enabled: true,
            burn_after_ms: 60_000,
            created_ts: 1234567890,
            updated_ts: None,
        };
        assert_eq!(row.user_id, "@alice:example.com");
        assert!(row.is_enabled);
        assert_eq!(row.burn_after_ms, 60_000);
    }

    #[test]
    fn test_burn_pending_row_fields() {
        let row = BurnPendingRow {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_id: "$event1".to_string(),
            created_ts: 1234567890,
            delete_ts: 1234567950,
            is_processed: false,
            retry_count: 0,
            last_error: None,
            is_dead_letter: false,
        };
        assert_eq!(row.id, 1);
        assert_eq!(row.event_id, "$event1");
        assert!(!row.is_processed);
        assert_eq!(row.retry_count, 0);
        assert!(row.last_error.is_none());
        assert!(!row.is_dead_letter);
    }

    #[test]
    fn test_burn_log_row_fields() {
        let row = BurnLogRow {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_id: "$event1".to_string(),
            burned_ts: 1234567890,
        };
        assert_eq!(row.id, 1);
        assert_eq!(row.event_id, "$event1");
    }

    #[test]
    fn test_burn_user_defaults_row_fields() {
        let row = BurnUserDefaultsRow {
            user_id: "@alice:example.com".to_string(),
            default_burn_ms: 30000,
            created_ts: 1234567890,
            updated_ts: None,
        };
        assert_eq!(row.user_id, "@alice:example.com");
        assert_eq!(row.default_burn_ms, 30000);
    }

    #[test]
    fn test_burn_stats_row_fields() {
        let row = BurnStatsRow { total_burned: 5, total_pending: 2, rooms_enabled: 3 };
        assert_eq!(row.total_burned, 5);
        assert_eq!(row.total_pending, 2);
        assert_eq!(row.rooms_enabled, 3);
    }

    /// B-07: BurnPendingRow must expose the new retry-cap fields so the
    /// service can decide whether to dead-letter without re-querying.
    #[test]
    fn test_burn_pending_row_exposes_retry_fields() {
        let row = BurnPendingRow {
            id: 7,
            user_id: "@retry:ex.com".into(),
            room_id: "!room:ex.com".into(),
            event_id: "$ev:ex.com".into(),
            created_ts: 0,
            delete_ts: 0,
            is_processed: false,
            retry_count: 4,
            last_error: Some("mark_burn_processed_batch: connection refused".into()),
            is_dead_letter: false,
        };
        assert_eq!(row.retry_count, 4);
        assert_eq!(row.last_error.as_deref(), Some("mark_burn_processed_batch: connection refused"));
        assert!(!row.is_dead_letter);

        let dead = BurnPendingRow {
            is_dead_letter: true,
            retry_count: 5,
            last_error: Some("cap reached [moved to dead-letter]".into()),
            ..row.clone()
        };
        assert!(dead.is_dead_letter);
        assert_eq!(dead.retry_count, 5);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::PgPool;
    use std::sync::Arc;

    async fn test_pool() -> Arc<PgPool> {
        crate::test_utils::connect_shared_test_pool()
            .await
            .expect("test database must be reachable - a swallowed error here surfaces later as an unrelated failure")
    }

    async fn cleanup_burn_settings(pool: &PgPool, user_id: &str, room_id: &str) {
        sqlx::query("DELETE FROM burn_after_read_settings WHERE user_id = $1 AND room_id = $2")
            .bind(user_id)
            .bind(room_id)
            .execute(pool)
            .await
            .expect(
                "test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure",
            );
    }

    async fn cleanup_burn_pending(pool: &PgPool, user_id: &str, room_id: &str) {
        sqlx::query("DELETE FROM burn_after_read_pending WHERE user_id = $1 AND room_id = $2")
            .bind(user_id)
            .bind(room_id)
            .execute(pool)
            .await
            .expect(
                "test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure",
            );
    }

    async fn cleanup_burn_log(pool: &PgPool, user_id: &str) {
        sqlx::query("DELETE FROM burn_after_read_log WHERE user_id = $1").bind(user_id).execute(pool).await.expect(
            "test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure",
        );
    }

    async fn cleanup_burn_user_defaults(pool: &PgPool, user_id: &str) {
        sqlx::query("DELETE FROM burn_after_read_user_defaults WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .expect(
                "test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure",
            );
    }

    // 1. Set burn settings and retrieve them.
    #[tokio::test]
    async fn test_set_and_get_settings() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@settings_{suffix}:test.com");
        let room_id = format!("!settings_{suffix}:test.com");

        cleanup_burn_settings(&pool, &user_id, &room_id).await;

        let row = storage.set_settings(&user_id, &room_id, true, 120_000).await.expect("set_settings should succeed");

        assert_eq!(row.user_id, user_id);
        assert_eq!(row.room_id, room_id);
        assert!(row.is_enabled);
        assert_eq!(row.burn_after_ms, 120_000);
        assert!(row.created_ts > 0);
        assert!(row.updated_ts.is_some());
        assert_eq!(row.created_ts, row.updated_ts.unwrap());

        let retrieved = storage
            .get_settings(&user_id, &room_id)
            .await
            .expect("get_settings should succeed")
            .expect("settings should exist");

        assert_eq!(retrieved.user_id, user_id);
        assert_eq!(retrieved.room_id, room_id);
        assert!(retrieved.is_enabled);
        assert_eq!(retrieved.burn_after_ms, 120_000);

        cleanup_burn_settings(&pool, &user_id, &room_id).await;
    }

    // 2. get_settings returns None for nonexistent settings.
    #[tokio::test]
    async fn test_get_settings_nonexistent() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@nonexist_{suffix}:test.com");
        let room_id = format!("!nonexist_{suffix}:test.com");

        let result = storage.get_settings(&user_id, &room_id).await.expect("get_settings should succeed");

        assert!(result.is_none(), "nonexistent settings should return None");
    }

    // 3. set_settings with upsert updates an existing row.
    #[tokio::test]
    async fn test_set_settings_update_existing() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@upsert_{suffix}:test.com");
        let room_id = format!("!upsert_{suffix}:test.com");

        cleanup_burn_settings(&pool, &user_id, &room_id).await;

        // First insert: enabled with 60s
        let row1 =
            storage.set_settings(&user_id, &room_id, true, 60_000).await.expect("first set_settings should succeed");

        assert!(row1.is_enabled);
        assert_eq!(row1.burn_after_ms, 60_000);

        // Second insert: disable (upsert should update, not insert a duplicate)
        let row2 =
            storage.set_settings(&user_id, &room_id, false, 30_000).await.expect("second set_settings should succeed");

        assert!(!row2.is_enabled);
        assert_eq!(row2.burn_after_ms, 30_000);
        // created_ts should remain the same; updated_ts should change
        assert_eq!(row2.created_ts, row1.created_ts);
        assert!(
            row2.updated_ts.unwrap() > row1.updated_ts.unwrap() || row2.updated_ts.unwrap() >= row1.updated_ts.unwrap()
        );

        cleanup_burn_settings(&pool, &user_id, &room_id).await;
    }

    // 4. Schedule a burn and retrieve pending burns.
    #[tokio::test]
    async fn test_schedule_and_get_pending_burns() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@pending_{suffix}:test.com");
        let room_id = format!("!pending_{suffix}:test.com");

        cleanup_burn_pending(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();
        let delete_ts = now + 60_000;
        let event_id = format!("$event_pending_{suffix}");

        let row = storage
            .schedule_burn(&user_id, &room_id, &event_id, delete_ts)
            .await
            .expect("schedule_burn should succeed");

        assert!(row.id > 0);
        assert_eq!(row.user_id, user_id);
        assert_eq!(row.room_id, room_id);
        assert_eq!(row.event_id, event_id);
        assert!(!row.is_processed);
        assert_eq!(row.delete_ts, delete_ts);

        let pending = storage.get_pending_burns(&user_id, &room_id).await.expect("get_pending_burns should succeed");

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_id, event_id);
        assert!(!pending[0].is_processed);

        cleanup_burn_pending(&pool, &user_id, &room_id).await;
    }

    // 5. Cancel a burn marks it as processed so it no longer appears as pending.
    #[tokio::test]
    async fn test_cancel_burn_removes_from_pending() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@cancel_{suffix}:test.com");
        let room_id = format!("!cancel_{suffix}:test.com");
        let event_id = format!("$event_cancel_{suffix}");

        cleanup_burn_pending(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();
        let delete_ts = now + 60_000;
        storage.schedule_burn(&user_id, &room_id, &event_id, delete_ts).await.expect("schedule_burn should succeed");

        storage.cancel_burn(&user_id, &room_id, &event_id).await.expect("cancel_burn should succeed");

        let pending = storage.get_pending_burns(&user_id, &room_id).await.expect("get_pending_burns should succeed");

        assert!(pending.is_empty(), "cancelled burns should not appear in pending list");

        cleanup_burn_pending(&pool, &user_id, &room_id).await;
    }

    // 6. get_expired_burns returns only burns whose delete_ts has passed.
    #[tokio::test]
    async fn test_get_expired_burns() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@expired_{suffix}:test.com");
        let room_id = format!("!expired_{suffix}:test.com");

        cleanup_burn_pending(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();

        // Past: should be expired now
        let past_ts = now - 60_000;
        let event_past = format!("$event_past_{suffix}");
        storage
            .schedule_burn(&user_id, &room_id, &event_past, past_ts)
            .await
            .expect("schedule past burn should succeed");

        // Future: should NOT be expired yet
        let future_ts = now + 3_600_000;
        let event_future = format!("$event_future_{suffix}");
        storage
            .schedule_burn(&user_id, &room_id, &event_future, future_ts)
            .await
            .expect("schedule future burn should succeed");

        let expired = storage.get_expired_burns(now).await.expect("get_expired_burns should succeed");

        // Should only contain the past event (the future one may appear too
        // if other tests leave data, but at minimum the past one must be present)
        let has_past = expired.iter().any(|r| r.event_id == event_past);
        assert!(has_past, "past-delete_ts burn should be in expired list");

        // Future event should NOT appear for cutoff == now
        let has_future = expired.iter().any(|r| r.event_id == event_future);
        assert!(!has_future, "future-delete_ts burn should NOT be in expired list");

        cleanup_burn_pending(&pool, &user_id, &room_id).await;
    }

    // 7. Mark a burn as processed by id.
    #[tokio::test]
    async fn test_mark_burn_processed_by_id() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@processed_{suffix}:test.com");
        let room_id = format!("!processed_{suffix}:test.com");
        let event_id = format!("$event_processed_{suffix}");

        cleanup_burn_pending(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();
        let delete_ts = now + 60_000;
        let scheduled = storage
            .schedule_burn(&user_id, &room_id, &event_id, delete_ts)
            .await
            .expect("schedule_burn should succeed");

        storage.mark_burn_processed(scheduled.id).await.expect("mark_burn_processed should succeed");

        // Should no longer appear in pending (is_processed = TRUE)
        let pending = storage.get_pending_burns(&user_id, &room_id).await.expect("get_pending_burns should succeed");

        assert!(
            pending.iter().all(|r| r.id != scheduled.id),
            "marked-as-processed burn should not appear in pending list"
        );

        cleanup_burn_pending(&pool, &user_id, &room_id).await;
    }

    // 8. Log a burned event and verify user stats.
    #[tokio::test]
    async fn test_log_burned_event_and_get_stats() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@logstats_{suffix}:test.com");
        let room_id = format!("!logstats_{suffix}:test.com");
        let event_id = format!("$event_log_{suffix}");

        cleanup_burn_log(&pool, &user_id).await;
        cleanup_burn_pending(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();

        // Log a burned event
        storage.log_burned_event(&user_id, &room_id, &event_id, now).await.expect("log_burned_event should succeed");

        // Schedule a pending burn to produce non-zero total_pending
        storage
            .schedule_burn(&user_id, &room_id, &format!("$pending2_{suffix}"), now + 60_000)
            .await
            .expect("schedule burn should succeed");

        let stats = storage.get_user_stats(&user_id).await.expect("get_user_stats should succeed");

        assert!(stats.total_burned >= 1, "should have at least 1 burned event");
        assert!(stats.total_pending >= 1, "should have at least 1 pending burn");

        cleanup_burn_log(&pool, &user_id).await;
        cleanup_burn_pending(&pool, &user_id, &room_id).await;
    }

    // 8b. Batch log is idempotent: re-logging the same (user_id, event_id) must not duplicate
    // rows and must not error. This requires a UNIQUE index on (user_id, event_id) — without
    // it PostgreSQL rejects the `ON CONFLICT (user_id, event_id) DO NOTHING` with 42P10.
    #[tokio::test]
    async fn test_log_burned_event_batch_is_idempotent_on_conflict_target() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@batchlog_{suffix}:test.com");
        let room_id = format!("!batchlog_{suffix}:test.com");
        let event_a = format!("$batch_a_{suffix}");
        let event_b = format!("$batch_b_{suffix}");

        cleanup_burn_log(&pool, &user_id).await;

        let now = current_timestamp_millis();
        let batch = vec![
            (user_id.clone(), room_id.clone(), event_a.clone(), now),
            (user_id.clone(), room_id.clone(), event_b.clone(), now),
        ];

        storage
            .log_burned_event_batch(&batch)
            .await
            .expect("batch insert must succeed — a missing (user_id, event_id) unique index makes it 42P10");

        // Replay the same batch: ON CONFLICT (user_id, event_id) DO NOTHING must swallow it.
        storage.log_burned_event_batch(&batch).await.expect("replayed batch must be a no-op, not an error");

        let count: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM burn_after_read_log WHERE user_id = $1", &user_id)
                .fetch_one(&*pool)
                .await
                .expect("counting must succeed");

        assert_eq!(count, 2, "replaying the batch must not duplicate rows");

        cleanup_burn_log(&pool, &user_id).await;
    }

    // 9. Set and retrieve user default burn time.
    #[tokio::test]
    async fn test_set_and_get_user_default() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@default_{suffix}:test.com");

        cleanup_burn_user_defaults(&pool, &user_id).await;

        // Initially no default
        let before = storage.get_user_default(&user_id).await.expect("get_user_default should succeed");
        assert!(before.is_none(), "new user should have no default");

        // Set a default
        storage.set_user_default(&user_id, 90_000).await.expect("set_user_default should succeed");

        // Retrieve and verify
        let after = storage
            .get_user_default(&user_id)
            .await
            .expect("get_user_default should succeed")
            .expect("default should exist after setting");

        assert_eq!(after.user_id, user_id);
        assert_eq!(after.default_burn_ms, 90_000);
        assert!(after.created_ts > 0);
        assert!(after.updated_ts.is_some());

        // Update the default (upsert)
        storage.set_user_default(&user_id, 120_000).await.expect("second set_user_default should succeed");

        let updated = storage
            .get_user_default(&user_id)
            .await
            .expect("get_user_default should succeed")
            .expect("default should still exist");

        assert_eq!(updated.default_burn_ms, 120_000);
        assert_eq!(updated.created_ts, after.created_ts);
        assert!(updated.updated_ts.unwrap() >= after.updated_ts.unwrap());

        cleanup_burn_user_defaults(&pool, &user_id).await;
    }

    // 10. Full round-trip: settings -> schedule -> get pending -> mark processed -> log -> stats.
    #[tokio::test]
    async fn test_full_round_trip() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@roundtrip_{suffix}:test.com");
        let room_id = format!("!roundtrip_{suffix}:test.com");
        let event_id = format!("$event_rt_{suffix}");

        // Cleanup start
        cleanup_burn_settings(&pool, &user_id, &room_id).await;
        cleanup_burn_pending(&pool, &user_id, &room_id).await;
        cleanup_burn_log(&pool, &user_id).await;
        cleanup_burn_user_defaults(&pool, &user_id).await;

        // Step 1: Set user default
        storage.set_user_default(&user_id, 60_000).await.expect("set_user_default should succeed");

        // Step 2: Enable burn-after-read for the room
        let settings =
            storage.set_settings(&user_id, &room_id, true, 60_000).await.expect("set_settings should succeed");
        assert!(settings.is_enabled);

        // Step 3: Schedule a burn for the event
        let now = current_timestamp_millis();
        let delete_ts = now + 30_000;
        let scheduled = storage
            .schedule_burn(&user_id, &room_id, &event_id, delete_ts)
            .await
            .expect("schedule_burn should succeed");
        assert!(!scheduled.is_processed);

        // Step 4: Verify it appears in pending burns
        let pending = storage.get_pending_burns(&user_id, &room_id).await.expect("get_pending_burns should succeed");
        assert!(pending.iter().any(|r| r.event_id == event_id));

        // Step 5: Mark as processed
        storage.mark_burn_processed(scheduled.id).await.expect("mark_burn_processed should succeed");

        // Step 6: Log the burned event
        let burned_ts = current_timestamp_millis();
        storage
            .log_burned_event(&user_id, &room_id, &event_id, burned_ts)
            .await
            .expect("log_burned_event should succeed");

        // Step 7: Verify stats reflect the burned event
        let stats = storage.get_user_stats(&user_id).await.expect("get_user_stats should succeed");
        assert!(stats.total_burned >= 1, "total_burned should be >= 1");

        // Cleanup end
        cleanup_burn_settings(&pool, &user_id, &room_id).await;
        cleanup_burn_pending(&pool, &user_id, &room_id).await;
        cleanup_burn_log(&pool, &user_id).await;
        cleanup_burn_user_defaults(&pool, &user_id).await;
    }

    // 11. Multiple pending burns with mixed expiration times.
    #[tokio::test]
    async fn test_batch_cleanup_expired() {
        let pool = test_pool().await;
        let storage = BurnAfterReadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@batch_{suffix}:test.com");
        let room_id = format!("!batch_{suffix}:test.com");

        cleanup_burn_pending(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();

        // Schedule 3 burns: 2 expired, 1 not yet
        for i in 0..3 {
            let delete_ts = if i < 2 { now - 60_000 } else { now + 3_600_000 };
            let event_id = format!("$event_batch_{suffix}_{i}");
            storage
                .schedule_burn(&user_id, &room_id, &event_id, delete_ts)
                .await
                .expect("schedule_burn should succeed");
        }

        let expired = storage.get_expired_burns(now).await.expect("get_expired_burns should succeed");

        // At least the 2 past-delete_ts burns should show up
        let past_count = expired.iter().filter(|r| r.user_id == user_id && r.room_id == room_id).count();
        assert!(past_count >= 2, "should find at least 2 expired burns for this user/room");

        // Mark all expired as processed
        for row in &expired {
            if row.user_id == user_id && row.room_id == room_id {
                storage.mark_burn_processed(row.id).await.expect("mark_burn_processed should succeed");
            }
        }

        // After marking, pending should only contain the future burn
        let remaining = storage.get_pending_burns(&user_id, &room_id).await.expect("get_pending_burns should succeed");

        let has_future = remaining.iter().any(|r| r.event_id.contains(&format!("{suffix}_2")));
        assert!(has_future || remaining.len() == 1, "only the future burn should remain in pending");

        cleanup_burn_pending(&pool, &user_id, &room_id).await;
    }
}
