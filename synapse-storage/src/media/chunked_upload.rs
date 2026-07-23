use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
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

// ── Trait ───────────────────────────────────────────────────────────────

#[async_trait]
pub trait ChunkedUploadStoreApi: Send + Sync {
    async fn create_upload(&self, request: CreateChunkedUploadRequest) -> Result<(), ApiError>;

    async fn store_chunk(&self, request: StoreUploadChunkRequest) -> Result<(), ApiError>;

    async fn increment_upload_progress(&self, upload_id: &str, chunk_size: i64, now_ts: i64) -> Result<(), ApiError>;

    async fn get_progress(&self, upload_id: &str) -> Result<Option<UploadProgress>, ApiError>;

    async fn load_chunk_data(&self, upload_id: &str) -> Result<Vec<Vec<u8>>, ApiError>;

    async fn finalize_upload(&self, upload_id: &str, now_ts: i64) -> Result<(), ApiError>;

    async fn delete_upload(&self, upload_id: &str) -> Result<(), ApiError>;

    async fn list_expired_upload_ids(&self, now_ts: i64) -> Result<Vec<String>, ApiError>;

    async fn list_user_uploads(&self, user_id: &str) -> Result<Vec<UploadProgress>, ApiError>;

    async fn cleanup_expired(&self) -> Result<u64, ApiError>;
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
        let now_ts = current_timestamp_millis();
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

// ── Delegation impl for ChunkedUploadStoreApi ─────────────────────────

#[async_trait]
impl ChunkedUploadStoreApi for ChunkedUploadStorage {
    async fn create_upload(&self, request: CreateChunkedUploadRequest) -> Result<(), ApiError> {
        self.create_upload(request).await
    }

    async fn store_chunk(&self, request: StoreUploadChunkRequest) -> Result<(), ApiError> {
        self.store_chunk(request).await
    }

    async fn increment_upload_progress(&self, upload_id: &str, chunk_size: i64, now_ts: i64) -> Result<(), ApiError> {
        self.increment_upload_progress(upload_id, chunk_size, now_ts).await
    }

    async fn get_progress(&self, upload_id: &str) -> Result<Option<UploadProgress>, ApiError> {
        self.get_progress(upload_id).await
    }

    async fn load_chunk_data(&self, upload_id: &str) -> Result<Vec<Vec<u8>>, ApiError> {
        self.load_chunk_data(upload_id).await
    }

    async fn finalize_upload(&self, upload_id: &str, now_ts: i64) -> Result<(), ApiError> {
        self.finalize_upload(upload_id, now_ts).await
    }

    async fn delete_upload(&self, upload_id: &str) -> Result<(), ApiError> {
        self.delete_upload(upload_id).await
    }

    async fn list_expired_upload_ids(&self, now_ts: i64) -> Result<Vec<String>, ApiError> {
        self.list_expired_upload_ids(now_ts).await
    }

    async fn list_user_uploads(&self, user_id: &str) -> Result<Vec<UploadProgress>, ApiError> {
        self.list_user_uploads(user_id).await
    }

