pub mod chunked_upload;

pub use chunked_upload::*;

use crate::common::ApiError;
use crate::common::random_string;
use crate::services::media_quota_service::MediaQuotaService;
use crate::services::media_service::MediaService;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFinalizationResponse {
    pub media_id: String,
    pub content_uri: String,
    pub size: i64,
}

#[derive(Clone)]
pub struct MediaDomainService {
    media_service: MediaService,
    media_quota_service: Arc<MediaQuotaService>,
    chunked_upload_service: Arc<chunked_upload::ChunkedUploadService>,
}

impl MediaDomainService {
    pub fn new(
        media_service: MediaService,
        media_quota_service: Arc<MediaQuotaService>,
        chunked_upload_service: Arc<chunked_upload::ChunkedUploadService>,
    ) -> Self {
        Self {
            media_service,
            media_quota_service,
            chunked_upload_service,
        }
    }

    async fn ensure_upload_allowed(&self, user_id: &str, file_size: i64) -> Result<(), ApiError> {
        let quota_check = self
            .media_quota_service
            .check_upload_quota(user_id, file_size)
            .await?;

        if !quota_check.is_allowed {
            return Err(ApiError::bad_request(
                quota_check
                    .reason
                    .unwrap_or_else(|| "Media quota exceeded".to_string()),
            ));
        }

        Ok(())
    }

    async fn record_upload_usage(
        &self,
        user_id: &str,
        media_id: &str,
        file_size: i64,
        content_type: &str,
    ) {
        if let Err(e) = self
            .media_quota_service
            .record_upload(user_id, media_id, file_size, Some(content_type))
            .await
        {
            tracing::warn!(
                "Failed to record media quota usage for user {} and media {}: {}",
                user_id,
                media_id,
                e
            );
        }
    }

