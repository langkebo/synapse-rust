use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::media::{ChunkedUploadStorage, CreateChunkedUploadRequest, StoreUploadChunkRequest};
use tracing::{debug, info};
use uuid::Uuid;

pub use synapse_storage::media::{ChunkUploadRequest, ChunkUploadResponse, CompletedUploadData, UploadProgress};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteUploadRequest {
    pub upload_id: String,
}

pub struct ChunkedUploadService {
    storage: ChunkedUploadStorage,
    chunk_size_limit: usize,
    max_file_size: usize,
    upload_expiry_seconds: i64,
}

impl ChunkedUploadService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            storage: ChunkedUploadStorage::new(&pool),
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

        self.storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_id.clone(),
                user_id: user_id.to_string(),
                filename: filename.map(str::to_owned),
                content_type: content_type.map(str::to_owned),
                total_size,
                total_chunks,
                created_ts: now,
                expires_at,
            })
            .await?;

        info!(
            upload_id = %upload_id,
            user_id = %user_id,
            filename = ?filename,
            content_type = ?content_type,
            total_size,
            total_chunks,
            "Started chunked upload"
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
            return Err(ApiError::bad_request(format!("Upload is in {} state", progress.status)));
        }

        let now = chrono::Utc::now().timestamp_millis();
        let chunk_size = request.chunk_data.len() as i64;

        self.storage
            .store_chunk(StoreUploadChunkRequest {
                upload_id: upload_id.clone(),
                chunk_index: request.chunk_index,
                chunk_data: request.chunk_data,
                chunk_size,
                created_ts: now,
            })
            .await?;
        self.storage.increment_upload_progress(&upload_id, chunk_size, now).await?;

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
        self.storage
            .get_progress(upload_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Upload not found".to_string()))
    }

    pub async fn load_completed_upload(&self, upload_id: &str, user_id: &str) -> Result<CompletedUploadData, ApiError> {
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

        let mut combined_data = Vec::new();
        for chunk_data in self.storage.load_chunk_data(upload_id).await? {
            combined_data.extend_from_slice(&chunk_data);
        }

        if combined_data.len() > self.max_file_size {
            return Err(ApiError::bad_request(format!(
                "File size {} exceeds maximum {}",
                combined_data.len(),
                self.max_file_size
            )));
        }

        Ok(CompletedUploadData {
            filename: progress.filename,
            content_type: progress.content_type,
            data: combined_data,
        })
    }

    pub async fn mark_upload_finalized(&self, upload_id: &str) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        self.storage.finalize_upload(upload_id, now).await?;

        info!(upload_id = %upload_id, "Finalized chunked upload and cleaned chunks");

        Ok(())
    }

    pub async fn cancel_upload(&self, upload_id: &str, user_id: &str) -> Result<(), ApiError> {
        let progress = self.get_progress(upload_id).await?;

        if progress.user_id != user_id {
            return Err(ApiError::forbidden("Upload does not belong to user"));
        }

        if let Err(error) = self.storage.delete_upload(upload_id).await {
            tracing::warn!(error = %error, upload_id = %upload_id, user_id = %user_id, "Failed to fully delete upload");
        }

        info!(upload_id = %upload_id, user_id = %user_id, "Cancelled chunked upload");
        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<u64, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        let expired = self.storage.list_expired_upload_ids(now).await?;

        let mut cleaned = 0u64;
        for upload_id in expired {
            match self.storage.delete_upload(&upload_id).await {
                Ok(()) => cleaned += 1,
                Err(error) => {
                    tracing::warn!(error = %error, upload_id = %upload_id, "Failed to cleanup expired upload");
                }
            }
        }

        if cleaned > 0 {
            info!(expired_upload_count = cleaned, "Cleaned up expired chunked uploads");
        }

        Ok(cleaned)
    }

    pub async fn list_user_uploads(&self, user_id: &str) -> Result<Vec<UploadProgress>, ApiError> {
        self.storage.list_user_uploads(user_id).await
    }
}