    async fn cleanup_expired(&self) -> Result<u64, ApiError> {
        self.cleanup_expired().await
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Insert a minimal user row so the foreign-key constraint on
    /// `upload_progress.user_id` is satisfied. Uses ON CONFLICT DO NOTHING.
    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let now = current_timestamp_millis();
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            r#"INSERT INTO users (user_id, username, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    /// Clean up any test artifacts for a given upload_id.
    async fn cleanup_upload(pool: &PgPool, upload_id: &str) {
        let _ = sqlx::query("DELETE FROM upload_chunks WHERE upload_id = $1").bind(upload_id).execute(pool).await;
        let _ = sqlx::query("DELETE FROM upload_progress WHERE upload_id = $1").bind(upload_id).execute(pool).await;
    }

    #[tokio::test]
    async fn test_create_upload_session() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let upload_id = format!("upload-create-{suffix}");
        let user_id = format!("@create-test_{suffix}:example.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_upload(&pool, &upload_id).await;

        let now = current_timestamp_millis();
        let expires_at = now + 3600000;
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_id.clone(),
                user_id: user_id.clone(),
                filename: Some("test.txt".to_string()),
                content_type: Some("text/plain".to_string()),
                total_size: Some(1024),
                total_chunks: 4,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create_upload should succeed");

        let progress =
            storage.get_progress(&upload_id).await.expect("get_progress should succeed").expect("upload should exist");

        assert_eq!(progress.upload_id, upload_id);
        assert_eq!(progress.user_id, user_id);
        assert_eq!(progress.filename.as_deref(), Some("test.txt"));
        assert_eq!(progress.content_type.as_deref(), Some("text/plain"));
        assert_eq!(progress.total_size, Some(1024));
        assert_eq!(progress.total_chunks, 4);
        assert_eq!(progress.uploaded_chunks, 0);
        assert_eq!(progress.uploaded_size, 0);
        assert_eq!(progress.status, "pending");

        cleanup_upload(&pool, &upload_id).await;
    }

    #[tokio::test]
    async fn test_get_progress_nonexistent_returns_none() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);

        let result = storage.get_progress("nonexistent-upload-id").await.expect("query should succeed");

        assert!(result.is_none(), "nonexistent upload should return None");
    }

    #[tokio::test]
    async fn test_store_chunk_and_increment_progress() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let upload_id = format!("upload-chunk-{suffix}");
        let user_id = format!("@chunk-test_{suffix}:example.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_upload(&pool, &upload_id).await;

        let now = current_timestamp_millis();
        let expires_at = now + 3600000;
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_id.clone(),
                user_id: user_id.clone(),
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 2,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create_upload should succeed");

        let chunk_data = b"hello world".to_vec();
        let chunk_size = chunk_data.len() as i64;
        storage
            .store_chunk(StoreUploadChunkRequest {
                upload_id: upload_id.clone(),
                chunk_index: 0,
                chunk_data: chunk_data.clone(),
                chunk_size,
                created_ts: now,
            })
            .await
            .expect("store_chunk should succeed");

        storage
            .increment_upload_progress(&upload_id, chunk_size, now)
            .await
            .expect("increment progress should succeed");

        let progress =
            storage.get_progress(&upload_id).await.expect("get_progress should succeed").expect("upload should exist");

        assert_eq!(progress.uploaded_chunks, 1);
        assert_eq!(progress.uploaded_size, chunk_size);

        cleanup_upload(&pool, &upload_id).await;
    }

