use crate::common::*;
use crate::services::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value as JsonValue;
use sqlx::Row;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct VoiceStorage {
    pool: Arc<sqlx::PgPool>,
}

impl VoiceStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS voice_messages (
                id SERIAL PRIMARY KEY,
                message_id VARCHAR(255) NOT NULL UNIQUE,
                user_id VARCHAR(255) NOT NULL,
                room_id VARCHAR(255),
                session_id VARCHAR(255),
                file_path VARCHAR(512) NOT NULL,
                content_type VARCHAR(100) NOT NULL,
                duration_ms INT NOT NULL,
                file_size BIGINT NOT NULL,
                waveform_data TEXT,
                transcribe_text TEXT,
                created_ts BIGINT NOT NULL
            )
            "#
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS voice_usage_stats (
                id SERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                date DATE NOT NULL,
                total_duration_ms INT DEFAULT 0,
                total_file_size BIGINT DEFAULT 0,
                message_count INT DEFAULT 0,
                UNIQUE(user_id, date)
            )
            "#
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_voice_message(
        &self,
        message_id: &str,
        user_id: &str,
        room_id: Option<&str>,
        session_id: Option<&str>,
        file_path: &str,
        content_type: &str,
        duration_ms: i32,
        file_size: i64,
        waveform_data: Option<JsonValue>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let waveform_json: Option<String> = waveform_data.as_ref().map(|v| v.to_string());
        let result = sqlx::query(
            r#"
            INSERT INTO voice_messages
            (message_id, user_id, room_id, session_id, file_path, content_type, duration_ms, file_size, waveform_data, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id
            "#,
        )
        .bind(message_id)
        .bind(user_id)
        .bind(room_id)
        .bind(session_id)
        .bind(file_path)
        .bind(content_type)
        .bind(duration_ms)
        .bind(file_size)
        .bind(waveform_json)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        let id: i64 = result.try_get("id")?;
        self.update_user_stats(user_id, duration_ms, file_size)
            .await?;

        Ok(id)
    }

    pub async fn get_voice_message(
        &self,
        message_id: &str,
    ) -> Result<Option<VoiceMessageInfo>, sqlx::Error> {
        let result: Option<(
            i64,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            i32,
            i64,
            Option<Vec<u8>>,
            Option<String>,
            i64,
        )> = sqlx::query_as(
            r#"
            SELECT id, message_id, user_id, room_id, session_id, file_path, content_type,
                   duration_ms, file_size, waveform_data, transcribe_text, created_ts
            FROM voice_messages WHERE message_id = $1
            "#,
        )
        .bind(message_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|r| VoiceMessageInfo {
            id: r.0,
            message_id: r.1,
            user_id: r.2,
            room_id: Some(r.3),
            session_id: r.4,
            file_path: r.5,
            content_type: r.6,
            duration_ms: r.7,
            file_size: r.8,
            waveform_data: r.9.map(|v| String::from_utf8_lossy(&v).to_string()),
            transcribe_text: r.10,
            created_ts: r.11,
        }))
    }

    pub async fn get_user_voice_messages(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VoiceMessageInfo>, sqlx::Error> {
        let rows: Vec<(
            i64,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            i32,
            i64,
            Option<Vec<u8>>,
            Option<String>,
            i64,
        )> = sqlx::query_as(
            r#"
            SELECT id, message_id, user_id, room_id, session_id, file_path, content_type,
                   duration_ms, file_size, waveform_data, transcribe_text, created_ts
            FROM voice_messages WHERE user_id = $1
            ORDER BY created_ts DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| VoiceMessageInfo {
                id: r.0,
                message_id: r.1.clone(),
                user_id: r.2.clone(),
                room_id: Some(r.3.clone()),
                session_id: r.4.clone(),
                file_path: r.5.clone(),
                content_type: r.6.clone(),
                duration_ms: r.7,
                file_size: r.8,
                waveform_data: r.9.clone().map(|v| String::from_utf8_lossy(&v).to_string()),
                transcribe_text: r.10.clone(),
                created_ts: r.11,
            })
            .collect())
    }

    pub async fn delete_voice_message(
        &self,
        message_id: &str,
        user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let message = sqlx::query!(
            r#"SELECT duration_ms, file_size FROM voice_messages WHERE message_id = $1 AND user_id = $2"#,
            message_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(msg) = message {
            sqlx::query!(
                r#"DELETE FROM voice_messages WHERE message_id = $1"#,
                message_id
            )
            .execute(&*self.pool)
            .await?;

            self.update_user_stats(user_id, -msg.duration_ms, -msg.file_size)
                .await?;
            return Ok(true);
        }
        Ok(false)
    }

    pub async fn get_room_voice_messages(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<VoiceMessageInfo>, sqlx::Error> {
        let rows: Vec<(
            i64,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            i32,
            i64,
            Option<Vec<u8>>,
            Option<String>,
            i64,
        )> = sqlx::query_as(
            r#"
            SELECT id, message_id, user_id, room_id, session_id, file_path, content_type,
                   duration_ms, file_size, waveform_data, transcribe_text, created_ts
            FROM voice_messages WHERE room_id = $1
            ORDER BY created_ts DESC
            LIMIT $2
            "#,
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| VoiceMessageInfo {
                id: r.0,
                message_id: r.1.clone(),
                user_id: r.2.clone(),
                room_id: Some(r.3.clone()),
                session_id: r.4.clone(),
                file_path: r.5.clone(),
                content_type: r.6.clone(),
                duration_ms: r.7,
                file_size: r.8,
                waveform_data: r.9.as_ref().map(|v| String::from_utf8_lossy(v).to_string()),
                transcribe_text: r.10.clone(),
                created_ts: r.11,
            })
            .collect())
    }

    async fn update_user_stats(
        &self,
        user_id: &str,
        duration_delta: i32,
        size_delta: i64,
    ) -> Result<(), sqlx::Error> {
        let today = chrono::Utc::now().date_naive();
        sqlx::query!(
            r#"
            INSERT INTO voice_usage_stats (user_id, date, total_duration_ms, total_file_size, message_count)
            VALUES ($1, $2, $3, $4, 1)
            ON CONFLICT (user_id, date) DO UPDATE SET
                total_duration_ms = voice_usage_stats.total_duration_ms + EXCLUDED.total_duration_ms,
                total_file_size = voice_usage_stats.total_file_size + EXCLUDED.total_file_size,
                message_count = voice_usage_stats.message_count + 1
            "#,
            user_id, today, duration_delta, size_delta
        ).execute(&*self.pool).await?;
        Ok(())
    }

    pub async fn get_user_stats(
        &self,
        user_id: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> Result<Vec<UserVoiceStats>, sqlx::Error> {
        let query = if let (Some(start), Some(end)) = (start_date, end_date) {
            let rows: Vec<(String, chrono::NaiveDate, i64, i64, i32)> = sqlx::query_as(
                r#"
                SELECT user_id, date, total_duration_ms, total_file_size, message_count
                FROM voice_usage_stats
                WHERE user_id = $1 AND date BETWEEN $2 AND $3
                ORDER BY date DESC
                "#,
            )
            .bind(user_id)
            .bind(start)
            .bind(end)
            .fetch_all(&*self.pool)
            .await?;
            rows
        } else {
            let rows: Vec<(String, chrono::NaiveDate, i64, i64, i32)> = sqlx::query_as(
                r#"
                SELECT user_id, date, total_duration_ms, total_file_size, message_count
                FROM voice_usage_stats
                WHERE user_id = $1
                ORDER BY date DESC
                "#,
            )
            .bind(user_id)
            .fetch_all(&*self.pool)
            .await?;
            rows
        };

        Ok(query
            .iter()
            .map(|r| UserVoiceStats {
                user_id: r.0.clone(),
                date: r.1.to_string(),
                total_duration_ms: r.2 as i32,
                total_file_size: r.3,
                message_count: r.4,
            })
            .collect())
    }

    pub async fn get_all_user_stats(&self, limit: i64) -> Result<Vec<UserVoiceStats>, sqlx::Error> {
        let rows: Vec<(String, chrono::NaiveDate, i64, i64, i32)> = sqlx::query_as(
            r#"
            SELECT user_id, date, total_duration_ms, total_file_size, message_count
            FROM voice_usage_stats
            WHERE date = CURRENT_DATE
            ORDER BY total_duration_ms DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| UserVoiceStats {
                user_id: r.0.clone(),
                date: r.1.to_string(),
                total_duration_ms: r.2 as i32,
                total_file_size: r.3,
                message_count: r.4,
            })
            .collect())
    }
}

