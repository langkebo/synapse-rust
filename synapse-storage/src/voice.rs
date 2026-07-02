use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VoiceUsageRecord {
    pub id: i64,
    pub user_id: String,
    pub room_id: Option<String>,
    pub media_id: String,
    pub content_type: String,
    pub duration_ms: i32,
    pub size_bytes: i64,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VoiceAggregatedStats {
    pub total_uploads: i64,
    pub total_duration_ms: i64,
    pub total_size_bytes: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VoiceUserAggregatedStats {
    pub total_uploads: i64,
    pub total_duration_ms: i64,
    pub total_size_bytes: i64,
    pub uploads_today: i64,
}

#[derive(Clone)]
pub struct VoiceStorage {
    pool: Arc<PgPool>,
}

impl VoiceStorage {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn record_upload(
        &self,
        user_id: &str,
        room_id: Option<&str>,
        media_id: &str,
        content_type: &str,
        duration_ms: i32,
        size_bytes: i64,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let row: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO voice_usage_stats (user_id, room_id, media_id, content_type, duration_ms, size_bytes, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(media_id)
        .bind(content_type)
        .bind(duration_ms)
        .bind(size_bytes)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;
        Ok(row.0)
    }

    #[allow(clippy::expect_used)]
    pub async fn get_user_stats(&self, user_id: &str) -> Result<VoiceUserAggregatedStats, sqlx::Error> {
        let today_start = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight 00:00:00 is always a valid time")
            .and_utc()
            .timestamp_millis();

        let row = sqlx::query_as::<_, VoiceUserAggregatedStats>(
            r#"
            SELECT
                COUNT(*) as total_uploads,
                COALESCE(SUM(duration_ms), 0) as total_duration_ms,
                COALESCE(SUM(size_bytes)::bigint, 0) as total_size_bytes,
                COALESCE(SUM(CASE WHEN created_ts >= $2 THEN 1 ELSE 0 END), 0) as uploads_today
            FROM voice_usage_stats
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .bind(today_start)
        .fetch_one(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_room_stats(&self, room_id: &str) -> Result<VoiceAggregatedStats, sqlx::Error> {
        let row = sqlx::query_as::<_, VoiceAggregatedStats>(
            r#"
            SELECT
                COUNT(*) as total_uploads,
                COALESCE(SUM(duration_ms), 0) as total_duration_ms,
                COALESCE(SUM(size_bytes)::bigint, 0) as total_size_bytes
            FROM voice_usage_stats
            WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_global_user_stats(&self, user_id: &str) -> Result<VoiceUserAggregatedStats, sqlx::Error> {
        self.get_user_stats(user_id).await
    }

    pub async fn delete_user_stats(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1").bind(user_id).execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_room_stats(&self, room_id: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM voice_usage_stats WHERE room_id = $1").bind(room_id).execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    pub async fn get_room_messages(
        &self,
        room_id: &str,
        limit: i64,
        from_ts: Option<i64>,
    ) -> Result<Vec<VoiceUsageRecord>, sqlx::Error> {
        let limit = limit.clamp(1, 1000);
        let rows = if let Some(ts) = from_ts {
            sqlx::query_as::<_, VoiceUsageRecord>(
                r#"
                SELECT id, user_id, room_id, media_id, content_type, duration_ms, size_bytes, created_ts
                FROM voice_usage_stats
                WHERE room_id = $1 AND created_ts < $2
                ORDER BY created_ts DESC
                LIMIT $3
                "#,
            )
            .bind(room_id)
            .bind(ts)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, VoiceUsageRecord>(
                r#"
                SELECT id, user_id, room_id, media_id, content_type, duration_ms, size_bytes, created_ts
                FROM voice_usage_stats
                WHERE room_id = $1
                ORDER BY created_ts DESC
                LIMIT $2
                "#,
            )
            .bind(room_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        };
        Ok(rows)
    }

    pub async fn get_user_messages(
        &self,
        user_id: &str,
        limit: i64,
        from_ts: Option<i64>,
    ) -> Result<Vec<VoiceUsageRecord>, sqlx::Error> {
        let limit = limit.clamp(1, 1000);
        let rows = if let Some(ts) = from_ts {
            sqlx::query_as::<_, VoiceUsageRecord>(
                r#"
                SELECT id, user_id, room_id, media_id, content_type, duration_ms, size_bytes, created_ts
                FROM voice_usage_stats
                WHERE user_id = $1 AND created_ts < $2
                ORDER BY created_ts DESC
                LIMIT $3
                "#,
            )
            .bind(user_id)
            .bind(ts)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, VoiceUsageRecord>(
                r#"
                SELECT id, user_id, room_id, media_id, content_type, duration_ms, size_bytes, created_ts
                FROM voice_usage_stats
                WHERE user_id = $1
                ORDER BY created_ts DESC
                LIMIT $2
                "#,
            )
            .bind(user_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        };
        Ok(rows)
    }

    pub async fn get_by_media_id(&self, media_id: &str) -> Result<Option<VoiceUsageRecord>, sqlx::Error> {
        let row = sqlx::query_as::<_, VoiceUsageRecord>(
            r#"
            SELECT id, user_id, room_id, media_id, content_type, duration_ms, size_bytes, created_ts
            FROM voice_usage_stats
            WHERE media_id = $1
            "#,
        )
        .bind(media_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(row)
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

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

    /// Insert a minimal room row so foreign-key constraints are satisfied
    /// when other tables reference rooms. Not needed for `voice_usage_stats`
    /// (which has no FK to rooms), but provided as a standard helper.
    async fn ensure_test_room(pool: &PgPool, room_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, created_ts)
               VALUES ($1, $2)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    /// Insert a minimal event row so foreign-key constraints are satisfied
    /// when other tables reference events. Not needed for `voice_usage_stats`
    /// (which has no FK to events and no event_id column), but provided as a
    /// standard helper.
    async fn ensure_test_event(pool: &PgPool, event_id: &str, room_id: &str, sender: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        ensure_test_room(pool, room_id).await;
        sqlx::query(
            r#"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
               VALUES ($1, $2, $3, 'm.room.message', '{}'::jsonb, $4)
               ON CONFLICT (event_id) DO NOTHING"#,
        )
        .bind(event_id)
        .bind(room_id)
        .bind(sender)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test event");
    }

    // ---------------------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_record_upload_success() {
        let pool = test_pool().await;
        let storage = VoiceStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4().to_string();
        let user_id = format!("@voice_user_{suffix}:test.com");
        let media_id = format!("media_{suffix}");
        let room_id = format!("!room_{suffix}:test.com");

        // Cleanup at start (previous failed run)
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();

        let id = storage
            .record_upload(&user_id, Some(&room_id), &media_id, "audio/ogg", 5000, 102400)
            .await
            .expect("record_upload should succeed");
        assert!(id > 0);

        // Cleanup at end
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_by_media_id_found_and_not_found() {
        let pool = test_pool().await;
        let storage = VoiceStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4().to_string();
        let user_id = format!("@voice_get_{suffix}:test.com");
        let media_id = format!("media_get_{suffix}");
        let room_id = format!("!room_get_{suffix}:test.com");

        // Cleanup at start
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();

        // Not found for non-existent media_id
        let result = storage
            .get_by_media_id("nonexistent_media_id_12345")
            .await
            .expect("query should succeed");
        assert!(result.is_none(), "non-existent media_id should return None");

        // Record and then find
        storage
            .record_upload(&user_id, Some(&room_id), &media_id, "audio/mp3", 3000, 51200)
            .await
            .expect("record_upload should succeed");

        let found = storage
            .get_by_media_id(&media_id)
            .await
            .expect("query should succeed")
            .expect("media_id should be found");

        assert_eq!(found.user_id, user_id);
        assert_eq!(found.media_id, media_id);
        assert_eq!(found.content_type, "audio/mp3");
        assert_eq!(found.duration_ms, 3000);
        assert_eq!(found.size_bytes, 51200);

        // Cleanup at end
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_room_messages_with_pagination() {
        let pool = test_pool().await;
        let storage = VoiceStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4().to_string();
        let user_id = format!("@voice_room_{suffix}:test.com");
        let room_id = format!("!room_msgs_{suffix}:test.com");

        // Cleanup at start
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();

        // Record 3 messages for the same room
        for i in 0..3 {
            let media_id = format!("room_media_{suffix}_{i}");
            storage
                .record_upload(
                    &user_id,
                    Some(&room_id),
                    &media_id,
                    "audio/ogg",
                    1000 * (i + 1),
                    1024 * (i + 1) as i64,
                )
                .await
                .expect("record should succeed");
        }

        // Retrieve with limit 2 (most recent first)
        let msgs = storage
            .get_room_messages(&room_id, 2, None)
            .await
            .expect("get_room_messages should succeed");

        assert_eq!(msgs.len(), 2, "should respect limit of 2");
        for msg in &msgs {
            assert_eq!(msg.room_id.as_deref(), Some(room_id.as_str()));
            assert_eq!(msg.user_id, user_id);
        }
        assert!(msgs[0].created_ts >= msgs[1].created_ts);

        // Cleanup at end
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_user_messages_basic() {
        let pool = test_pool().await;
        let storage = VoiceStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4().to_string();
        let user_id = format!("@voice_usermsgs_{suffix}:test.com");
        let media_id = format!("user_media_{suffix}");

        // Cleanup at start
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();

        storage
            .record_upload(&user_id, None, &media_id, "audio/ogg", 7000, 204800)
            .await
            .expect("record should succeed");

        let msgs = storage
            .get_user_messages(&user_id, 10, None)
            .await
            .expect("get_user_messages should succeed");

        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].user_id, user_id);
        assert_eq!(msgs[0].media_id, media_id);
        assert!(msgs[0].room_id.is_none());

        // Cleanup at end
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_user_stats_aggregation() {
        let pool = test_pool().await;
        let storage = VoiceStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4().to_string();
        let user_id = format!("@voice_ustats_{suffix}:test.com");

        // Cleanup at start
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();

        // Record 3 uploads
        for i in 0..3 {
            storage
                .record_upload(
                    &user_id,
                    None,
                    &format!("stats_media_{suffix}_{i}"),
                    "audio/ogg",
                    3000,
                    50000,
                )
                .await
                .expect("record should succeed");
        }

        let stats = storage
            .get_user_stats(&user_id)
            .await
            .expect("get_user_stats should succeed");

        assert_eq!(stats.total_uploads, 3);
        assert_eq!(stats.total_duration_ms, 9000);
        assert_eq!(stats.total_size_bytes, 150000);
        assert!(stats.uploads_today >= 0);

        // Cleanup at end
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_room_stats_aggregation() {
        let pool = test_pool().await;
        let storage = VoiceStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4().to_string();
        let user_id = format!("@voice_rstats_{suffix}:test.com");
        let room_id = format!("!room_stats_{suffix}:test.com");

        // Cleanup at start
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();

        // Record 2 uploads in the same room
        for i in 0..2 {
            storage
                .record_upload(
                    &user_id,
                    Some(&room_id),
                    &format!("roomstats_media_{suffix}_{i}"),
                    "audio/mp3",
                    15000,
                    100000,
                )
                .await
                .expect("record should succeed");
        }

        let stats = storage
            .get_room_stats(&room_id)
            .await
            .expect("get_room_stats should succeed");

        assert_eq!(stats.total_uploads, 2);
        assert_eq!(stats.total_duration_ms, 30000);
        assert_eq!(stats.total_size_bytes, 200000);

        // Cleanup at end
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_delete_user_stats_removes_rows() {
        let pool = test_pool().await;
        let storage = VoiceStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4().to_string();
        let user_id = format!("@voice_deluser_{suffix}:test.com");

        // Cleanup at start
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();

        storage
            .record_upload(&user_id, None, &format!("del_media_{suffix}"), "audio/ogg", 1000, 1024)
            .await
            .expect("record should succeed");

        let affected = storage
            .delete_user_stats(&user_id)
            .await
            .expect("delete_user_stats should succeed");
        assert_eq!(affected, 1);

        // Verify deleted
        let stats = storage.get_user_stats(&user_id).await.expect("query should succeed");
        assert_eq!(stats.total_uploads, 0);

        // Cleanup at end (already deleted, but safe to re-run)
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_round_trip_all_fields() {
        let pool = test_pool().await;
        let storage = VoiceStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4().to_string();
        let user_id = format!("@voice_roundtrip_{suffix}:test.com");
        let media_id = format!("rt_media_{suffix}");
        let room_id = format!("!rt_room_{suffix}:test.com");

        // Cleanup at start
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();

        let id = storage
            .record_upload(
                &user_id,
                Some(&room_id),
                &media_id,
                "audio/ogg; codecs=opus",
                42000,
                987654,
            )
            .await
            .expect("record_upload should succeed");
        assert!(id > 0);

        // Retrieve by media_id and verify all fields
        let record = storage
            .get_by_media_id(&media_id)
            .await
            .expect("query should succeed")
            .expect("record should exist");

        assert_eq!(record.user_id, user_id);
        assert_eq!(record.room_id, Some(room_id.clone()));
        assert_eq!(record.media_id, media_id);
        assert_eq!(record.content_type, "audio/ogg; codecs=opus");
        assert_eq!(record.duration_ms, 42000);
        assert_eq!(record.size_bytes, 987654);
        assert!(record.created_ts > 0);

        // Verify user messages returns the same record
        let user_msgs = storage
            .get_user_messages(&user_id, 10, None)
            .await
            .expect("get_user_messages should succeed");
        assert_eq!(user_msgs.len(), 1);
        assert_eq!(user_msgs[0].id, record.id);

        // Verify global_user_stats delegates to get_user_stats
        let global_stats = storage
            .get_global_user_stats(&user_id)
            .await
            .expect("get_global_user_stats should succeed");
        assert_eq!(global_stats.total_uploads, 1);
        assert_eq!(global_stats.total_duration_ms, 42000);
        assert_eq!(global_stats.total_size_bytes, 987654);

        // Cleanup at end
        sqlx::query("DELETE FROM voice_usage_stats WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .ok();
    }
}
