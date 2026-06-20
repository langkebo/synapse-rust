use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::ApiError;
use tracing::{info, warn};

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
pub struct CompletedUploadData {
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct CreateChunkedUploadRequest {
    pub upload_id: String,
    pub user_id: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub total_size: Option<i64>,
    pub total_chunks: i32,
    pub created_ts: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct StoreUploadChunkRequest {
    pub upload_id: String,
    pub chunk_index: i32,
    pub chunk_data: Vec<u8>,
    pub chunk_size: i64,
    pub created_ts: i64,
}

#[derive(Clone)]
pub struct ChunkedUploadStorage {
    pool: PgPool,
}

impl ChunkedUploadStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: (**pool).clone() }
    }

    pub async fn create_upload(&self, request: CreateChunkedUploadRequest) -> Result<(), ApiError> {
        sqlx::query(
            r"
            INSERT INTO upload_progress
            (upload_id, user_id, filename, content_type, total_size, total_chunks, status, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8)
            ",
        )
        .bind(&request.upload_id)
        .bind(&request.user_id)
        .bind(&request.filename)
        .bind(&request.content_type)
        .bind(request.total_size)
        .bind(request.total_chunks)
        .bind(request.created_ts)
        .bind(request.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to start upload", &e))?;

        Ok(())
    }

    pub async fn store_chunk(&self, request: StoreUploadChunkRequest) -> Result<(), ApiError> {
        sqlx::query(
            r"
            INSERT INTO upload_chunks (upload_id, chunk_index, chunk_data, chunk_size, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (upload_id, chunk_index) DO UPDATE SET
                chunk_data = EXCLUDED.chunk_data,
                chunk_size = EXCLUDED.chunk_size
            ",
        )
        .bind(&request.upload_id)
        .bind(request.chunk_index)
        .bind(&request.chunk_data)
        .bind(request.chunk_size)
        .bind(request.created_ts)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to store chunk", &e))?;

        Ok(())
    }

    pub async fn increment_upload_progress(
        &self,
        upload_id: &str,
        chunk_size: i64,
        now_ts: i64,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r"
            UPDATE upload_progress
            SET uploaded_chunks = uploaded_chunks + 1,
                uploaded_size = uploaded_size + $2,
                updated_ts = $3,
                status = CASE WHEN uploaded_chunks + 1 >= total_chunks THEN 'complete' ELSE 'pending' END
            WHERE upload_id = $1
            ",
        )
        .bind(upload_id)
        .bind(chunk_size)
        .bind(now_ts)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update progress", &e))?;

        Ok(())
    }

    pub async fn get_progress(&self, upload_id: &str) -> Result<Option<UploadProgress>, ApiError> {
        sqlx::query_as::<_, UploadProgress>(
            "SELECT upload_id, user_id, filename, content_type, total_size, uploaded_size, total_chunks, uploaded_chunks, status, created_ts, updated_ts, expires_at FROM upload_progress WHERE upload_id = $1",
        )
        .bind(upload_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get progress", &e))
    }

    pub async fn load_chunk_data(&self, upload_id: &str) -> Result<Vec<Vec<u8>>, ApiError> {
        let rows = sqlx::query("SELECT chunk_data FROM upload_chunks WHERE upload_id = $1 ORDER BY chunk_index")
            .bind(upload_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get chunks", &e))?;

        Ok(rows.into_iter().map(|row| sqlx::Row::get::<Vec<u8>, _>(&row, "chunk_data")).collect())
    }

    pub async fn finalize_upload(&self, upload_id: &str, now_ts: i64) -> Result<(), ApiError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to start upload finalization transaction", &e))?;

        sqlx::query(
            r"
            UPDATE upload_progress
            SET status = 'finalized', updated_ts = $2
            WHERE upload_id = $1
            ",
        )
        .bind(upload_id)
        .bind(now_ts)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to finalize upload status", &e))?;

        sqlx::query("DELETE FROM upload_chunks WHERE upload_id = $1")
            .bind(upload_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup finalized upload chunks", &e))?;

        tx.commit().await.map_err(|e| ApiError::internal_with_log("Failed to commit upload finalization", &e))?;

        Ok(())
    }

    pub async fn delete_upload(&self, upload_id: &str) -> Result<(), ApiError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to start upload deletion transaction", &e))?;

        sqlx::query("DELETE FROM upload_chunks WHERE upload_id = $1")
            .bind(upload_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete upload chunks", &e))?;

        sqlx::query("DELETE FROM upload_progress WHERE upload_id = $1")
            .bind(upload_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete upload progress", &e))?;

        tx.commit().await.map_err(|e| ApiError::internal_with_log("Failed to commit upload deletion", &e))?;

        Ok(())
    }

    pub async fn list_expired_upload_ids(&self, now_ts: i64) -> Result<Vec<String>, ApiError> {
        sqlx::query_scalar("SELECT upload_id FROM upload_progress WHERE expires_at < $1")
            .bind(now_ts)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to find expired uploads", &e))
    }

    pub async fn list_user_uploads(&self, user_id: &str) -> Result<Vec<UploadProgress>, ApiError> {
        sqlx::query_as::<_, UploadProgress>(
            "SELECT upload_id, user_id, filename, content_type, total_size, uploaded_size, total_chunks, uploaded_chunks, status, created_ts, updated_ts, expires_at FROM upload_progress WHERE user_id = $1 AND status != 'finalized' ORDER BY created_ts DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to list uploads", &e))
    }

    /// Delete all uploads whose `expires_at` is before `now_ts`.
    ///
    /// This lives on the storage struct so that callers that only hold a
    /// `ChunkedUploadStorage` (e.g. `RetentionService`) can drive expired-upload
    /// cleanup without constructing a full `ChunkedUploadService` or holding a
    /// raw `PgPool`.
    pub async fn cleanup_expired(&self) -> Result<u64, ApiError> {
        let now_ts = chrono::Utc::now().timestamp_millis();
        let expired = self.list_expired_upload_ids(now_ts).await?;

        let mut cleaned = 0u64;
        for upload_id in expired {
            match self.delete_upload(&upload_id).await {
                Ok(()) => cleaned += 1,
                Err(error) => {
                    warn!(error = %error, upload_id = %upload_id, "Failed to cleanup expired upload");
                }
            }
        }

        if cleaned > 0 {
            info!(expired_upload_count = cleaned, "Cleaned up expired chunked uploads");
        }

        Ok(cleaned)
    }
}
