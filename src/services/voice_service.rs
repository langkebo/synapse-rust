use crate::common::*;
use crate::services::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value as JsonValue;
use sqlx::Row;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

type VoiceMessageRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<i64>,
    Option<i64>,
    Option<sqlx::types::Json<serde_json::Value>>,
    i64,
    Option<String>,
    Option<bool>,
    Option<i64>,
    Option<String>,
    Option<sqlx::types::Json<serde_json::Value>>,
);

#[derive(Debug, Clone)]
pub struct VoiceMessageSaveParams {
    pub message_id: String,
    pub user_id: String,
    pub room_id: Option<String>,
    pub session_id: Option<String>,
    pub file_path: String,
    pub content_type: String,
    pub duration_ms: i32,
    pub file_size: i64,
    pub waveform_data: Option<JsonValue>,
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
                date DATE NOT NULL,
                total_duration_ms BIGINT DEFAULT 0,
                total_file_size BIGINT DEFAULT 0,
                message_count INT DEFAULT 0,
                UNIQUE(user_id, date)
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
        let waveform_json: Option<sqlx::types::Json<serde_json::Value>> = params
            .waveform_data
            .as_ref()
            .map(|v| sqlx::types::Json(v.clone()));
        let result = sqlx::query(
            r#"
            INSERT INTO voice_messages
            (message_id, user_id, room_id, session_id, file_path, content_type, duration_ms, file_size, waveform_data, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id
            "#,
        )
        .bind(&params.message_id)
        .bind(&params.user_id)
        .bind(params.room_id.as_deref())
        .bind(params.session_id.as_deref())
        .bind(&params.file_path)
        .bind(&params.content_type)
        .bind(params.duration_ms as i64)
        .bind(params.file_size)
        .bind(waveform_json)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        let id: i64 = result.try_get("id")?;
        self.update_user_stats(&params.user_id, params.duration_ms as i64, params.file_size)
            .await?;

        Ok(id)
    }

    pub async fn get_voice_message(
        &self,
        message_id: &str,
    ) -> Result<Option<VoiceMessageInfo>, sqlx::Error> {
        let result: Option<VoiceMessageRow> = sqlx::query_as(
            r#"
            SELECT id, message_id, user_id, room_id, session_id, file_path, 
                   content_type, duration_ms, file_size, waveform_data,
                   created_ts, transcribe_text, processed, processed_ts,
                   mime_type, encryption
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
            room_id: r.3,
            session_id: r.4,
            file_path: r.5,
            content_type: r.6,
            duration_ms: r.7.unwrap_or(0) as i32,
            file_size: r.8.unwrap_or(0),
            waveform_data: r.9.map(|w| w.0),
            transcribe_text: r.11,
            created_ts: r.10,
        }))
    }

    pub async fn get_user_voice_messages(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VoiceMessageInfo>, sqlx::Error> {
        let rows: Vec<VoiceMessageRow> = sqlx::query_as(
            r#"
            SELECT id, message_id, user_id, room_id, session_id, file_path, 
                   content_type, duration_ms, file_size, waveform_data,
                   created_ts, transcribe_text, processed, processed_ts,
                   mime_type, encryption
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
                room_id: r.3.clone(),
                session_id: r.4.clone(),
                file_path: r.5.clone(),
                content_type: r.6.clone(),
                duration_ms: r.7.unwrap_or(0) as i32,
                file_size: r.8.unwrap_or(0),
                waveform_data: r.9.clone().map(|w| w.0),
                transcribe_text: r.11.clone(),
                created_ts: r.10,
            })
            .collect())
    }

    pub async fn delete_voice_message(
        &self,
        message_id: &str,
        user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let message = sqlx::query(
            r#"
            SELECT duration_ms, file_size
            FROM voice_messages
            WHERE message_id = $1 AND user_id = $2
            "#,
        )
        .bind(message_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(msg) = message {
            let duration_ms: i64 = msg.try_get::<Option<i64>, _>("duration_ms")?.unwrap_or(0);
            let file_size: i64 = msg.try_get::<Option<i64>, _>("file_size")?.unwrap_or(0);

            sqlx::query(r#"DELETE FROM voice_messages WHERE message_id = $1"#)
                .bind(message_id)
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
        let rows: Vec<VoiceMessageRow> = sqlx::query_as(
            r#"
            SELECT id, message_id, user_id, room_id, session_id, file_path, 
                   content_type, duration_ms, file_size, waveform_data,
                   created_ts, transcribe_text, processed, processed_ts,
                   mime_type, encryption
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
                room_id: r.3.clone(),
                session_id: r.4.clone(),
                file_path: r.5.clone(),
                content_type: r.6.clone(),
                duration_ms: r.7.unwrap_or(0) as i32,
                file_size: r.8.unwrap_or(0),
                waveform_data: r.9.clone().map(|w| w.0),
                transcribe_text: r.11.clone(),
                created_ts: r.10,
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
            let _ = cache.delete(&list_cache_key).await;

            // Use Redis Hash for today's stats (Fast write)
            let daily_key = format!("voice_stats_daily:{}:{}", user_id, today_str);
            let _ = cache
                .hincrby(&daily_key, "total_duration_ms", duration_delta)
                .await;
            let _ = cache
                .hincrby(&daily_key, "total_file_size", size_delta)
                .await;
            let _ = cache.hincrby(&daily_key, "message_count", 1).await;
            let _ = cache.expire(&daily_key, 86400 * 2).await; // 2 days TTL
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
    pub message_id: String,
    pub user_id: String,
    pub room_id: Option<String>,
    pub session_id: Option<String>,
    pub file_path: String,
    pub content_type: String,
    pub duration_ms: i32,
    pub file_size: i64,
    pub waveform_data: Option<serde_json::Value>,
    pub transcribe_text: Option<String>,
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
        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        let message_id = format!("vm_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        let extension = self.get_extension_from_content_type(&params.content_type);
        let file_name = format!("{}.{}", message_id, extension);
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
                message_id: message_id.clone(),
                user_id: params.user_id,
                room_id: params.room_id,
                session_id: params.session_id,
                file_path: file_path.to_string_lossy().to_string(),
                content_type: params.content_type.clone(),
                duration_ms: params.duration_ms,
                file_size,
                waveform_data: None,
            })
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(json!({
            "message_id": message_id,
            "content_type": params.content_type,
            "duration_ms": params.duration_ms,
            "size": file_size,
            "created_ts": chrono::Utc::now().timestamp_millis()
        }))
    }

    pub async fn get_voice_message(
        &self,
        message_id: &str,
    ) -> ApiResult<Option<(Vec<u8>, String)>> {
        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        let message = voice_storage
            .get_voice_message(message_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(msg) = message {
            if let Ok(content) = fs::read(&msg.file_path).await {
                return Ok(Some((content, msg.content_type)));
            }
        }
        Ok(None)
    }

    pub async fn delete_voice_message(&self, user_id: &str, message_id: &str) -> ApiResult<bool> {
        let voice_storage = VoiceStorage::new(&self.pool, self.cache.clone());
        let deleted = voice_storage
            .delete_voice_message(message_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if deleted {
            let _file_path = self.voice_path.join(format!("{}.*", message_id));
            if let Ok(mut entries) = fs::read_dir(&self.voice_path).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.starts_with(message_id) {
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
