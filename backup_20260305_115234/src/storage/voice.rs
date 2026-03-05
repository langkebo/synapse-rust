use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VoiceMessage {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub media_id: Option<String>,
    pub duration_ms: i32,
    pub waveform: Option<String>,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
    pub transcription: Option<String>,
    pub encryption: Option<serde_json::Value>,
    pub is_processed: Option<bool>,
    pub processed_ts: Option<i64>,
    pub created_ts: i64,
}

#[derive(Debug, Clone)]
pub struct CreateVoiceMessage {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub media_id: Option<String>,
    pub duration_ms: i32,
    pub waveform: Option<String>,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
    pub transcription: Option<String>,
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

impl VoiceMessage {
    fn from_row(row: &sqlx::postgres::PgRow) -> Self {
        Self {
            id: row.get("id"),
            event_id: row.get("event_id"),
            room_id: row.get("room_id"),
            user_id: row.get("user_id"),
            media_id: row.get("media_id"),
            duration_ms: row.get("duration_ms"),
            waveform: row.get("waveform"),
            mime_type: row.get("mime_type"),
            file_size: row.get("file_size"),
            transcription: row.get("transcription"),
            encryption: row.get("encryption"),
            is_processed: row.get("is_processed"),
            processed_ts: row.get("processed_ts"),
            created_ts: row.get("created_ts"),
        }
    }
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
        let date_threshold = chrono::Utc::now() - chrono::Duration::days(days);

        let stats = sqlx::query_as::<_, VoiceUsageStats>(
            r#"
            SELECT id, user_id, date, message_count, total_duration_ms, total_file_size, created_ts, updated_ts
            FROM voice_usage_stats
            WHERE user_id = $1 AND date >= $2
            ORDER BY date DESC
            "#,
        )
        .bind(user_id)
        .bind(date_threshold.naive_utc())
        .fetch_all(&*self.pool)
        .await?;

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
            INSERT INTO voice_messages (event_id, room_id, user_id, media_id, duration_ms, waveform, mime_type, file_size, transcription, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, event_id, room_id, user_id, media_id, duration_ms, waveform, mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
            "#,
        )
        .bind(&message.event_id)
        .bind(&message.room_id)
        .bind(&message.user_id)
        .bind(&message.media_id)
        .bind(message.duration_ms)
        .bind(&message.waveform)
        .bind(&message.mime_type)
        .bind(message.file_size)
        .bind(&message.transcription)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(VoiceMessage::from_row(&row))
    }

