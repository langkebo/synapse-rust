use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VoiceMessage {
    pub message_id: String,
    pub room_id: Option<String>,
    pub user_id: String,
    pub duration_ms: i32,
    pub file_size: i64,
    pub content_type: String,
    pub waveform_data: Option<serde_json::Value>,
    pub file_path: Option<String>,
    pub session_id: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone)]
pub struct CreateVoiceMessage {
    pub message_id: String,
    pub room_id: Option<String>,
    pub user_id: String,
    pub duration_ms: i32,
    pub file_size: i64,
    pub content_type: String,
    pub waveform_data: Option<serde_json::Value>,
    pub file_path: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VoiceUsageStats {
    pub id: i64,
    pub user_id: String,
    pub date: chrono::NaiveDate,
    pub message_count: i32,
    pub total_duration_ms: i32,
    pub total_file_size: i64,
    pub created_ts: i64,
    pub updated_ts: i64,
}

pub struct VoiceUsageStatsStorage {
    pool: Arc<Pool<Postgres>>,
}

impl VoiceUsageStatsStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn get_user_stats(
        &self,
        user_id: &str,
        days: i64,
    ) -> Result<Vec<VoiceUsageStats>, sqlx::Error> {
        let date_threshold = chrono::Utc::now() - chrono::Duration::days(days as i64);

        let rows = sqlx::query(
            r#"
            SELECT * FROM voice_usage_stats
            WHERE user_id = $1 AND date >= $2
            ORDER BY date DESC
            "#,
        )
        .bind(user_id)
        .bind(date_threshold.naive_utc())
        .fetch_all(&*self.pool)
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            stats.push(VoiceUsageStats {
                id: row.get("id"),
                user_id: row.get("user_id"),
                date: row.get("date"),
                message_count: row.get("message_count"),
                total_duration_ms: row.get("total_duration_ms"),
                total_file_size: row.get("total_file_size"),
                created_ts: row.get("created_ts"),
                updated_ts: row.get("updated_ts"),
            });
        }

        Ok(stats)
    }

    pub async fn update_or_create_stats(
        &self,
        user_id: &str,
        duration_ms: i32,
        file_size: i64,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now();
        let today = now.naive_utc().date();
        let now_ts = now.timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO voice_usage_stats (user_id, date, message_count, total_duration_ms, total_file_size, created_ts, updated_ts)
            VALUES ($1, $2, 1, $3, $4, $5, $6)
            ON CONFLICT (user_id, date) DO UPDATE SET
                message_count = voice_usage_stats.message_count + 1,
                total_duration_ms = voice_usage_stats.total_duration_ms + $3,
                total_file_size = voice_usage_stats.total_file_size + $4,
                updated_ts = $6
            "#,
        )
        .bind(user_id)
        .bind(today)
        .bind(duration_ms)
        .bind(file_size)
        .bind(now_ts)
        .bind(now_ts)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

pub struct VoiceMessageStorage {
    pool: Arc<Pool<Postgres>>,
}

