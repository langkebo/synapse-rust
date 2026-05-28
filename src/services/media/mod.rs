pub mod chunked_upload;

pub use chunked_upload::*;

use crate::common::ApiError;
use crate::services::media_service::MediaService;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct MediaDomainService {
    media_service: MediaService,
    chunked_upload_service: Arc<chunked_upload::ChunkedUploadService>,
}

impl MediaDomainService {
    pub fn new(
        media_service: MediaService,
        chunked_upload_service: Arc<chunked_upload::ChunkedUploadService>,
    ) -> Self {
        Self {
            media_service,
            chunked_upload_service,
        }
    }

    pub async fn upload_media(
        &self,
        user_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> Result<Value, ApiError> {
        self.media_service
            .upload_media(user_id, content, content_type, filename)
            .await
    }

    pub async fn upload_media_with_id(
        &self,
        user_id: &str,
        media_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> Result<Value, ApiError> {
        self.media_service
            .upload_media_with_id(user_id, media_id, content, content_type, filename)
            .await
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
    ) -> Result<chunked_upload::MediaUploadResponse, ApiError> {
        self.chunked_upload_service
            .complete_upload(upload_id, user_id)
            .await
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
