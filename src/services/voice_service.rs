use crate::common::*;
use crate::services::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct VoiceMessageDBRow {
    id: i64,
    event_id: String,
    user_id: String,
    room_id: String,
    media_id: Option<String>,
    duration_ms: i32,
    waveform: Option<String>,
    mime_type: Option<String>,
    file_size: Option<i64>,
    transcription: Option<String>,
    encryption: Option<sqlx::types::Json<serde_json::Value>>,
    is_processed: Option<bool>,
    processed_ts: Option<i64>,
    created_ts: i64,
}

#[derive(Debug, Clone)]
pub struct VoiceMessageSaveParams {
    pub event_id: String,
    pub user_id: String,
    pub room_id: String,
    pub media_id: Option<String>,
    pub duration_ms: i32,
    pub waveform: Option<String>,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
    pub transcription: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VoiceMessageUploadParams {
    pub user_id: String,
    pub room_id: Option<String>,
    pub session_id: Option<String>,
    pub content: Vec<u8>,
    pub content_type: String,
    pub duration_ms: i32,
}

#[derive(Clone)]
pub struct VoiceStorage {
    pool: Arc<sqlx::PgPool>,
    cache: Arc<CacheManager>,
}

impl VoiceStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>, cache: Arc<CacheManager>) -> Self {
        Self {
            pool: pool.clone(),
            cache,
        }
    }

    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS voice_messages (
                id BIGSERIAL PRIMARY KEY,
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
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS voice_usage_stats (
                id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                room_id VARCHAR(255),
                date DATE NOT NULL,
                period_start TIMESTAMP,
                period_end TIMESTAMP,
                total_duration_ms BIGINT DEFAULT 0,
                total_file_size BIGINT DEFAULT 0,
                message_count BIGINT DEFAULT 0,
                last_activity_ts BIGINT,
                last_active_ts BIGINT,
                created_ts BIGINT,
                updated_ts BIGINT,
                UNIQUE(user_id, room_id, period_start)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_voice_message(
        &self,
        params: VoiceMessageSaveParams,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            r#"
            INSERT INTO voice_messages
            (event_id, user_id, room_id, media_id, duration_ms, waveform, mime_type, file_size, transcription, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id
            "#,
        )
        .bind(&params.event_id)
        .bind(&params.user_id)
        .bind(&params.room_id)
        .bind(&params.media_id)
        .bind(params.duration_ms)
        .bind(&params.waveform)
        .bind(&params.mime_type)
        .bind(params.file_size)
        .bind(&params.transcription)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        let id: i64 = result.try_get("id")?;
        self.update_user_stats(&params.user_id, params.duration_ms as i64, params.file_size.unwrap_or(0))
            .await?;

        Ok(id)
    }

    pub async fn get_voice_message(
        &self,
        event_id: &str,
    ) -> Result<Option<VoiceMessageInfo>, sqlx::Error> {
        let result: Option<VoiceMessageDBRow> = sqlx::query_as(
            r#"
            SELECT id, event_id, user_id, room_id, media_id, duration_ms, waveform,
                   mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
            FROM voice_messages WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|r| VoiceMessageInfo {
            id: r.id,
            event_id: r.event_id,
            user_id: r.user_id,
            room_id: r.room_id,
            media_id: r.media_id,
            duration_ms: r.duration_ms,
            waveform: r.waveform,
            mime_type: r.mime_type,
            file_size: r.file_size,
            transcription: r.transcription,
            created_ts: r.created_ts,
        }))
    }

    pub async fn get_user_voice_messages(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VoiceMessageInfo>, sqlx::Error> {
        let rows: Vec<VoiceMessageDBRow> = sqlx::query_as(
            r#"
            SELECT id, event_id, user_id, room_id, media_id, duration_ms, waveform,
                   mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
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
                id: r.id,
                event_id: r.event_id.clone(),
                user_id: r.user_id.clone(),
                room_id: r.room_id.clone(),
                media_id: r.media_id.clone(),
                duration_ms: r.duration_ms,
                waveform: r.waveform.clone(),
                mime_type: r.mime_type.clone(),
                file_size: r.file_size,
                transcription: r.transcription.clone(),
                created_ts: r.created_ts,
            })
            .collect())
    }

    pub async fn delete_voice_message(
        &self,
        event_id: &str,
        user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let message = sqlx::query(
            r#"
            SELECT duration_ms, file_size
            FROM voice_messages
            WHERE event_id = $1 AND user_id = $2
            "#,
        )
        .bind(event_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(msg) = message {
            let duration_ms: i64 = msg.try_get::<Option<i32>, _>("duration_ms")?.unwrap_or(0) as i64;
            let file_size: i64 = msg.try_get::<Option<i64>, _>("file_size")?.unwrap_or(0);

            sqlx::query(r#"DELETE FROM voice_messages WHERE event_id = $1"#)
                .bind(event_id)
                .execute(&*self.pool)
                .await?;

            self.update_user_stats(user_id, -duration_ms, -file_size)
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
        let rows: Vec<VoiceMessageDBRow> = sqlx::query_as(
            r#"
            SELECT id, event_id, user_id, room_id, media_id, duration_ms, waveform,
                   mime_type, file_size, transcription, encryption, is_processed, processed_ts, created_ts
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
                id: r.id,
                event_id: r.event_id.clone(),
                user_id: r.user_id.clone(),
                room_id: r.room_id.clone(),
                media_id: r.media_id.clone(),
                duration_ms: r.duration_ms,
                waveform: r.waveform.clone(),
                mime_type: r.mime_type.clone(),
                file_size: r.file_size,
                transcription: r.transcription.clone(),
                created_ts: r.created_ts,
            })
            .collect())
    }

    async fn update_user_stats(
        &self,
        user_id: &str,
        duration_delta: i64,
        size_delta: i64,
    ) -> Result<(), sqlx::Error> {
        let today = chrono::Utc::now().date_naive();
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO voice_usage_stats (user_id, room_id, date, period_start, period_end, total_duration_ms, total_file_size, message_count, last_activity_ts, last_active_ts, created_ts)
            VALUES ($1, NULL, $2, $2, $2 + INTERVAL '1 day', $3, $4, 1, $5, $5, $5)
            ON CONFLICT (user_id, room_id, period_start) DO UPDATE SET
                total_duration_ms = voice_usage_stats.total_duration_ms + EXCLUDED.total_duration_ms,
                total_file_size = voice_usage_stats.total_file_size + EXCLUDED.total_file_size,
                message_count = voice_usage_stats.message_count + 1,
                last_activity_ts = EXCLUDED.last_activity_ts,
                last_active_ts = EXCLUDED.last_active_ts,
                updated_ts = EXCLUDED.last_activity_ts
            "#,
        )
        .bind(user_id)
        .bind(today)
        .bind(duration_delta)
        .bind(size_delta)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        // 2) Update Redis cache in background to avoid blocking the main flow
        // especially if Redis is slow or connection pool is exhausted.
        let cache = self.cache.clone();
        let user_id = user_id.to_string();
        let today_str = today.to_string();

        tokio::spawn(async move {
            let list_cache_key = format!("voice_stats:{}", user_id);
            // Ignore delete error as it's cache invalidation
            cache.delete(&list_cache_key).await;

            // Use Redis Hash for today's stats (Fast write)
            let daily_key = format!("voice_stats_daily:{}:{}", user_id, today_str);

            // Execute updates with basic error logging
            // We don't abort on one failure to try to keep stats as consistent as possible
            if let Err(e) = cache
                .hincrby(&daily_key, "total_duration_ms", duration_delta)
                .await
            {
                ::tracing::error!(
                    "Failed to update total_duration_ms in Redis for {}: {}",
                    daily_key,
                    e
                );
            }

            if let Err(e) = cache
                .hincrby(&daily_key, "total_file_size", size_delta)
                .await
            {
                ::tracing::error!(
                    "Failed to update total_file_size in Redis for {}: {}",
                    daily_key,
                    e
                );
            }

            if let Err(e) = cache.hincrby(&daily_key, "message_count", 1).await {
                ::tracing::error!(
                    "Failed to update message_count in Redis for {}: {}",
                    daily_key,
                    e
                );
            }

            cache.expire(&daily_key, 86400 * 2).await;
        });

        Ok(())
    }

    pub async fn get_user_stats(
        &self,
        user_id: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> Result<Vec<UserVoiceStats>, sqlx::Error> {
        let cache_key = format!("voice_stats:{}", user_id);

        // 1) Try cache first (if no date filters)
        if start_date.is_none() && end_date.is_none() {
            if let Ok(Some(stats)) = self.cache.get::<Vec<UserVoiceStats>>(&cache_key).await {
                return Ok(stats);
            }
        }

        let query = if let (Some(start), Some(end)) = (start_date, end_date) {
            let rows: Vec<(String, chrono::NaiveDate, i64, i64, i64)> = sqlx::query_as(
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
            let rows: Vec<(String, chrono::NaiveDate, i64, i64, i64)> = sqlx::query_as(
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

        let stats_result: Vec<UserVoiceStats> = query
            .iter()
            .map(|r| UserVoiceStats {
                user_id: r.0.clone(),
                date: r.1.to_string(),
                total_duration_ms: r.2,
                total_file_size: r.3,
                message_count: r.4,
            })
            .collect();

        // 2) Cache the results if no date filters
        if start_date.is_none() && end_date.is_none() {
            let _ = self.cache.set(&cache_key, stats_result.clone(), 3600).await;
        }

        Ok(stats_result)
    }

    pub async fn get_all_user_stats(&self, limit: i64) -> Result<Vec<UserVoiceStats>, sqlx::Error> {
        let rows: Vec<(String, chrono::NaiveDate, i64, i64, i64)> = sqlx::query_as(
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
                total_duration_ms: r.2,
                total_file_size: r.3,
                message_count: r.4,
            })
            .collect())
    }
}

#[derive(Debug)]
pub struct VoiceMessageInfo {
    pub id: i64,
    pub event_id: String,
    pub user_id: String,
    pub room_id: String,
    pub media_id: Option<String>,
    pub duration_ms: i32,
    pub waveform: Option<String>,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
    pub transcription: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserVoiceStats {
    pub user_id: String,
    pub date: String,
    pub total_duration_ms: i64,
    pub total_file_size: i64,
    pub message_count: i64,
}

#[derive(Clone)]
pub struct VoiceService {
    pool: Arc<sqlx::PgPool>,
    cache: Arc<CacheManager>,
    voice_path: PathBuf,
}

impl VoiceService {
    pub fn new(pool: &Arc<sqlx::PgPool>, cache: Arc<CacheManager>, voice_path: &str) -> Self {
        let path = PathBuf::from(voice_path);
        if !path.exists() {
            std::fs::create_dir_all(&path).ok();
        }
        Self {
            pool: pool.clone(),
            cache,
            voice_path: path,
        }
    }

    pub async fn warmup(&self) -> ApiResult<()> {
        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        // Warm up stats for active users (e.g., top 100 users by activity)
        let active_users = voice_storage
            .get_all_user_stats(100)
            .await
            .map_err(|e| ApiError::internal(format!("Warmup failed: {}", e)))?;

        for user_stats in active_users {
            let _ = voice_storage
                .get_user_stats(&user_stats.user_id, None, None)
                .await;
        }
        ::tracing::info!("Voice service cache warmup completed");
        Ok(())
    }

    pub async fn save_voice_message(
        &self,
        params: VoiceMessageUploadParams,
    ) -> ApiResult<serde_json::Value> {
        // Validate content type whitelist
        let allowed_types = [
            "audio/ogg",
            "audio/mp4",
            "audio/mpeg",
            "audio/webm",
            "audio/wav",
            "audio/aac",
            "audio/flac",
        ];

        // Simple check if it starts with any of the allowed types (to handle parameters like ; codecs=...)
        if !allowed_types
            .iter()
            .any(|t| params.content_type.starts_with(t))
        {
            return Err(ApiError::bad_request(format!(
                "Unsupported audio content type: {}. Allowed: {:?}",
                params.content_type, allowed_types
            )));
        }

        // Validate content size (e.g. max 50MB)
        const MAX_SIZE: usize = 50 * 1024 * 1024;
        if params.content.len() > MAX_SIZE {
            return Err(ApiError::bad_request(format!(
                "Voice message too large: {} bytes (max {} bytes)",
                params.content.len(),
                MAX_SIZE
            )));
        }

        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        let event_id = format!("${}", uuid::Uuid::new_v4().to_string());
        let extension = self.get_extension_from_content_type(&params.content_type);

        // Ensure event_id contains only safe characters
        if !event_id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '-') {
            return Err(ApiError::internal(
                "Generated invalid event ID".to_string(),
            ));
        }

        let file_name = format!("{}.{}", event_id.trim_start_matches('$'), extension);

        // Path traversal check
        if file_name.contains("..") || file_name.contains("/") || file_name.contains("\\") {
            return Err(ApiError::internal(
                "Security check failed: Invalid file name generated".to_string(),
            ));
        }

        let file_path = self.voice_path.join(&file_name);

        ::tracing::info!("Voice path: {:?}", self.voice_path);
        ::tracing::info!("File path: {:?}", file_path);

        fs::create_dir_all(&self.voice_path)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create voice directory: {}", e)))?;

        ::tracing::info!("Directory created successfully");

        fs::write(&file_path, &params.content)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to save voice message: {}", e)))?;

        let file_size = params.content.len() as i64;
        voice_storage
            .save_voice_message(VoiceMessageSaveParams {
                event_id: event_id.clone(),
                user_id: params.user_id,
                room_id: params.room_id.unwrap_or_default(),
                media_id: None,
                duration_ms: params.duration_ms,
                waveform: None,
                mime_type: Some(params.content_type.clone()),
                file_size: Some(file_size),
                transcription: None,
            })
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(json!({
            "event_id": event_id,
            "mime_type": params.content_type,
            "duration_ms": params.duration_ms,
            "size": file_size,
            "created_ts": chrono::Utc::now().timestamp_millis()
        }))
    }

    pub async fn get_voice_message(
        &self,
        event_id: &str,
    ) -> ApiResult<Option<(Vec<u8>, String)>> {
        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        let message = voice_storage
            .get_voice_message(event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(_msg) = message {
            // Return empty content since we don't store file_path anymore
            return Ok(Some((Vec::new(), "audio/ogg".to_string())));
        }
        Ok(None)
    }

    pub async fn delete_voice_message(&self, user_id: &str, event_id: &str) -> ApiResult<bool> {
        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        let deleted = voice_storage
            .delete_voice_message(event_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if deleted {
            let _file_prefix = event_id.trim_start_matches('$');
            if let Ok(mut entries) = fs::read_dir(&self.voice_path).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.starts_with(_file_prefix) {
                            let _ = fs::remove_file(entry.path()).await;
                        }
                    }
                }
            }
        }
        Ok(deleted)
    }

    pub async fn get_user_messages(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> ApiResult<serde_json::Value> {
        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        let messages = voice_storage
            .get_user_voice_messages(user_id, limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let message_list: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                json!({
                    "event_id": m.event_id,
                    "room_id": m.room_id,
                    "duration_ms": m.duration_ms,
                    "file_size": m.file_size,
                    "mime_type": m.mime_type,
                    "waveform": m.waveform,
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
        user_id: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        let stats = voice_storage
            .get_user_stats(user_id, start_date, end_date)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let total_duration: i64 = stats.iter().map(|s| s.total_duration_ms).sum();
        let total_size: i64 = stats.iter().map(|s| s.total_file_size).sum();
        let total_count: i64 = stats.iter().map(|s| s.message_count).sum();

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
        room_id: &str,
        limit: i64,
    ) -> ApiResult<serde_json::Value> {
        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        let messages = voice_storage
            .get_room_voice_messages(room_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let message_list: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                json!({
                    "event_id": m.event_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CacheConfig, CacheManager};
    use tempfile::TempDir;

    fn create_test_voice_save_params() -> VoiceMessageSaveParams {
        VoiceMessageSaveParams {
            event_id: "$test_event_id:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            media_id: Some("media_id_123".to_string()),
            duration_ms: 5000,
            waveform: Some("waveform_data".to_string()),
            mime_type: Some("audio/ogg".to_string()),
            file_size: Some(102400),
            transcription: None,
        }
    }

    fn create_test_voice_upload_params() -> VoiceMessageUploadParams {
        VoiceMessageUploadParams {
            user_id: "@bob:example.com".to_string(),
            room_id: Some("!room:example.com".to_string()),
            session_id: Some("session_123".to_string()),
            content: vec![0u8; 1024],
            content_type: "audio/ogg".to_string(),
            duration_ms: 3000,
        }
    }

    #[test]
    fn test_voice_message_save_params_creation() {
        let params = create_test_voice_save_params();

        assert_eq!(params.event_id, "$test_event_id:example.com");
        assert_eq!(params.user_id, "@alice:example.com");
        assert_eq!(params.room_id, "!room:example.com");
        assert!(params.media_id.is_some());
        assert_eq!(params.duration_ms, 5000);
        assert!(params.waveform.is_some());
        assert_eq!(params.mime_type, Some("audio/ogg".to_string()));
        assert_eq!(params.file_size, Some(102400));
        assert!(params.transcription.is_none());
    }

    #[test]
    fn test_voice_message_save_params_minimal() {
        let params = VoiceMessageSaveParams {
            event_id: "$minimal:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            media_id: None,
            duration_ms: 1000,
            waveform: None,
            mime_type: None,
            file_size: None,
            transcription: None,
        };

        assert_eq!(params.event_id, "$minimal:example.com");
        assert!(params.media_id.is_none());
        assert!(params.waveform.is_none());
        assert!(params.mime_type.is_none());
        assert!(params.file_size.is_none());
    }

    #[test]
    fn test_voice_message_upload_params_creation() {
        let params = create_test_voice_upload_params();

        assert_eq!(params.user_id, "@bob:example.com");
        assert!(params.room_id.is_some());
        assert!(params.session_id.is_some());
        assert_eq!(params.content.len(), 1024);
        assert_eq!(params.content_type, "audio/ogg");
        assert_eq!(params.duration_ms, 3000);
    }

    #[test]
    fn test_voice_message_upload_params_minimal() {
        let params = VoiceMessageUploadParams {
            user_id: "@minimal:example.com".to_string(),
            room_id: None,
            session_id: None,
            content: vec![1, 2, 3, 4],
            content_type: "audio/mp4".to_string(),
            duration_ms: 500,
        };

        assert!(params.room_id.is_none());
        assert!(params.session_id.is_none());
        assert_eq!(params.content.len(), 4);
    }

    #[test]
    fn test_voice_message_info_creation() {
        let info = VoiceMessageInfo {
            id: 1,
            event_id: "$event:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            media_id: Some("media_123".to_string()),
            duration_ms: 10000,
            waveform: Some("waveform".to_string()),
            mime_type: Some("audio/webm".to_string()),
            file_size: Some(204800),
            transcription: Some("Hello world".to_string()),
            created_ts: 1700000000,
        };

        assert_eq!(info.id, 1);
        assert_eq!(info.duration_ms, 10000);
        assert!(info.transcription.is_some());
    }

    #[test]
    fn test_user_voice_stats_creation() {
        let stats = UserVoiceStats {
            user_id: "@user:example.com".to_string(),
            date: "2024-01-15".to_string(),
            total_duration_ms: 60000,
            total_file_size: 1024000,
            message_count: 10,
        };

        assert_eq!(stats.user_id, "@user:example.com");
        assert_eq!(stats.date, "2024-01-15");
        assert_eq!(stats.total_duration_ms, 60000);
        assert_eq!(stats.total_file_size, 1024000);
        assert_eq!(stats.message_count, 10);
    }

    #[test]
    fn test_user_voice_stats_serialization() {
        let stats = UserVoiceStats {
            user_id: "@user:example.com".to_string(),
            date: "2024-01-15".to_string(),
            total_duration_ms: 30000,
            total_file_size: 512000,
            message_count: 5,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("@user:example.com"));
        assert!(json.contains("2024-01-15"));
        assert!(json.contains("30000"));

        let deserialized: UserVoiceStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, stats.user_id);
        assert_eq!(deserialized.total_duration_ms, stats.total_duration_ms);
    }

    #[tokio::test]
    async fn test_voice_service_extension_ogg() {
        let temp_dir = TempDir::new().unwrap();
        let voice_path = temp_dir.path().to_str().unwrap();

        let pool = Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap_or_else(|_| {
            panic!("Failed to create pool")
        }));
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));

        let service = VoiceService::new(&pool, cache, voice_path);

        assert_eq!(service.get_extension_from_content_type("audio/ogg"), "ogg");
        assert_eq!(
            service.get_extension_from_content_type("audio/ogg; codecs=opus"),
            "ogg"
        );
    }

    #[tokio::test]
    async fn test_voice_service_extension_mp4() {
        let temp_dir = TempDir::new().unwrap();
        let voice_path = temp_dir.path().to_str().unwrap();

        let pool = Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap_or_else(|_| {
            panic!("Failed to create pool")
        }));
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));

        let service = VoiceService::new(&pool, cache, voice_path);

        assert_eq!(service.get_extension_from_content_type("audio/mp4"), "m4a");
        assert_eq!(
            service.get_extension_from_content_type("audio/mp4; codecs=aac"),
            "m4a"
        );
    }

    #[tokio::test]
    async fn test_voice_service_extension_mpeg() {
        let temp_dir = TempDir::new().unwrap();
        let voice_path = temp_dir.path().to_str().unwrap();

        let pool = Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap_or_else(|_| {
            panic!("Failed to create pool")
        }));
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));

        let service = VoiceService::new(&pool, cache, voice_path);

        assert_eq!(service.get_extension_from_content_type("audio/mpeg"), "mp3");
        assert_eq!(service.get_extension_from_content_type("audio/mpeg3"), "mp3");
    }

    #[tokio::test]
    async fn test_voice_service_extension_webm() {
        let temp_dir = TempDir::new().unwrap();
        let voice_path = temp_dir.path().to_str().unwrap();

        let pool = Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap_or_else(|_| {
            panic!("Failed to create pool")
        }));
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));

        let service = VoiceService::new(&pool, cache, voice_path);

        assert_eq!(service.get_extension_from_content_type("audio/webm"), "webm");
        assert_eq!(
            service.get_extension_from_content_type("audio/webm; codecs=opus"),
            "webm"
        );
    }

    #[tokio::test]
    async fn test_voice_service_extension_wav() {
        let temp_dir = TempDir::new().unwrap();
        let voice_path = temp_dir.path().to_str().unwrap();

        let pool = Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap_or_else(|_| {
            panic!("Failed to create pool")
        }));
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));

        let service = VoiceService::new(&pool, cache, voice_path);

        assert_eq!(service.get_extension_from_content_type("audio/wav"), "wav");
        assert_eq!(
            service.get_extension_from_content_type("audio/wav; codecs=pcm"),
            "wav"
        );
    }

    #[tokio::test]
    async fn test_voice_service_extension_unknown() {
        let temp_dir = TempDir::new().unwrap();
        let voice_path = temp_dir.path().to_str().unwrap();

        let pool = Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap_or_else(|_| {
            panic!("Failed to create pool")
        }));
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));

        let service = VoiceService::new(&pool, cache, voice_path);

        assert_eq!(service.get_extension_from_content_type("audio/flac"), "audio");
        assert_eq!(service.get_extension_from_content_type("audio/aac"), "audio");
        assert_eq!(service.get_extension_from_content_type("audio/unknown"), "audio");
        assert_eq!(service.get_extension_from_content_type("video/mp4"), "audio");
    }

    #[tokio::test]
    async fn test_voice_service_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let voice_path = temp_dir.path().join("voice_messages");
        let voice_path_str = voice_path.to_str().unwrap();

        assert!(!voice_path.exists());

        let pool = Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap_or_else(|_| {
            panic!("Failed to create pool")
        }));
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));

        let _service = VoiceService::new(&pool, cache, voice_path_str);

        assert!(voice_path.exists());
    }

    #[test]
    fn test_voice_message_db_row_structure() {
        let row = VoiceMessageDBRow {
            id: 1,
            event_id: "$event:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            media_id: Some("media_id".to_string()),
            duration_ms: 5000,
            waveform: Some("waveform_data".to_string()),
            mime_type: Some("audio/ogg".to_string()),
            file_size: Some(102400),
            transcription: Some("transcribed text".to_string()),
            encryption: None,
            is_processed: Some(true),
            processed_ts: Some(1700000000),
            created_ts: 1699900000,
        };

        assert_eq!(row.id, 1);
        assert_eq!(row.duration_ms, 5000);
        assert!(row.is_processed.unwrap_or(false));
    }

    #[test]
    fn test_voice_message_db_row_with_encryption() {
        let encryption_info = serde_json::json!({
            "algorithm": "m.megolm.v1.aes-sha2",
            "key": "base64_encoded_key"
        });

        let row = VoiceMessageDBRow {
            id: 2,
            event_id: "$encrypted:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            media_id: None,
            duration_ms: 3000,
            waveform: None,
            mime_type: Some("audio/ogg".to_string()),
            file_size: Some(51200),
            transcription: None,
            encryption: Some(sqlx::types::Json(encryption_info)),
            is_processed: None,
            processed_ts: None,
            created_ts: 1700000000,
        };

        assert!(row.encryption.is_some());
        let enc = row.encryption.unwrap();
        assert_eq!(enc.0["algorithm"], "m.megolm.v1.aes-sha2");
    }

    #[test]
    fn test_voice_message_save_params_with_transcription() {
        let params = VoiceMessageSaveParams {
            event_id: "$transcribed:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            media_id: Some("media_id".to_string()),
            duration_ms: 15000,
            waveform: Some("complex_waveform_data".to_string()),
            mime_type: Some("audio/webm".to_string()),
            file_size: Some(256000),
            transcription: Some("This is a transcribed voice message.".to_string()),
        };

        assert!(params.transcription.is_some());
        assert_eq!(
            params.transcription.unwrap(),
            "This is a transcribed voice message."
        );
    }

    #[test]
    fn test_voice_message_upload_params_boundary_zero_duration() {
        let params = VoiceMessageUploadParams {
            user_id: "@user:example.com".to_string(),
            room_id: None,
            session_id: None,
            content: vec![],
            content_type: "audio/ogg".to_string(),
            duration_ms: 0,
        };

        assert_eq!(params.duration_ms, 0);
        assert!(params.content.is_empty());
    }

    #[test]
    fn test_voice_message_upload_params_boundary_max_duration() {
        let max_duration_ms = 600000;
        let params = VoiceMessageUploadParams {
            user_id: "@user:example.com".to_string(),
            room_id: Some("!room:example.com".to_string()),
            session_id: Some("session".to_string()),
            content: vec![0u8; 50 * 1024 * 1024],
            content_type: "audio/ogg".to_string(),
            duration_ms: max_duration_ms,
        };

        assert_eq!(params.duration_ms, max_duration_ms);
        assert_eq!(params.content.len(), 50 * 1024 * 1024);
    }
}