#[derive(Debug)]
pub struct VoiceMessageInfo {
    pub id: i64,
    pub message_id: String,
    pub user_id: String,
    pub room_id: Option<String>,
    pub session_id: Option<String>,
    pub file_path: String,
    pub content_type: String,
    pub duration_ms: i32,
    pub file_size: i64,
    pub waveform_data: Option<String>,
    pub transcribe_text: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserVoiceStats {
    pub user_id: String,
    pub date: String,
    pub total_duration_ms: i32,
    pub total_file_size: i64,
    pub message_count: i32,
}

#[derive(Clone)]
pub struct VoiceService {
    voice_path: PathBuf,
}

impl VoiceService {
    pub fn new(voice_path: &str) -> Self {
        let path = PathBuf::from(voice_path);
        if !path.exists() {
            std::fs::create_dir_all(&path).ok();
        }
        Self { voice_path: path }
    }

    pub async fn save_voice_message(
        &self,
        pool: &Arc<sqlx::PgPool>,
        user_id: &str,
        room_id: Option<&str>,
        session_id: Option<&str>,
        content: &[u8],
        content_type: &str,
        duration_ms: i32,
    ) -> ApiResult<serde_json::Value> {
        let voice_storage = VoiceStorage::new(pool);
        let message_id = format!("vm_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        let extension = self.get_extension_from_content_type(content_type);
        let file_name = format!("{}.{}", message_id, extension);
        let file_path = self.voice_path.join(&file_name);

        std::fs::write(&file_path, content)
            .map_err(|e| ApiError::internal(format!("Failed to save voice message: {}", e)))?;

        let file_size = content.len() as i64;
        voice_storage
            .save_voice_message(
                &message_id,
                user_id,
                room_id,
                session_id,
                &file_path.to_string_lossy(),
                content_type,
                duration_ms,
                file_size,
                None,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(json!({
            "message_id": message_id,
            "content_type": content_type,
            "duration_ms": duration_ms,
            "size": file_size,
            "created_ts": chrono::Utc::now().timestamp_millis()
        }))
    }

    pub async fn get_voice_message(
        &self,
        pool: &Arc<sqlx::PgPool>,
        message_id: &str,
    ) -> ApiResult<Option<(Vec<u8>, String)>> {
        let voice_storage = VoiceStorage::new(pool);
        let message = voice_storage
            .get_voice_message(message_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(msg) = message {
            if let Ok(content) = std::fs::read(&msg.file_path) {
                return Ok(Some((content, msg.content_type)));
            }
        }
        Ok(None)
    }

    pub async fn delete_voice_message(
        &self,
        pool: &Arc<sqlx::PgPool>,
        user_id: &str,
        message_id: &str,
    ) -> ApiResult<bool> {
        let voice_storage = VoiceStorage::new(pool);
        let deleted = voice_storage
            .delete_voice_message(message_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if deleted {
            let _file_path = self.voice_path.join(format!("{}.*", message_id));
            if let Ok(entries) = std::fs::read_dir(&self.voice_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.starts_with(message_id) {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }
        Ok(deleted)
    }

    pub async fn get_user_messages(
        &self,
        pool: &Arc<sqlx::PgPool>,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> ApiResult<serde_json::Value> {
        let voice_storage = VoiceStorage::new(pool);
        let messages = voice_storage
            .get_user_voice_messages(user_id, limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let message_list: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                json!({
                    "message_id": m.message_id,
                    "room_id": m.room_id,
                    "duration_ms": m.duration_ms,
                    "file_size": m.file_size,
                    "content_type": m.content_type,
                    "waveform_data": m.waveform_data,
                    "created_ts": m.created_ts
                })
            })
            .collect();

        Ok(json!({
            "messages": message_list,
            "count": message_list.len()
        }))
    }

    pub async fn get_user_stats(
        &self,
        pool: &Arc<sqlx::PgPool>,
        user_id: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let voice_storage = VoiceStorage::new(pool);
        let stats = voice_storage
            .get_user_stats(user_id, start_date, end_date)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let total_duration: i32 = stats.iter().map(|s| s.total_duration_ms).sum();
        let total_size: i64 = stats.iter().map(|s| s.total_file_size).sum();
        let total_count: i32 = stats.iter().map(|s| s.message_count).sum();

        Ok(json!({
            "user_id": user_id,
            "total_duration_ms": total_duration,
            "total_file_size": total_size,
            "total_message_count": total_count,
            "daily_stats": stats
        }))
    }

    pub async fn get_room_messages(
        &self,
        pool: &Arc<sqlx::PgPool>,
        room_id: &str,
        limit: i64,
    ) -> ApiResult<serde_json::Value> {
        let voice_storage = VoiceStorage::new(pool);
        let messages = voice_storage
            .get_room_voice_messages(room_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let message_list: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                json!({
                    "message_id": m.message_id,
                    "user_id": m.user_id,
                    "duration_ms": m.duration_ms,
                    "file_size": m.file_size,
                    "created_ts": m.created_ts
                })
            })
            .collect();

        Ok(json!({
            "messages": message_list,
            "count": message_list.len()
        }))
    }

    fn get_extension_from_content_type(&self, content_type: &str) -> &str {
        if content_type.starts_with("audio/ogg") {
            "ogg"
        } else if content_type.starts_with("audio/mp4") {
            "m4a"
        } else if content_type.starts_with("audio/mpeg") {
            "mp3"
        } else if content_type.starts_with("audio/webm") {
            "webm"
        } else if content_type.starts_with("audio/wav") {
            "wav"
        } else {
            "audio"
        }
    }
}