    pub async fn get_message(&self, event_id: &str) -> Result<Option<VoiceMessage>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, event_id, room_id, user_id, media_id, duration_ms, waveform, mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
            FROM voice_messages
            WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.as_ref().map(VoiceMessage::from_row))
    }

    pub async fn get_user_messages(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VoiceMessage>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, event_id, room_id, user_id, media_id, duration_ms, waveform, mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
            FROM voice_messages
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

        Ok(rows.iter().map(VoiceMessage::from_row).collect())
    }

    pub async fn get_room_messages(
        &self,
        room_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VoiceMessage>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, event_id, room_id, user_id, media_id, duration_ms, waveform, mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
            FROM voice_messages
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

        Ok(rows.iter().map(VoiceMessage::from_row).collect())
    }

    pub async fn get_session_messages(
        &self,
        _session_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VoiceMessage>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, event_id, room_id, user_id, media_id, duration_ms, waveform, mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
            FROM voice_messages
            ORDER BY created_ts DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.iter().map(VoiceMessage::from_row).collect())
    }

    pub async fn delete_message(&self, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM voice_messages
            WHERE event_id = $1
            "#,
        )
        .bind(event_id)
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
            SELECT id, event_id, room_id, user_id, media_id, duration_ms, waveform, mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
            FROM voice_messages
            ORDER BY created_ts DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.iter().map(VoiceMessage::from_row).collect())
    }

    pub async fn get_messages_by_date_range(
        &self,
        user_id: &str,
        start_date: chrono::NaiveDate,
        end_date: chrono::NaiveDate,
    ) -> Result<Vec<VoiceMessage>, sqlx::Error> {
        let start_ts = start_date
            .and_hms_opt(0, 0, 0)
            .expect("Invalid start time constant")
            .and_utc()
            .timestamp_millis();
        let end_ts = end_date
            .and_hms_opt(23, 59, 59)
            .expect("Invalid end time constant")
            .and_utc()
            .timestamp_millis();

        let rows = sqlx::query(
            r#"
            SELECT id, event_id, room_id, user_id, media_id, duration_ms, waveform, mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
            FROM voice_messages
            WHERE user_id = $1 AND created_ts >= $2 AND created_ts <= $3
            ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .bind(start_ts)
        .bind(end_ts)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.iter().map(VoiceMessage::from_row).collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_message_creation() {
        let voice_message = VoiceMessage {
            id: 1,
            event_id: "$event123:example.com".to_string(),
            room_id: "!room123:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            media_id: Some("media123".to_string()),
            duration_ms: 5000,
            waveform: Some("waveform_data".to_string()),
            mime_type: Some("audio/ogg".to_string()),
            file_size: Some(1024000),
            transcription: Some("Hello world".to_string()),
            encryption: None,
            is_processed: Some(true),
            processed_ts: Some(1234567890),
            created_ts: 1234567800,
        };

        assert_eq!(voice_message.id, 1);
        assert_eq!(voice_message.event_id, "$event123:example.com");
        assert!(voice_message.media_id.is_some());
        assert_eq!(voice_message.duration_ms, 5000);
        assert_eq!(voice_message.user_id, "@alice:example.com");
    }

    #[test]
    fn test_voice_message_optional_fields() {
        let voice_message = VoiceMessage {
            id: 2,
            event_id: "$event456:example.com".to_string(),
            room_id: "!room456:example.com".to_string(),
            user_id: "@bob:example.com".to_string(),
            media_id: None,
            duration_ms: 3000,
            waveform: None,
            mime_type: None,
            file_size: None,
            transcription: None,
            encryption: None,
            is_processed: None,
            processed_ts: None,
            created_ts: 1234567900,
        };

        assert!(voice_message.media_id.is_none());
        assert!(voice_message.waveform.is_none());
        assert!(voice_message.is_processed.is_none());
    }

    #[test]
    fn test_create_voice_message() {
        let create_msg = CreateVoiceMessage {
            event_id: "$new_event:example.com".to_string(),
            room_id: "!new_room:example.com".to_string(),
            user_id: "@charlie:example.com".to_string(),
            media_id: Some("media456".to_string()),
            duration_ms: 10000,
            waveform: Some("waveform_abc".to_string()),
            mime_type: Some("audio/webm".to_string()),
            file_size: Some(2048000),
            transcription: Some("Test transcription".to_string()),
        };

        assert_eq!(create_msg.event_id, "$new_event:example.com");
        assert_eq!(create_msg.duration_ms, 10000);
    }

    #[test]
    fn test_create_voice_message_minimal() {
        let create_msg = CreateVoiceMessage {
            event_id: "$min_event:example.com".to_string(),
            room_id: "!min_room:example.com".to_string(),
            user_id: "@dave:example.com".to_string(),
            media_id: None,
            duration_ms: 0,
            waveform: None,
            mime_type: None,
            file_size: None,
            transcription: None,
        };

        assert_eq!(create_msg.duration_ms, 0);
        assert!(create_msg.media_id.is_none());
    }

    #[test]
    fn test_voice_usage_stats() {
        let stats = VoiceUsageStats {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 2, 26).unwrap(),
            message_count: 10,
            total_duration_ms: 50000,
            total_file_size: 10240000,
            created_ts: 1234567800,
            updated_ts: 1234567900,
        };

        assert_eq!(stats.user_id, "@alice:example.com");
        assert_eq!(stats.message_count, 10);
    }

    #[test]
    fn test_voice_usage_stats_zero_values() {
        let stats = VoiceUsageStats {
            id: 2,
            user_id: "@bob:example.com".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            message_count: 0,
            total_duration_ms: 0,
            total_file_size: 0,
            created_ts: 0,
            updated_ts: 0,
        };

        assert_eq!(stats.message_count, 0);
        assert_eq!(stats.total_duration_ms, 0);
    }

    #[test]
    fn test_voice_message_encryption() {
        let encryption_data = serde_json::json!({
            "algorithm": "m.olm.v1.curve25519-aes-sha2",
            "sender_key": "sender_key_value"
        });

        let voice_message = VoiceMessage {
            id: 1,
            event_id: "$encrypted_event:example.com".to_string(),
            room_id: "!room123:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            media_id: Some("encrypted_media".to_string()),
            duration_ms: 5000,
            waveform: None,
            mime_type: Some("audio/ogg".to_string()),
            file_size: Some(1024000),
            transcription: None,
            encryption: Some(encryption_data),
            is_processed: Some(true),
            processed_ts: Some(1234567890),
            created_ts: 1234567800,
        };

        assert!(voice_message.encryption.is_some());
    }
}
