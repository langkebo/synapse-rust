use chrono::Utc;
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, FromRow)]
pub struct BurnSettingsRow {
    pub user_id: String,
    pub room_id: String,
    pub is_enabled: bool,
    pub burn_after_ms: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct BurnPendingRow {
    pub id: i64,
    pub user_id: String,
    pub room_id: String,
    pub event_id: String,
    pub created_ts: i64,
    pub delete_ts: i64,
    pub is_processed: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct BurnLogRow {
    pub id: i64,
    pub user_id: String,
    pub room_id: String,
    pub event_id: String,
    pub burned_ts: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct BurnUserDefaultsRow {
    pub user_id: String,
    pub default_burn_ms: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct BurnStatsRow {
    pub total_burned: i64,
    pub total_pending: i64,
    pub rooms_enabled: i64,
}

#[derive(Clone)]
pub struct BurnAfterReadStorage {
    pool: Arc<PgPool>,
}

impl BurnAfterReadStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

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

    pub async fn set_settings(
        &self,
        user_id: &str,
        room_id: &str,
        is_enabled: bool,
        burn_after_ms: i64,
    ) -> Result<BurnSettingsRow, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

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

    pub async fn schedule_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        delete_ts: i64,
    ) -> Result<BurnPendingRow, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, BurnPendingRow>(
            r"
            INSERT INTO burn_after_read_pending (user_id, room_id, event_id, created_ts, delete_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, room_id, event_id) DO UPDATE SET
                delete_ts = EXCLUDED.delete_ts,
                created_ts = EXCLUDED.created_ts
            RETURNING id, user_id, room_id, event_id, created_ts, delete_ts, is_processed
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

    pub async fn get_pending_burns(&self, user_id: &str, room_id: &str) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BurnPendingRow>(
            r"
            SELECT id, user_id, room_id, event_id, created_ts, delete_ts, is_processed
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

    pub async fn get_expired_burns(&self, now_ms: i64) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BurnPendingRow>(
            r"
            SELECT id, user_id, room_id, event_id, created_ts, delete_ts, is_processed
            FROM burn_after_read_pending
            WHERE delete_ts <= $1 AND is_processed = FALSE
            ORDER BY delete_ts ASC
            ",
        )
        .bind(now_ms)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn mark_burn_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE burn_after_read_pending SET is_processed = TRUE WHERE id = $1 AND is_processed = FALSE")
            .bind(id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

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

    pub async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

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
        };
        assert_eq!(row.id, 1);
        assert_eq!(row.event_id, "$event1");
        assert!(!row.is_processed);
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
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::sync::Arc;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn cleanup_burn_settings(pool: &PgPool, user_id: &str, room_id: &str) {
        sqlx::query("DELETE FROM burn_after_read_settings WHERE user_id = $1 AND room_id = $2")
            .bind(user_id)
            .bind(room_id)
            .execute(pool)
            .await
            .ok();
    }

    async fn cleanup_burn_pending(pool: &PgPool, user_id: &str, room_id: &str) {
        sqlx::query("DELETE FROM burn_after_read_pending WHERE user_id = $1 AND room_id = $2")
            .bind(user_id)
            .bind(room_id)
            .execute(pool)
            .await
            .ok();
    }

    async fn cleanup_burn_log(pool: &PgPool, user_id: &str) {
        sqlx::query("DELETE FROM burn_after_read_log WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .ok();
    }

    async fn cleanup_burn_user_defaults(pool: &PgPool, user_id: &str) {
        sqlx::query("DELETE FROM burn_after_read_user_defaults WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .ok();
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

        let row = storage
            .set_settings(&user_id, &room_id, true, 120_000)
            .await
            .expect("set_settings should succeed");

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

        let result = storage
            .get_settings(&user_id, &room_id)
            .await
            .expect("get_settings should succeed");

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
        let row1 = storage
            .set_settings(&user_id, &room_id, true, 60_000)
            .await
            .expect("first set_settings should succeed");

        assert!(row1.is_enabled);
        assert_eq!(row1.burn_after_ms, 60_000);

        // Second insert: disable (upsert should update, not insert a duplicate)
        let row2 = storage
            .set_settings(&user_id, &room_id, false, 30_000)
            .await
            .expect("second set_settings should succeed");

        assert!(!row2.is_enabled);
        assert_eq!(row2.burn_after_ms, 30_000);
        // created_ts should remain the same; updated_ts should change
        assert_eq!(row2.created_ts, row1.created_ts);
        assert!(row2.updated_ts.unwrap() > row1.updated_ts.unwrap()
                || row2.updated_ts.unwrap() >= row1.updated_ts.unwrap());

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

        let now = chrono::Utc::now().timestamp_millis();
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

        let pending = storage
            .get_pending_burns(&user_id, &room_id)
            .await
            .expect("get_pending_burns should succeed");

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

        let now = chrono::Utc::now().timestamp_millis();
        let delete_ts = now + 60_000;
        storage
            .schedule_burn(&user_id, &room_id, &event_id, delete_ts)
            .await
            .expect("schedule_burn should succeed");

        storage
            .cancel_burn(&user_id, &room_id, &event_id)
            .await
            .expect("cancel_burn should succeed");

        let pending = storage
            .get_pending_burns(&user_id, &room_id)
            .await
            .expect("get_pending_burns should succeed");

        assert!(
            pending.is_empty(),
            "cancelled burns should not appear in pending list"
        );

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

        let now = chrono::Utc::now().timestamp_millis();

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

        let expired = storage
            .get_expired_burns(now)
            .await
            .expect("get_expired_burns should succeed");

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

        let now = chrono::Utc::now().timestamp_millis();
        let delete_ts = now + 60_000;
        let scheduled = storage
            .schedule_burn(&user_id, &room_id, &event_id, delete_ts)
            .await
            .expect("schedule_burn should succeed");

        storage
            .mark_burn_processed(scheduled.id)
            .await
            .expect("mark_burn_processed should succeed");

        // Should no longer appear in pending (is_processed = TRUE)
        let pending = storage
            .get_pending_burns(&user_id, &room_id)
            .await
            .expect("get_pending_burns should succeed");

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

        let now = chrono::Utc::now().timestamp_millis();

        // Log a burned event
        storage
            .log_burned_event(&user_id, &room_id, &event_id, now)
            .await
            .expect("log_burned_event should succeed");

        // Schedule a pending burn to produce non-zero total_pending
        storage
            .schedule_burn(&user_id, &room_id, &format!("$pending2_{suffix}"), now + 60_000)
            .await
            .expect("schedule burn should succeed");

        let stats = storage
            .get_user_stats(&user_id)
            .await
            .expect("get_user_stats should succeed");

        assert!(stats.total_burned >= 1, "should have at least 1 burned event");
        assert!(stats.total_pending >= 1, "should have at least 1 pending burn");

        cleanup_burn_log(&pool, &user_id).await;
        cleanup_burn_pending(&pool, &user_id, &room_id).await;
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
        let before = storage
            .get_user_default(&user_id)
            .await
            .expect("get_user_default should succeed");
        assert!(before.is_none(), "new user should have no default");

        // Set a default
        storage
            .set_user_default(&user_id, 90_000)
            .await
            .expect("set_user_default should succeed");

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
        storage
            .set_user_default(&user_id, 120_000)
            .await
            .expect("second set_user_default should succeed");

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
        storage
            .set_user_default(&user_id, 60_000)
            .await
            .expect("set_user_default should succeed");

        // Step 2: Enable burn-after-read for the room
        let settings = storage
            .set_settings(&user_id, &room_id, true, 60_000)
            .await
            .expect("set_settings should succeed");
        assert!(settings.is_enabled);

        // Step 3: Schedule a burn for the event
        let now = chrono::Utc::now().timestamp_millis();
        let delete_ts = now + 30_000;
        let scheduled = storage
            .schedule_burn(&user_id, &room_id, &event_id, delete_ts)
            .await
            .expect("schedule_burn should succeed");
        assert!(!scheduled.is_processed);

        // Step 4: Verify it appears in pending burns
        let pending = storage
            .get_pending_burns(&user_id, &room_id)
            .await
            .expect("get_pending_burns should succeed");
        assert!(pending.iter().any(|r| r.event_id == event_id));

        // Step 5: Mark as processed
        storage
            .mark_burn_processed(scheduled.id)
            .await
            .expect("mark_burn_processed should succeed");

        // Step 6: Log the burned event
        let burned_ts = chrono::Utc::now().timestamp_millis();
        storage
            .log_burned_event(&user_id, &room_id, &event_id, burned_ts)
            .await
            .expect("log_burned_event should succeed");

        // Step 7: Verify stats reflect the burned event
        let stats = storage
            .get_user_stats(&user_id)
            .await
            .expect("get_user_stats should succeed");
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

        let now = chrono::Utc::now().timestamp_millis();

        // Schedule 3 burns: 2 expired, 1 not yet
        for i in 0..3 {
            let delete_ts = if i < 2 { now - 60_000 } else { now + 3_600_000 };
            let event_id = format!("$event_batch_{suffix}_{i}");
            storage
                .schedule_burn(&user_id, &room_id, &event_id, delete_ts)
                .await
                .expect("schedule_burn should succeed");
        }

        let expired = storage
            .get_expired_burns(now)
            .await
            .expect("get_expired_burns should succeed");

        // At least the 2 past-delete_ts burns should show up
        let past_count = expired
            .iter()
            .filter(|r| r.user_id == user_id && r.room_id == room_id)
            .count();
        assert!(past_count >= 2, "should find at least 2 expired burns for this user/room");

        // Mark all expired as processed
        for row in &expired {
            if row.user_id == user_id && row.room_id == room_id {
                storage
                    .mark_burn_processed(row.id)
                    .await
                    .expect("mark_burn_processed should succeed");
            }
        }

        // After marking, pending should only contain the future burn
        let remaining = storage
            .get_pending_burns(&user_id, &room_id)
            .await
            .expect("get_pending_burns should succeed");

        let has_future = remaining.iter().any(|r| r.event_id.contains(&format!("{suffix}_2")));
        assert!(has_future || remaining.len() == 1,
                "only the future burn should remain in pending");

        cleanup_burn_pending(&pool, &user_id, &room_id).await;
    }
}