impl VoiceMessageStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_message(
        &self,
        message: &CreateVoiceMessage,
    ) -> Result<VoiceMessage, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let row = sqlx::query(
            r#"
            INSERT INTO voice_messages (message_id, room_id, user_id, duration_ms, file_size, content_type, waveform_data, file_path, session_id, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(&message.message_id)
        .bind(&message.room_id)
        .bind(&message.user_id)
        .bind(message.duration_ms)
        .bind(message.file_size)
        .bind(&message.content_type)
        .bind(&message.waveform_data)
        .bind(&message.file_path)
        .bind(&message.session_id)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(VoiceMessage {
            message_id: row.get("message_id"),
            room_id: row.get("room_id"),
            user_id: row.get("user_id"),
            duration_ms: row.get("duration_ms"),
            file_size: row.get("file_size"),
            content_type: row.get("content_type"),
            waveform_data: row.get("waveform_data"),
            file_path: row.get("file_path"),
            session_id: row.get("session_id"),
            created_ts: row.get("created_ts"),
        })
    }

    pub async fn get_message(&self, message_id: &str) -> Result<Option<VoiceMessage>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT * FROM voice_messages
            WHERE message_id = $1
            "#,
        )
        .bind(message_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(VoiceMessage {
                message_id: row.get("message_id"),
                room_id: row.get("room_id"),
                user_id: row.get("user_id"),
                duration_ms: row.get("duration_ms"),
                file_size: row.get("file_size"),
                content_type: row.get("content_type"),
                waveform_data: row.get("waveform_data"),
                file_path: row.get("file_path"),
                session_id: row.get("session_id"),
                created_ts: row.get("created_ts"),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_user_messages(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VoiceMessage>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM voice_messages
            WHERE user_id = $1
            ORDER BY created_ts DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(VoiceMessage {
                message_id: row.get("message_id"),
                room_id: row.get("room_id"),
                user_id: row.get("user_id"),
                duration_ms: row.get("duration_ms"),
                file_size: row.get("file_size"),
                content_type: row.get("content_type"),
                waveform_data: row.get("waveform_data"),
                file_path: row.get("file_path"),
                session_id: row.get("session_id"),
                created_ts: row.get("created_ts"),
            });
        }

        Ok(messages)
    }

    pub async fn get_room_messages(
        &self,
        room_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VoiceMessage>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM voice_messages
            WHERE room_id = $1
            ORDER BY created_ts DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(room_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(VoiceMessage {
                message_id: row.get("message_id"),
                room_id: row.get("room_id"),
                user_id: row.get("user_id"),
                duration_ms: row.get("duration_ms"),
                file_size: row.get("file_size"),
                content_type: row.get("content_type"),
                waveform_data: row.get("waveform_data"),
                file_path: row.get("file_path"),
                session_id: row.get("session_id"),
                created_ts: row.get("created_ts"),
            });
        }

        Ok(messages)
    }

    pub async fn get_session_messages(
        &self,
        session_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VoiceMessage>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM voice_messages
            WHERE session_id = $1
            ORDER BY created_ts DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(session_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(VoiceMessage {
                message_id: row.get("message_id"),
                room_id: row.get("room_id"),
                user_id: row.get("user_id"),
                duration_ms: row.get("duration_ms"),
                file_size: row.get("file_size"),
                content_type: row.get("content_type"),
                waveform_data: row.get("waveform_data"),
                file_path: row.get("file_path"),
                session_id: row.get("session_id"),
                created_ts: row.get("created_ts"),
            });
        }

        Ok(messages)
    }

    pub async fn delete_message(&self, message_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM voice_messages
            WHERE message_id = $1
            "#,
        )
        .bind(message_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_user_messages(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM voice_messages
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn delete_room_messages(&self, room_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM voice_messages
            WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn get_recent_messages(&self, limit: i64) -> Result<Vec<VoiceMessage>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM voice_messages
            ORDER BY created_ts DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(VoiceMessage {
                message_id: row.get("message_id"),
                room_id: row.get("room_id"),
                user_id: row.get("user_id"),
                duration_ms: row.get("duration_ms"),
                file_size: row.get("file_size"),
                content_type: row.get("content_type"),
                waveform_data: row.get("waveform_data"),
                file_path: row.get("file_path"),
                session_id: row.get("session_id"),
                created_ts: row.get("created_ts"),
            });
        }

        Ok(messages)
    }

    pub async fn get_messages_by_date_range(
        &self,
        user_id: &str,
        start_date: chrono::NaiveDate,
        end_date: chrono::NaiveDate,
    ) -> Result<Vec<VoiceMessage>, sqlx::Error> {
        let start_ts = start_date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let end_ts = end_date
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc()
            .timestamp_millis();

        let rows = sqlx::query(
            r#"
            SELECT * FROM voice_messages
            WHERE user_id = $1 AND created_ts >= $2 AND created_ts <= $3
            ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .bind(start_ts)
        .bind(end_ts)
        .fetch_all(&*self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(VoiceMessage {
                message_id: row.get("message_id"),
                room_id: row.get("room_id"),
                user_id: row.get("user_id"),
                duration_ms: row.get("duration_ms"),
                file_size: row.get("file_size"),
                content_type: row.get("content_type"),
                waveform_data: row.get("waveform_data"),
                file_path: row.get("file_path"),
                session_id: row.get("session_id"),
                created_ts: row.get("created_ts"),
            });
        }

        Ok(messages)
    }

    pub async fn get_message_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM voice_messages
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.get::<i64, _>("count"))
    }

    pub async fn get_room_message_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM voice_messages
            WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.get::<i64, _>("count"))
    }
}