    #[tokio::test]
    async fn test_store_chunk_upserts_on_conflict() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let upload_id = format!("upload-upsert-{suffix}");
        let user_id = format!("@upsert-test_{suffix}:example.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_upload(&pool, &upload_id).await;

        let now = current_timestamp_millis();
        let expires_at = now + 3600000;
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_id.clone(),
                user_id,
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create_upload should succeed");

        let first_chunk = b"first".to_vec();
        storage
            .store_chunk(StoreUploadChunkRequest {
                upload_id: upload_id.clone(),
                chunk_index: 0,
                chunk_data: first_chunk.clone(),
                chunk_size: first_chunk.len() as i64,
                created_ts: now,
            })
            .await
            .expect("first store_chunk should succeed");

        let second_chunk = b"second-overwrite".to_vec();
        storage
            .store_chunk(StoreUploadChunkRequest {
                upload_id: upload_id.clone(),
                chunk_index: 0,
                chunk_data: second_chunk.clone(),
                chunk_size: second_chunk.len() as i64,
                created_ts: now,
            })
            .await
            .expect("second store_chunk should succeed");

        let chunks = storage.load_chunk_data(&upload_id).await.expect("load_chunk_data should succeed");

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], second_chunk);

        cleanup_upload(&pool, &upload_id).await;
    }

    #[tokio::test]
    async fn test_load_chunk_data_ordered_by_index() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let upload_id = format!("upload-order-{suffix}");
        let user_id = format!("@order-test_{suffix}:example.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_upload(&pool, &upload_id).await;

        let now = current_timestamp_millis();
        let expires_at = now + 3600000;
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_id.clone(),
                user_id,
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 3,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create_upload should succeed");

        // Store out of order to verify ORDER BY chunk_index works
        let chunk2 = b"chunk-2".to_vec();
        let chunk0 = b"chunk-0".to_vec();
        let chunk1 = b"chunk-1".to_vec();

        storage
            .store_chunk(StoreUploadChunkRequest {
                upload_id: upload_id.clone(),
                chunk_index: 2,
                chunk_data: chunk2.clone(),
                chunk_size: chunk2.len() as i64,
                created_ts: now,
            })
            .await
            .expect("store chunk2 should succeed");

        storage
            .store_chunk(StoreUploadChunkRequest {
                upload_id: upload_id.clone(),
                chunk_index: 0,
                chunk_data: chunk0.clone(),
                chunk_size: chunk0.len() as i64,
                created_ts: now,
            })
            .await
            .expect("store chunk0 should succeed");

        storage
            .store_chunk(StoreUploadChunkRequest {
                upload_id: upload_id.clone(),
                chunk_index: 1,
                chunk_data: chunk1.clone(),
                chunk_size: chunk1.len() as i64,
                created_ts: now,
            })
            .await
            .expect("store chunk1 should succeed");

        let chunks = storage.load_chunk_data(&upload_id).await.expect("load_chunk_data should succeed");

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], chunk0);
        assert_eq!(chunks[1], chunk1);
        assert_eq!(chunks[2], chunk2);

        cleanup_upload(&pool, &upload_id).await;
    }

    #[tokio::test]
    async fn test_finalize_upload_cleans_chunks() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let upload_id = format!("upload-finalize-{suffix}");
        let user_id = format!("@finalize-test_{suffix}:example.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_upload(&pool, &upload_id).await;

        let now = current_timestamp_millis();
        let expires_at = now + 3600000;
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_id.clone(),
                user_id: user_id.clone(),
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create_upload should succeed");

        let chunk_data = b"finalize-me".to_vec();
        storage
            .store_chunk(StoreUploadChunkRequest {
                upload_id: upload_id.clone(),
                chunk_index: 0,
                chunk_data,
                chunk_size: 11,
                created_ts: now,
            })
            .await
            .expect("store_chunk should succeed");

        storage.finalize_upload(&upload_id, current_timestamp_millis()).await.expect("finalize_upload should succeed");

        let progress = storage
            .get_progress(&upload_id)
            .await
            .expect("get_progress should succeed")
            .expect("upload should still exist");

        assert_eq!(progress.status, "finalized");

        let chunks = storage.load_chunk_data(&upload_id).await.expect("load_chunk_data should succeed");

        assert!(chunks.is_empty(), "chunks should be deleted after finalize");

        cleanup_upload(&pool, &upload_id).await;
    }

    #[tokio::test]
    async fn test_delete_upload_removes_progress_and_chunks() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let upload_id = format!("upload-delete-{suffix}");
        let user_id = format!("@delete-test_{suffix}:example.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_upload(&pool, &upload_id).await;

        let now = current_timestamp_millis();
        let expires_at = now + 3600000;
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_id.clone(),
                user_id,
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create_upload should succeed");

        let chunk_data = b"to-delete".to_vec();
        storage
            .store_chunk(StoreUploadChunkRequest {
                upload_id: upload_id.clone(),
                chunk_index: 0,
                chunk_data,
                chunk_size: 8,
                created_ts: now,
            })
            .await
            .expect("store_chunk should succeed");

        storage.delete_upload(&upload_id).await.expect("delete_upload should succeed");

        let progress = storage.get_progress(&upload_id).await.expect("get_progress should succeed");

        assert!(progress.is_none(), "upload progress should be deleted");

        let chunks = storage.load_chunk_data(&upload_id).await.expect("load_chunk_data should succeed");

        assert!(chunks.is_empty(), "upload chunks should be deleted");
    }

    #[tokio::test]
    async fn test_list_user_uploads_excludes_finalized() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@list-test_{suffix}:example.com");
        let upload_a = format!("upload-list-a-{suffix}");
        let upload_b = format!("upload-list-b-{suffix}");
        let upload_c = format!("upload-list-c-{suffix}");

        ensure_test_user(&pool, &user_id).await;
        cleanup_upload(&pool, &upload_a).await;
        cleanup_upload(&pool, &upload_b).await;
        cleanup_upload(&pool, &upload_c).await;

        let now = current_timestamp_millis();
        let expires_at = now + 3600000;

        // Upload A: pending (should appear)
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_a.clone(),
                user_id: user_id.clone(),
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create upload_a should succeed");

        // Upload B: pending (should appear)
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_b.clone(),
                user_id: user_id.clone(),
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create upload_b should succeed");

        // Upload C: finalized (should NOT appear)
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_c.clone(),
                user_id: user_id.clone(),
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create upload_c should succeed");
        storage.finalize_upload(&upload_c, current_timestamp_millis()).await.expect("finalize upload_c should succeed");

        let uploads = storage.list_user_uploads(&user_id).await.expect("list_user_uploads should succeed");

        let upload_ids: Vec<&str> = uploads.iter().map(|u| u.upload_id.as_str()).collect();
        assert!(upload_ids.contains(&upload_a.as_str()), "should contain pending upload A");
        assert!(upload_ids.contains(&upload_b.as_str()), "should contain pending upload B");
        assert!(!upload_ids.contains(&upload_c.as_str()), "should NOT contain finalized upload C");

        cleanup_upload(&pool, &upload_a).await;
        cleanup_upload(&pool, &upload_b).await;
        cleanup_upload(&pool, &upload_c).await;
    }

    #[tokio::test]
    async fn test_cleanup_expired_removes_expired_uploads() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@expired-test_{suffix}:example.com");
        let active_upload = format!("upload-active-{suffix}");
        let expired_upload = format!("upload-expired-{suffix}");

        ensure_test_user(&pool, &user_id).await;
        cleanup_upload(&pool, &active_upload).await;
        cleanup_upload(&pool, &expired_upload).await;

        let now = current_timestamp_millis();

        // Active upload: expires in the future
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: active_upload.clone(),
                user_id: user_id.clone(),
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now,
                expires_at: now + 3600000,
            })
            .await
            .expect("create active upload should succeed");

        // Expired upload: expired 1 hour ago
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: expired_upload.clone(),
                user_id: user_id.clone(),
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now - 7200000,
                expires_at: now - 3600000,
            })
            .await
            .expect("create expired upload should succeed");

        let cleaned = storage.cleanup_expired().await.expect("cleanup_expired should succeed");

        assert_eq!(cleaned, 1, "should clean exactly 1 expired upload");

        let active = storage.get_progress(&active_upload).await.expect("get_progress should succeed");

        assert!(active.is_some(), "active upload should still exist");

        let expired = storage.get_progress(&expired_upload).await.expect("get_progress should succeed");

        assert!(expired.is_none(), "expired upload should be removed");

        cleanup_upload(&pool, &active_upload).await;
    }

    #[tokio::test]
    async fn test_round_trip_full_upload_flow() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let upload_id = format!("upload-roundtrip-{suffix}");
        let user_id = format!("@roundtrip_{suffix}:example.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_upload(&pool, &upload_id).await;

        let now = current_timestamp_millis();
        let expires_at = now + 3600000;

        // Step 1: Create the upload session
        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_id.clone(),
                user_id: user_id.clone(),
                filename: Some("roundtrip.bin".to_string()),
                content_type: Some("application/octet-stream".to_string()),
                total_size: Some(3),
                total_chunks: 3,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create_upload should succeed");

        // Step 2: Store chunks and increment progress for each
        let chunks: Vec<Vec<u8>> = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
        for (i, chunk) in chunks.iter().enumerate() {
            storage
                .store_chunk(StoreUploadChunkRequest {
                    upload_id: upload_id.clone(),
                    chunk_index: i as i32,
                    chunk_data: chunk.clone(),
                    chunk_size: chunk.len() as i64,
                    created_ts: now + i as i64,
                })
                .await
                .expect("store_chunk should succeed");

            storage
                .increment_upload_progress(&upload_id, chunk.len() as i64, now + i as i64)
                .await
                .expect("increment_upload_progress should succeed");
        }

        // Step 3: Verify progress after all chunks
        let progress =
            storage.get_progress(&upload_id).await.expect("get_progress should succeed").expect("upload should exist");

        assert_eq!(progress.uploaded_chunks, 3);
        assert_eq!(progress.uploaded_size, 3);
        // When all chunks are uploaded, status becomes 'complete'
        assert_eq!(progress.status, "complete");

        // Step 4: Load chunk data and verify content
        let loaded_chunks = storage.load_chunk_data(&upload_id).await.expect("load_chunk_data should succeed");

        assert_eq!(loaded_chunks.len(), 3);
        assert_eq!(loaded_chunks[0], b"a");
        assert_eq!(loaded_chunks[1], b"b");
        assert_eq!(loaded_chunks[2], b"c");

        // Combined data should match
        let combined: Vec<u8> = loaded_chunks.iter().flatten().copied().collect();
        assert_eq!(combined, b"abc");

        // Step 5: Finalize
        storage.finalize_upload(&upload_id, current_timestamp_millis()).await.expect("finalize_upload should succeed");

        let finalized = storage
            .get_progress(&upload_id)
            .await
            .expect("get_progress should succeed")
            .expect("upload should still exist");

        assert_eq!(finalized.status, "finalized");
        assert_eq!(finalized.filename.as_deref(), Some("roundtrip.bin"));

        let chunks_after = storage.load_chunk_data(&upload_id).await.expect("load_chunk_data should succeed");

        assert!(chunks_after.is_empty(), "chunks should be cleaned after finalize");

        cleanup_upload(&pool, &upload_id).await;
    }

    #[tokio::test]
    async fn test_list_user_uploads_isolated_per_user() {
        let pool = test_pool().await;
        let storage = ChunkedUploadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_a = format!("@user-a_{suffix}:example.com");
        let user_b = format!("@user-b_{suffix}:example.com");
        let upload_a = format!("upload-isolate-a-{suffix}");
        let upload_b = format!("upload-isolate-b-{suffix}");

        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;
        cleanup_upload(&pool, &upload_a).await;
        cleanup_upload(&pool, &upload_b).await;

        let now = current_timestamp_millis();
        let expires_at = now + 3600000;

        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_a.clone(),
                user_id: user_a.clone(),
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create upload_a should succeed");

        storage
            .create_upload(CreateChunkedUploadRequest {
                upload_id: upload_b.clone(),
                user_id: user_b.clone(),
                filename: None,
                content_type: None,
                total_size: None,
                total_chunks: 1,
                created_ts: now,
                expires_at,
            })
            .await
            .expect("create upload_b should succeed");

        let uploads_a: Vec<String> = storage
            .list_user_uploads(&user_a)
            .await
            .expect("list user_a uploads should succeed")
            .into_iter()
            .map(|u| u.upload_id)
            .collect();

        assert!(uploads_a.contains(&upload_a), "user_a should see own upload");
        assert!(!uploads_a.contains(&upload_b), "user_a should NOT see user_b upload");

        cleanup_upload(&pool, &upload_a).await;
        cleanup_upload(&pool, &upload_b).await;
    }
}
