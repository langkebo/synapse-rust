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
                COALESCE(SUM(size_bytes), 0) as total_size_bytes,
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
                COALESCE(SUM(size_bytes), 0) as total_size_bytes
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
