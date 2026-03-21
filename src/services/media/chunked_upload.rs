use crate::common::ApiError;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UploadProgress {
    pub upload_id: String,
    pub user_id: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub total_size: Option<i64>,
    pub uploaded_size: i64,
    pub total_chunks: i32,
    pub uploaded_chunks: i32,
    pub status: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkUploadRequest {
    pub upload_id: Option<String>,
    pub chunk_index: i32,
    pub total_chunks: i32,
    pub chunk_data: Vec<u8>,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub total_size: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkUploadResponse {
    pub upload_id: String,
    pub chunk_index: i32,
    pub uploaded_chunks: i32,
    pub total_chunks: i32,
    pub uploaded_size: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteUploadRequest {
    pub upload_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaUploadResponse {
    pub media_id: String,
    pub content_uri: String,
    pub size: i64,
}

pub struct ChunkedUploadService {
    pool: Arc<PgPool>,
    chunk_size_limit: usize,
    max_file_size: usize,
    upload_expiry_seconds: i64,
}

impl ChunkedUploadService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            pool,
            chunk_size_limit: 10 * 1024 * 1024, // 10MB per chunk
            max_file_size: 100 * 1024 * 1024,   // 100MB total
            upload_expiry_seconds: 3600,        // 1 hour
        }
    }

    pub async fn start_upload(
        &self,
        user_id: &str,
        filename: Option<&str>,
        content_type: Option<&str>,
        total_size: Option<i64>,
        total_chunks: i32,
    ) -> Result<String, ApiError> {
        let upload_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + (self.upload_expiry_seconds * 1000);

        sqlx::query(
            r#"
            INSERT INTO upload_progress 
            (upload_id, user_id, filename, content_type, total_size, total_chunks, status, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8)
            "#,
        )
        .bind(&upload_id)
        .bind(user_id)
        .bind(filename)
        .bind(content_type)
        .bind(total_size)
        .bind(total_chunks)
        .bind(now)
        .bind(expires_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start upload: {}", e)))?;

        info!(
            "Started chunked upload: {} for user: {}",
            upload_id, user_id
        );
        Ok(upload_id)
    }

    pub async fn upload_chunk(
        &self,
        request: ChunkUploadRequest,
        user_id: &str,
    ) -> Result<ChunkUploadResponse, ApiError> {
        if request.chunk_data.len() > self.chunk_size_limit {
            return Err(ApiError::bad_request(format!(
                "Chunk size {} exceeds limit {}",
                request.chunk_data.len(),
                self.chunk_size_limit
            )));
        }

        let upload_id = match request.upload_id {
            Some(id) => id,
            None => {
                self.start_upload(
                    user_id,
                    request.filename.as_deref(),
                    request.content_type.as_deref(),
                    request.total_size,
                    request.total_chunks,
                )
                .await?
            }
        };

        let progress = self.get_progress(&upload_id).await?;
        if progress.user_id != user_id {
            return Err(ApiError::forbidden("Upload does not belong to user"));
        }

        if progress.status != "pending" {
            return Err(ApiError::bad_request(format!(
                "Upload is in {} state",
                progress.status
            )));
        }

        let now = chrono::Utc::now().timestamp_millis();
        let chunk_size = request.chunk_data.len() as i64;

        sqlx::query(
            r#"
            INSERT INTO upload_chunks (upload_id, chunk_index, chunk_data, chunk_size, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (upload_id, chunk_index) DO UPDATE SET 
                chunk_data = EXCLUDED.chunk_data,
                chunk_size = EXCLUDED.chunk_size
            "#,
        )
        .bind(&upload_id)
        .bind(request.chunk_index)
        .bind(&request.chunk_data)
        .bind(chunk_size)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store chunk: {}", e)))?;

        sqlx::query(
            r#"
            UPDATE upload_progress 
            SET uploaded_chunks = uploaded_chunks + 1,
                uploaded_size = uploaded_size + $2,
                updated_ts = $3,
                status = CASE WHEN uploaded_chunks + 1 >= total_chunks THEN 'complete' ELSE 'pending' END
            WHERE upload_id = $1
            "#,
        )
        .bind(&upload_id)
        .bind(chunk_size)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update progress: {}", e)))?;

        let progress = self.get_progress(&upload_id).await?;

        debug!(
            "Uploaded chunk {} for upload {}, progress: {}/{}",
            request.chunk_index, upload_id, progress.uploaded_chunks, progress.total_chunks
        );

        Ok(ChunkUploadResponse {
            upload_id: upload_id.clone(),
            chunk_index: request.chunk_index,
            uploaded_chunks: progress.uploaded_chunks,
            total_chunks: progress.total_chunks,
            uploaded_size: progress.uploaded_size,
            status: progress.status,
        })
    }

    pub async fn get_progress(&self, upload_id: &str) -> Result<UploadProgress, ApiError> {
        sqlx::query_as::<_, UploadProgress>("SELECT * FROM upload_progress WHERE upload_id = $1")
            .bind(upload_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get progress: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Upload not found".to_string()))
    }

    pub async fn complete_upload(
        &self,
        upload_id: &str,
        user_id: &str,
    ) -> Result<MediaUploadResponse, ApiError> {
        let progress = self.get_progress(upload_id).await?;

        if progress.user_id != user_id {
            return Err(ApiError::forbidden("Upload does not belong to user"));
        }

        if progress.status != "complete" {
            return Err(ApiError::bad_request(format!(
                "Upload not complete: {}/{} chunks uploaded",
                progress.uploaded_chunks, progress.total_chunks
            )));
        }

        let chunks = sqlx::query(
            "SELECT chunk_data FROM upload_chunks WHERE upload_id = $1 ORDER BY chunk_index",
        )
        .bind(upload_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get chunks: {}", e)))?;

        let mut combined_data = Vec::new();
        for row in chunks {
            let chunk_data: Vec<u8> = row.get("chunk_data");
            combined_data.extend_from_slice(&chunk_data);
        }

        if combined_data.len() > self.max_file_size {
            return Err(ApiError::bad_request(format!(
                "File size {} exceeds maximum {}",
                combined_data.len(),
                self.max_file_size
            )));
        }

        let media_id = Uuid::new_v4().to_string();
        let content_uri = format!("mxc://localhost/{}", media_id);
        let size = combined_data.len() as i64;

        sqlx::query(
            r#"
            UPDATE upload_progress 
            SET status = 'finalized', updated_ts = $2
            WHERE upload_id = $1
            "#,
        )
        .bind(upload_id)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await
        .ok();

        info!(
            "Completed chunked upload: {} as media: {}, size: {}",
            upload_id, media_id, size
        );

        Ok(MediaUploadResponse {
            media_id,
            content_uri,
            size,
        })
    }

    pub async fn cancel_upload(&self, upload_id: &str, user_id: &str) -> Result<(), ApiError> {
        let progress = self.get_progress(upload_id).await?;

        if progress.user_id != user_id {
            return Err(ApiError::forbidden("Upload does not belong to user"));
        }

        sqlx::query("DELETE FROM upload_chunks WHERE upload_id = $1")
            .bind(upload_id)
            .execute(&*self.pool)
            .await
            .ok();

        sqlx::query("DELETE FROM upload_progress WHERE upload_id = $1")
            .bind(upload_id)
            .execute(&*self.pool)
            .await
            .ok();

        info!("Cancelled upload: {}", upload_id);
        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<u64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let expired: Vec<String> =
            sqlx::query_scalar("SELECT upload_id FROM upload_progress WHERE expires_at < $1")
                .bind(now)
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to find expired uploads: {}", e))
                })?;

        let mut cleaned = 0u64;
        for upload_id in expired {
            sqlx::query("DELETE FROM upload_chunks WHERE upload_id = $1")
                .bind(&upload_id)
                .execute(&*self.pool)
                .await
                .ok();

            sqlx::query("DELETE FROM upload_progress WHERE upload_id = $1")
                .bind(&upload_id)
                .execute(&*self.pool)
                .await
                .ok();

            cleaned += 1;
        }

        if cleaned > 0 {
            info!("Cleaned up {} expired uploads", cleaned);
        }

        Ok(cleaned)
    }

    pub async fn list_user_uploads(&self, user_id: &str) -> Result<Vec<UploadProgress>, ApiError> {
        sqlx::query_as::<_, UploadProgress>(
            "SELECT * FROM upload_progress WHERE user_id = $1 AND status != 'finalized' ORDER BY created_ts DESC",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list uploads: {}", e)))
    }
}

impl Default for ChunkedUploadService {
    fn default() -> Self {
        panic!("ChunkedUploadService requires a database pool")
    }
}
