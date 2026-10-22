use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use synapse_storage::media::{ChunkedUploadStorage, CreateChunkedUploadRequest, StoreUploadChunkRequest};
use tracing::{debug, info};
use uuid::Uuid;

pub use synapse_storage::media::{ChunkUploadRequest, ChunkUploadResponse, CompletedUploadData, UploadProgress};

/// The `CompleteUploadRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteUploadRequest {
    /// The `upload_id` field.
    pub upload_id: String,
}

/// Upper bound on how many chunks one upload may declare.
///
/// `total_chunks` is client-supplied and sizes the read that reassembles the
/// upload. At 10 MiB per chunk this already allows a 97 GiB file, far above any
/// configured `max_upload_size`, so the ceiling only exists to stop a single
/// row from turning into an unbounded allocation.
pub const MAX_CHUNKS_PER_UPLOAD: i32 = 10_000;

/// The `ChunkedUploadService` struct.
pub struct ChunkedUploadService {
    storage: ChunkedUploadStorage,
    chunk_size_limit: usize,
    max_file_size: usize,
    upload_expiry_seconds: i64,
}

impl ChunkedUploadService {
    /// G-1: `max_file_size` 必须来自权威配置 `config.server.max_upload_size`（字节），
    /// 由调用方（container 装配处）传入，不再硬编码 100MB。
    pub fn new(pool: Arc<PgPool>, max_file_size: usize) -> Self {
        Self {
            storage: ChunkedUploadStorage::new(&pool),
            chunk_size_limit: 10 * 1024 * 1024, // 10MB per chunk
            max_file_size,
            upload_expiry_seconds: 3600, // 1 hour
        }
    }

    /// See [`start_upload`].
    pub async fn start_upload(
        &self,
        user_id: &str,
        filename: Option<&str>,
        content_type: Option<&str>,
        total_size: Option<i64>,
        total_chunks: i32,
    ) -> Result<String, ApiError> {
        // `total_chunks` is client-supplied and sizes the reassembly read, so it is
        // validated at this one write entry point rather than trusted downstream.
        if total_chunks < 1 {
            return Err(ApiError::bad_request(format!("total_chunks must be at least 1, got {total_chunks}")));
        }

        if total_chunks > MAX_CHUNKS_PER_UPLOAD {
            return Err(ApiError::too_large(format!(
                "total_chunks {total_chunks} exceeds maximum {MAX_CHUNKS_PER_UPLOAD}"
            )));
        }

        let upload_id = Uuid::new_v4().to_string();
        let now = current_timestamp_millis();
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

    /// See [`upload_chunk`].
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

        let now = current_timestamp_millis();
        let chunk_size = request.chunk_data.len() as i64;

        // FUNC-06: the declared total is a hard ceiling, not a hint. A client that
        // streams past it would otherwise hold the excess on disk until completion,
        // so the offending chunk is refused and the session is closed.
        let declared_total = progress.total_size.unwrap_or(self.max_file_size as i64);
        if progress.uploaded_size + chunk_size > declared_total {
            self.storage.delete_upload(&upload_id).await?;
            return Err(ApiError::too_large(format!(
                "Chunk would exceed declared total size {declared_total} (already uploaded {})",
                progress.uploaded_size
            )));
        }

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

    /// See [`get_progress`].
    pub async fn get_progress(&self, upload_id: &str) -> Result<UploadProgress, ApiError> {
        self.storage.get_progress(upload_id).await?.ok_or_else(|| ApiError::not_found("Upload not found".to_string()))
    }

    /// The per-file ceiling this service enforces (`config.server.max_upload_size`).
    pub fn max_file_size(&self) -> i64 {
        self.max_file_size as i64
    }

    /// Total size of the chunks, rejecting before the caller allocates (PERF-14).
    ///
    /// Returns the summed length so the reassembly buffer can be reserved in one
    /// go instead of grown chunk by chunk.
    fn checked_chunk_total(chunks: &[Vec<u8>], max_file_size: usize) -> Result<usize, ApiError> {
        let total: usize = chunks.iter().map(Vec::len).sum();

        if total > max_file_size {
            return Err(ApiError::bad_request(format!("File size {total} exceeds maximum {max_file_size}")));
        }

        Ok(total)
    }

    /// Bytes already held by the user's in-flight uploads, optionally excluding
    /// one upload (the one currently being converted from pending to used).
    pub async fn pending_bytes(&self, user_id: &str, exclude_upload_id: Option<&str>) -> Result<i64, ApiError> {
        self.storage.pending_bytes(user_id, exclude_upload_id).await
    }

    /// See [`load_completed_upload`].
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

        // Ask for one row past the declared count: a fuller result can only come from
        // rows written out of band, and silently dropping them would reassemble a
        // truncated file.
        let chunk_rows = self.storage.load_chunk_data(upload_id, i64::from(progress.total_chunks) + 1).await?;
        if chunk_rows.len() as i64 > i64::from(progress.total_chunks) {
            return Err(ApiError::internal(format!(
                "Upload {upload_id} holds {} chunk rows but declared {}",
                chunk_rows.len(),
                progress.total_chunks
            )));
        }

        // PERF-14: decide the size from the chunk lengths, before anything is
        // concatenated. The ceiling used to be checked on the assembled buffer, so
        // an oversized upload was fully materialised before being rejected.
        let total_size = Self::checked_chunk_total(&chunk_rows, self.max_file_size)?;

        let mut combined_data = Vec::with_capacity(total_size);
        for chunk_data in chunk_rows {
            combined_data.extend_from_slice(&chunk_data);
        }

        Ok(CompletedUploadData {
            filename: progress.filename,
            content_type: progress.content_type,
            data: combined_data,
        })
    }

    /// See [`mark_upload_finalized`].
    pub async fn mark_upload_finalized(&self, upload_id: &str) -> Result<(), ApiError> {
        let now = current_timestamp_millis();
        self.storage.finalize_upload(upload_id, now).await?;

        info!(upload_id = %upload_id, "Finalized chunked upload and cleaned chunks");

        Ok(())
    }

    /// See [`cancel_upload`].
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

    /// See [`cleanup_expired`].
    pub async fn cleanup_expired(&self) -> Result<u64, ApiError> {
        self.storage.cleanup_expired().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_assembly_is_rejected_before_the_buffer_is_allocated() {
        // PERF-14: the ceiling used to be checked on the concatenated buffer, so an
        // oversized upload was fully materialised before being rejected. The size
        // must be decided from the chunk lengths alone.
        let chunks = vec![vec![0u8; 4], vec![0u8; 4]];
        let error = ChunkedUploadService::checked_chunk_total(&chunks, 7).expect_err("8 > 7 must be rejected");
        assert!(error.is_bad_request(), "expected a 400, got {:?}", error.kind);

        // Exactly at the ceiling is allowed, and the sum doubles as the capacity.
        assert_eq!(ChunkedUploadService::checked_chunk_total(&chunks, 8).expect("8 == 8"), 8);
        assert_eq!(ChunkedUploadService::checked_chunk_total(&[], 8).expect("empty"), 0);
    }
}