    pub async fn upload_media(
        &self,
        user_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> Result<Value, ApiError> {
        let file_size = content.len() as i64;
        self.ensure_upload_allowed(user_id, file_size).await?;

        let response = self
            .media_service
            .upload_media(user_id, content, content_type, filename)
            .await?;

        if let Some(media_id) = response
            .get("content_uri")
            .and_then(|value| value.as_str())
            .and_then(|content_uri| content_uri.rsplit('/').next())
        {
            self.record_upload_usage(user_id, media_id, file_size, content_type)
                .await;
        }

        Ok(response)
    }

    pub async fn upload_media_with_id(
        &self,
        user_id: &str,
        media_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> Result<Value, ApiError> {
        let file_size = content.len() as i64;
        self.ensure_upload_allowed(user_id, file_size).await?;

        let response = self
            .media_service
            .upload_media_with_id(user_id, media_id, content, content_type, filename)
            .await?;

        self.record_upload_usage(user_id, media_id, file_size, content_type)
            .await;

        Ok(response)
    }

    pub async fn start_chunked_upload(
        &self,
        user_id: &str,
        filename: Option<&str>,
        content_type: Option<&str>,
        total_size: Option<i64>,
        total_chunks: i32,
    ) -> Result<String, ApiError> {
        self.chunked_upload_service
            .start_upload(user_id, filename, content_type, total_size, total_chunks)
            .await
    }

    pub async fn upload_chunk(
        &self,
        request: chunked_upload::ChunkUploadRequest,
        user_id: &str,
    ) -> Result<chunked_upload::ChunkUploadResponse, ApiError> {
        self.chunked_upload_service
            .upload_chunk(request, user_id)
            .await
    }

    pub async fn complete_chunked_upload(
        &self,
        upload_id: &str,
        user_id: &str,
    ) -> Result<MediaFinalizationResponse, ApiError> {
        let completed = self
            .chunked_upload_service
            .load_completed_upload(upload_id, user_id)
            .await?;

        let media_id = random_string(32);
        let content_type = completed
            .content_type
            .as_deref()
            .unwrap_or("application/octet-stream");
        let size = completed.data.len() as i64;

        let upload_response = self
            .media_service
            .upload_media_with_id(
                user_id,
                &media_id,
                &completed.data,
                content_type,
                completed.filename.as_deref(),
            )
            .await?;

        if let Err(e) = self.chunked_upload_service.mark_upload_finalized(upload_id).await {
            tracing::warn!(
                "Chunked upload {} stored as media {} but failed to finalize progress state: {}",
                upload_id,
                media_id,
                e
            );
        }

        let content_uri = upload_response
            .get("content_uri")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("Media upload response missing content_uri"))?
            .to_string();

        Ok(MediaFinalizationResponse {
            media_id,
            content_uri,
            size,
        })
    }

    pub async fn cancel_chunked_upload(
        &self,
        upload_id: &str,
        user_id: &str,
    ) -> Result<(), ApiError> {
        self.chunked_upload_service
            .cancel_upload(upload_id, user_id)
            .await
    }

    pub async fn get_chunked_upload_progress(
        &self,
        upload_id: &str,
    ) -> Result<chunked_upload::UploadProgress, ApiError> {
        self.chunked_upload_service.get_progress(upload_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::media_quota_service::MediaQuotaService;
    use crate::storage::media_quota::{MediaQuotaStorage, SetUserQuotaRequest};
    use crate::test_utils;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_chunked_complete_can_be_downloaded_via_media_service() {
        let pool = test_utils::prepare_isolated_test_pool()
            .await
            .expect("failed to prepare isolated test pool");
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let media_path = temp_dir.path().to_str().expect("temp dir path should be valid utf-8");

        let media_service = MediaService::with_pool(media_path, None, "test.server", Some(pool.clone()));
        let media_quota_storage = Arc::new(MediaQuotaStorage::new(&pool));
        let media_quota_service = Arc::new(MediaQuotaService::new(media_quota_storage));
        media_quota_service
            .set_user_quota(SetUserQuotaRequest {
                user_id: "@chunk:test.server".to_string(),
                quota_config_id: None,
                custom_max_storage_bytes: Some(10 * 1024 * 1024),
                custom_max_file_size_bytes: Some(10 * 1024 * 1024),
                custom_max_files_count: Some(10),
            })
            .await
            .expect("failed to set user quota");

        let chunked_upload_service = Arc::new(chunked_upload::ChunkedUploadService::new(pool.clone()));
        let media_domain_service = MediaDomainService::new(
            media_service.clone(),
            media_quota_service,
            chunked_upload_service,
        );

        let user_id = "@chunk:test.server";
        let first_chunk = b"hello ".to_vec();
        let second_chunk = b"world".to_vec();

        let upload_id = media_domain_service
            .start_chunked_upload(
                user_id,
                Some("greeting.txt"),
                Some("text/plain"),
                Some((first_chunk.len() + second_chunk.len()) as i64),
                2,
            )
            .await
            .expect("failed to start chunked upload");

        media_domain_service
            .upload_chunk(
                chunked_upload::ChunkUploadRequest {
                    upload_id: Some(upload_id.clone()),
                    chunk_index: 0,
                    total_chunks: 2,
                    chunk_data: first_chunk,
                    filename: Some("greeting.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                    total_size: Some(11),
                },
                user_id,
            )
            .await
            .expect("failed to upload first chunk");

        media_domain_service
            .upload_chunk(
                chunked_upload::ChunkUploadRequest {
                    upload_id: Some(upload_id.clone()),
                    chunk_index: 1,
                    total_chunks: 2,
                    chunk_data: second_chunk,
                    filename: Some("greeting.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                    total_size: Some(11),
                },
                user_id,
            )
            .await
            .expect("failed to upload second chunk");

        let response = media_domain_service
            .complete_chunked_upload(&upload_id, user_id)
            .await
            .expect("failed to finalize chunked upload");

        let downloaded = media_service
            .download_media("test.server", &response.media_id)
            .await
            .expect("failed to download finalized media");

        assert_eq!(downloaded, b"hello world");

        let metadata = media_service
            .get_media_metadata("test.server", &response.media_id)
            .await
            .expect("media metadata should exist");
        assert_eq!(metadata.get("content_type").and_then(|v| v.as_str()), Some("text/plain"));

        let progress = media_domain_service
            .get_chunked_upload_progress(&upload_id)
            .await
            .expect("finalized progress record should remain accessible");
        assert_eq!(progress.status, "finalized");
    }
}
