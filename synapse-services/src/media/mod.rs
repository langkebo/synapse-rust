/// The `chunked_upload` module.
pub mod chunked_upload;

pub use chunked_upload::{ChunkedUploadService, CompleteUploadRequest};

// Media domain group — re-exports media_service types under `media::`.
pub use crate::media_service::{MediaService, ThumbnailMethod, ThumbnailSettings};

// P7.4 — additional media-domain service re-exports (previously flat in lib.rs).
pub use crate::content_scanner::*;
pub use crate::media_quota_service::*;
#[cfg(feature = "voice-extended")]
pub use crate::voice_service::{VoiceMessageUploadParams, VoiceService};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::random_string;
use synapse_common::ApiError;
use synapse_storage::media_quota::{QuotaCheckResult, QuotaRejection};

/// The `MediaFinalizationResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFinalizationResponse {
    /// The `media_id` field.
    pub media_id: String,
    /// The `content_uri` field.
    pub content_uri: String,
    /// The `size` field.
    pub size: i64,
}

/// The `MediaResponseHeaders` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaResponseHeaders {
    /// The `content_type` field.
    pub content_type: String,
    /// The `content_length` field.
    pub content_length: usize,
    /// The `content_disposition` field.
    pub content_disposition: String,
    /// The `x_content_type_options` field.
    pub x_content_type_options: &'static str,
    /// The `content_security_policy` field.
    pub content_security_policy: &'static str,
    /// The `cross_origin_resource_policy` field.
    pub cross_origin_resource_policy: &'static str,
    /// The `referrer_policy` field.
    pub referrer_policy: &'static str,
}

/// The `MediaResponsePayload` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaResponsePayload {
    /// The `content` field.
    pub content: Vec<u8>,
    /// The `headers` field.
    pub headers: MediaResponseHeaders,
}

/// Streaming media response — the file is streamed from disk instead of
/// being loaded entirely into memory as `Vec<u8>`.
///
/// This struct is the S3 optimization: it carries an open `tokio::fs::File`
/// handle and the HTTP headers that the web layer needs to build the response.
/// The web layer converts the file to a streaming body via
/// `Body::from_stream(ReaderStream::new(file))`.
pub struct MediaStreamPayload {
    /// Open file handle positioned at byte 0, ready for streaming.
    pub file: tokio::fs::File,
    /// Total content length (file size in bytes).
    pub content_length: u64,
    /// HTTP headers (content-type, disposition, CSP, etc.).
    pub headers: MediaResponseHeaders,
}

/// The `MediaDomainService` struct.
#[derive(Clone)]
pub struct MediaDomainService {
    media_service: MediaService,
    media_quota_service: Arc<MediaQuotaService>,
    chunked_upload_service: Arc<chunked_upload::ChunkedUploadService>,
    quarantine_change_storage: Option<Arc<dyn synapse_storage::media::QuarantinedMediaChangeStoreApi>>,
    cache_invalidation: Option<Arc<synapse_cache::invalidation::CacheInvalidationManager>>,
}

/// Map a refused quota check to the errcode the spec prescribes for that reason.
///
/// - a single file over the per-file cap ⇒ `M_TOO_LARGE` (413): the request body
///   is too large, which is exactly what upstream Synapse's `max_upload_size`
///   enforces;
/// - an aggregate allowance (user quota, server storage) ⇒
///   `M_RESOURCE_LIMIT_EXCEEDED` (403): a resource limit, not malformed input.
///
/// `M_USER_LIMIT_EXCEEDED` is deliberately **not** used: this repo registers it
/// as MSC4335's *user account count* limit (`synapse-common/src/error/code.rs`).
fn quota_error(check: &QuotaCheckResult) -> ApiError {
    let message = check.reason.clone().unwrap_or_else(|| "Media quota exceeded".to_string());
    match check.rejection {
        Some(QuotaRejection::FileTooLarge) => ApiError::too_large(message),
        // `StorageExceeded`, and any future refusal that forgot to name its
        // reason, degrade to the conservative resource-limit code rather than
        // 400 (which would blame the client's JSON).
        _ => ApiError::resource_limit_exceeded(message),
    }
}

impl MediaDomainService {
    /// See [`new`].
    pub fn new(
        media_service: MediaService,
        media_quota_service: Arc<MediaQuotaService>,
        chunked_upload_service: Arc<chunked_upload::ChunkedUploadService>,
    ) -> Self {
        Self {
            media_service,
            media_quota_service,
            chunked_upload_service,
            quarantine_change_storage: None,
            cache_invalidation: None,
        }
    }

    /// Attach quarantine stream storage and cache invalidation support.
    /// This enables the quarantine/unquarantine admin APIs and stream writer
    /// integration for multi-worker deployments.
    pub fn with_quarantine_stream(
        mut self,
        storage: Arc<dyn synapse_storage::media::QuarantinedMediaChangeStoreApi>,
        cache_invalidation: Option<Arc<synapse_cache::invalidation::CacheInvalidationManager>>,
    ) -> Self {
        self.quarantine_change_storage = Some(storage);
        self.cache_invalidation = cache_invalidation;
        self
    }

    /// Quarantine a media item by server_name/media_id.
    /// Records the change in the quarantine stream and updates media_metadata.
    /// Invalidates relevant caches in multi-worker deployments.
    pub async fn quarantine_media(&self, server_name: &str, media_id: &str, changed_by: &str) -> Result<i64, ApiError> {
        let storage = self
            .quarantine_change_storage
            .as_ref()
            .ok_or_else(|| ApiError::internal("Quarantine stream storage not configured"))?;

        let now_ts = current_timestamp_millis();

        // Record the quarantine change in the stream
        let stream_id =
            storage.record_media_quarantine_change(media_id, server_name, "quarantine", changed_by, now_ts).await?;

        // Update the actual quarantine status on the media record
        storage.set_media_quarantine_status(media_id, server_name, "quarantined").await?;

        // Invalidate caches for this media item across workers
        if let Some(cache_mgr) = &self.cache_invalidation {
            let cache_key = format!("media_metadata:{server_name}:{media_id}");
            if let Err(e) = cache_mgr.invalidate_key(&cache_key).await {
                tracing::warn!(
                    error = %e,
                    cache_key = %cache_key,
                    server_name = %server_name,
                    media_id = %media_id,
                    changed_by = %changed_by,
                    "Failed to invalidate media cache after quarantine"
                );
            }
        }

        Ok(stream_id)
    }

    /// Remove quarantine from a media item by server_name/media_id.
    /// Records the change in the quarantine stream and updates media_metadata.
    /// Invalidates relevant caches in multi-worker deployments.
    pub async fn unquarantine_media(
        &self,
        server_name: &str,
        media_id: &str,
        changed_by: &str,
    ) -> Result<i64, ApiError> {
        let storage = self
            .quarantine_change_storage
            .as_ref()
            .ok_or_else(|| ApiError::internal("Quarantine stream storage not configured"))?;

        let now_ts = current_timestamp_millis();

        // Record the unquarantine change in the stream
        let stream_id =
            storage.record_media_quarantine_change(media_id, server_name, "unquarantine", changed_by, now_ts).await?;

        // Update the actual quarantine status on the media record
        storage.set_media_quarantine_status(media_id, server_name, "").await?;

        // Invalidate caches for this media item across workers
        if let Some(cache_mgr) = &self.cache_invalidation {
            let cache_key = format!("media_metadata:{server_name}:{media_id}");
            if let Err(e) = cache_mgr.invalidate_key(&cache_key).await {
                tracing::warn!(
                    error = %e,
                    cache_key = %cache_key,
                    server_name = %server_name,
                    media_id = %media_id,
                    changed_by = %changed_by,
                    "Failed to invalidate media cache after unquarantine"
                );
            }
        }

        Ok(stream_id)
    }

    /// Get quarantine changes since a given stream position.
    /// Used by workers to catch up on quarantine state changes.
    pub async fn get_quarantined_media_changes(
        &self,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<synapse_storage::media::QuarantinedMediaChange>, ApiError> {
        let storage = self
            .quarantine_change_storage
            .as_ref()
            .ok_or_else(|| ApiError::internal("Quarantine stream storage not configured"))?;
        storage.get_quarantined_media_changes(since_stream_id, limit).await
    }

    /// Check if local media is quarantined. Non-admin downloads of quarantined
    /// media return 403 Forbidden. This is a security measure to prevent
    /// malicious content from being served to regular users.
    ///
    /// - For remote media (server_name != local): returns Ok(()), no check.
    /// - For local media by admin: returns Ok(()), admin can access quarantined.
    /// - For local media by non-admin + quarantined: returns 403 Forbidden.
    pub async fn ensure_media_not_quarantined(
        &self,
        is_admin: bool,
        server_name: &str,
        media_id: &str,
    ) -> Result<(), ApiError> {
        // Only check local media; remote media has separate quarantine handling
        if server_name != self.media_service.server_name() {
            return Ok(());
        }

        // Admins can always access quarantined media (for forensic purposes)
        if is_admin {
            return Ok(());
        }

        let storage = self
            .quarantine_change_storage
            .as_ref()
            .ok_or_else(|| ApiError::internal("Quarantine stream storage not configured"))?;

        let is_quarantined = storage.get_media_quarantine_status(media_id, server_name).await?;
        if is_quarantined {
            return Err(ApiError::forbidden(format!(
                "Media {}/{} is quarantined and cannot be accessed by non-admin users",
                server_name, media_id
            )));
        }

        Ok(())
    }

    /// Check the upload against the quota, counting the bytes already reserved by
    /// the user's in-flight chunked uploads.
    ///
    /// `exclude_upload_id` is set when finalizing a chunked upload: that upload's
    /// own reservation must not be counted against itself.
    async fn ensure_upload_allowed(
        &self,
        user_id: &str,
        file_size: i64,
        exclude_upload_id: Option<&str>,
    ) -> Result<(), ApiError> {
        let reserved_bytes = self.chunked_upload_service.pending_bytes(user_id, exclude_upload_id).await?;
        let quota_check =
            self.media_quota_service.check_upload_quota_with_reserved(user_id, file_size, reserved_bytes).await?;

        if !quota_check.is_allowed {
            return Err(quota_error(&quota_check));
        }

        Ok(())
    }

    async fn record_upload_usage(&self, user_id: &str, media_id: &str, file_size: i64, content_type: &str) {
        if let Err(e) = self.media_quota_service.record_upload(user_id, media_id, file_size, Some(content_type)).await {
            tracing::warn!(
                error = %e,
                user_id = %user_id,
                media_id = %media_id,
                file_size,
                content_type = %content_type,
                "Failed to record media quota usage"
            );
        }
    }

    async fn record_delete_usage(&self, user_id: &str, media_id: &str, file_size: i64) {
        if let Err(e) = self.media_quota_service.record_delete(user_id, media_id, file_size).await {
            tracing::warn!(
                error = %e,
                user_id = %user_id,
                media_id = %media_id,
                file_size,
                "Failed to record media quota delete"
            );
        }
    }

    /// See [`upload_media`].
    pub async fn upload_media(
        &self,
        user_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> Result<Value, ApiError> {
        let file_size = content.len() as i64;
        self.ensure_upload_allowed(user_id, file_size, None).await?;

        let response = self.media_service.upload_media(user_id, content, content_type, filename).await?;

        if let Some(media_id) = response
            .get("content_uri")
            .and_then(|value| value.as_str())
            .and_then(|content_uri| content_uri.rsplit('/').next())
        {
            self.record_upload_usage(user_id, media_id, file_size, content_type).await;
        }

        Ok(response)
    }

    /// See [`upload_media_with_id`].
    pub async fn upload_media_with_id(
        &self,
        user_id: &str,
        media_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> Result<Value, ApiError> {
        self.upload_media_with_id_accounted(user_id, media_id, content, content_type, filename, None).await
    }

    /// The only place that stores a file under a known id **and** bills it.
    ///
    /// FUNC-03: keeping the quota check and the usage record together means a new
    /// upload path cannot store bytes without being billed.
    async fn upload_media_with_id_accounted(
        &self,
        user_id: &str,
        media_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
        exclude_upload_id: Option<&str>,
    ) -> Result<Value, ApiError> {
        let file_size = content.len() as i64;
        self.ensure_upload_allowed(user_id, file_size, exclude_upload_id).await?;

        let response =
            self.media_service.upload_media_with_id(user_id, media_id, content, content_type, filename).await?;

        self.record_upload_usage(user_id, media_id, file_size, content_type).await;

        Ok(response)
    }

    /// See [`start_chunked_upload`].
    pub async fn start_chunked_upload(
        &self,
        user_id: &str,
        filename: Option<&str>,
        content_type: Option<&str>,
        total_size: Option<i64>,
        total_chunks: i32,
    ) -> Result<String, ApiError> {
        // FUNC-06: the declared size is the only hook for a quota reservation,
        // so an upload without one must not start at all.
        let total_size = total_size.ok_or_else(|| ApiError::missing_param("total_size is required".to_string()))?;
        if total_size < 0 {
            return Err(ApiError::bad_request("total_size must not be negative".to_string()));
        }
        if total_size > self.chunked_upload_service.max_file_size() {
            return Err(ApiError::too_large(format!(
                "total_size {total_size} exceeds the maximum upload size {}",
                self.chunked_upload_service.max_file_size()
            )));
        }
        if total_size > 0 {
            self.ensure_upload_allowed(user_id, total_size, None).await?;
        }

        self.chunked_upload_service.start_upload(user_id, filename, content_type, Some(total_size), total_chunks).await
    }

    /// See [`upload_chunk`].
    pub async fn upload_chunk(
        &self,
        request: chunked_upload::ChunkUploadRequest,
        user_id: &str,
    ) -> Result<chunked_upload::ChunkUploadResponse, ApiError> {
        self.chunked_upload_service.upload_chunk(request, user_id).await
    }

    /// See [`complete_chunked_upload`].
    pub async fn complete_chunked_upload(
        &self,
        upload_id: &str,
        user_id: &str,
    ) -> Result<MediaFinalizationResponse, ApiError> {
        let completed = self.chunked_upload_service.load_completed_upload(upload_id, user_id).await?;

        let media_id = random_string(32);
        let content_type = completed.content_type.as_deref().unwrap_or("application/octet-stream");
        let size = completed.data.len() as i64;

        // FUNC-03: finalizing is an upload, so it goes through the same
        // check-and-bill path as every other upload. The upload's own reservation
        // is excluded because these bytes are the very bytes it reserved.
        let upload_response = self
            .upload_media_with_id_accounted(
                user_id,
                &media_id,
                &completed.data,
                content_type,
                completed.filename.as_deref(),
                Some(upload_id),
            )
            .await?;

        if let Err(e) = self.chunked_upload_service.mark_upload_finalized(upload_id).await {
            tracing::warn!(
                error = %e,
                upload_id = %upload_id,
                user_id = %user_id,
                media_id = %media_id,
                size,
                content_type = %content_type,
                "Chunked upload stored but failed to finalize progress state"
            );
        }

        let content_uri = upload_response
            .get("content_uri")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("Media upload response missing content_uri"))?
            .to_string();

        Ok(MediaFinalizationResponse { media_id, content_uri, size })
    }

    /// See [`cancel_chunked_upload`].
    pub async fn cancel_chunked_upload(&self, upload_id: &str, user_id: &str) -> Result<(), ApiError> {
        self.chunked_upload_service.cancel_upload(upload_id, user_id).await
    }

    /// See [`get_chunked_upload_progress`].
    pub async fn get_chunked_upload_progress(
        &self,
        upload_id: &str,
    ) -> Result<chunked_upload::UploadProgress, ApiError> {
        self.chunked_upload_service.get_progress(upload_id).await
    }

    /// Sign a media download URL for the given server_name/media_id pair.
    pub fn sign_media_download_url(&self, server_name: &str, media_id: &str) -> Option<String> {
        self.media_service.sign_media_download_url(server_name, media_id)
    }

    /// Verify a signed media download URL.
    pub fn verify_media_download_url(&self, server_name: &str, media_id: &str, signature: &str, expires: u64) -> bool {
        self.media_service.verify_media_download_url(server_name, media_id, signature, expires)
    }

    /// See [`download_media`].
    pub async fn download_media(
        &self,
        server_name: &str,
        media_id: &str,
        response_filename: Option<&str>,
    ) -> Result<MediaResponsePayload, ApiError> {
        let content = self.media_service.download_media(server_name, media_id).await?;
        let metadata = self.media_service.get_media_metadata(server_name, media_id).await.unwrap_or(Value::Null);

        let stored_content_type =
            metadata.get("content_type").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());

        let stored_filename =
            metadata.get("filename").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());
        let response_filename = response_filename.or(stored_filename.as_deref());

        let content_type = stored_content_type.unwrap_or_else(|| {
            guess_content_type(stored_filename.as_deref().unwrap_or(media_id), &content).to_string()
        });

        let headers = build_media_response_headers(content_type, content.len(), response_filename);

        Ok(MediaResponsePayload { content, headers })
    }

    /// Stream-oriented variant of [`download_media`].
    ///
    /// Instead of reading the entire file into a `Vec<u8>`, this method opens
    /// the file with `tokio::fs::File` and returns a [`MediaStreamPayload`]
    /// containing the file handle plus pre-built HTTP headers.  The web layer
    /// converts the handle to a streaming HTTP body, avoiding OOM on large
    /// media downloads.
    ///
    /// Content-type is resolved from stored metadata when available; otherwise
    /// a small prefix (first 1 KB) is read for magic-byte detection and the
    /// file is seeked back to position 0 before returning.
    pub async fn download_media_stream(
        &self,
        server_name: &str,
        media_id: &str,
        response_filename: Option<&str>,
    ) -> Result<MediaStreamPayload, ApiError> {
        let file_path = self
            .media_service
            .get_media_file_path(server_name, media_id)
            .await
            .ok_or_else(|| ApiError::not_found("Media not found".to_string()))?;

        let file = tokio::fs::File::open(&file_path).await.map_err(|e| {
            // A concurrent delete between path resolution and open is a
            // legitimate "gone" condition, not a server fault.
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::not_found("Media not found".to_string())
            } else {
                ApiError::internal(format!("Failed to open media file: {e}"))
            }
        })?;

        let content_length = file.metadata().await.map(|m| m.len()).unwrap_or(0);

        let metadata = self.media_service.get_media_metadata(server_name, media_id).await.unwrap_or(Value::Null);

        let stored_content_type =
            metadata.get("content_type").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());

        let stored_filename =
            metadata.get("filename").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());
        let response_filename = response_filename.or(stored_filename.as_deref());

        // Determine content type: prefer stored metadata, fall back to
        // magic-byte detection from a small prefix read.
        let content_type = if let Some(ct) = stored_content_type {
            ct
        } else {
            // Read up to 1 KB for type detection, then seek back to start.
            use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

            let mut file_for_detection = file;
            let mut prefix = vec![0u8; 1024];
            let n = file_for_detection.read(&mut prefix).await.unwrap_or(0);
            prefix.truncate(n);
            let _ = file_for_detection.seek(SeekFrom::Start(0)).await;

            guess_content_type(stored_filename.as_deref().unwrap_or(media_id), &prefix).to_string()
        };

        let headers = build_media_response_headers(content_type, content_length as usize, response_filename);

        // Re-open the file to ensure a clean handle at position 0.
        // (The detection read may have consumed bytes even after seek.)
        let file = tokio::fs::File::open(&file_path).await.map_err(|e| {
            // The file may have been deleted between the detection read and
            // this re-open; treat as 404 rather than 500.
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::not_found("Media not found".to_string())
            } else {
                ApiError::internal(format!("Failed to reopen media file for streaming: {e}"))
            }
        })?;

        Ok(MediaStreamPayload { file, content_length, headers })
    }

    /// See [`get_thumbnail`].
    pub async fn get_thumbnail(
        &self,
        server_name: &str,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
        animated: bool,
    ) -> Result<MediaResponsePayload, ApiError> {
        let content = self.media_service.get_thumbnail(server_name, media_id, width, height, method, animated).await?;
        let content_type = if animated { "image/webp" } else { "image/jpeg" };
        let headers = build_media_response_headers(content_type.to_string(), content.len(), None);
        Ok(MediaResponsePayload { content, headers })
    }

    /// See [`preview_url`].
    ///
    /// `blacklist` is `url_preview.ip_range_blacklist`; `first_hop` is the caller's
    /// resolution of `url`, reused so the checked address is the fetched one (PERF-13).
    /// Every **redirect hop** is still resolved inside `crate::url_preview`.
    pub async fn preview_url(
        &self,
        url: &str,
        first_hop: Option<&crate::url_preview::VerifiedFirstHop>,
        blacklist: &[String],
    ) -> Result<Value, ApiError> {
        self.media_service.preview_url(url, first_hop, blacklist).await
    }

    /// See [`delete_media_for_user`].
    pub async fn delete_media_for_user(
        &self,
        server_name: &str,
        media_id: &str,
        user_id: &str,
    ) -> Result<(), ApiError> {
        let metadata = self.media_service.get_media_metadata(server_name, media_id).await.unwrap_or(Value::Null);
        let media_info = self.media_service.get_media_info(server_name, media_id).await?;

        let uploader = metadata
            .get("uploader_user_id")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .or_else(|| media_info.get("uploader").and_then(|v| v.as_str()))
            .unwrap_or("");

        if uploader != user_id {
            return Err(ApiError::forbidden("You can only delete your own media".to_string()));
        }

        let file_size = metadata
            .get("size")
            .and_then(|v| v.as_i64())
            .or_else(|| media_info.get("size").and_then(|v| v.as_i64()))
            .unwrap_or(0);

        self.media_service.delete_media(server_name, media_id).await?;
        if file_size > 0 {
            self.record_delete_usage(user_id, media_id, file_size).await;
        }

        Ok(())
    }

    /// See [`get_user_quota`].
    pub async fn get_user_quota(&self, user_id: &str) -> Result<crate::media_quota_service::UserQuotaInfo, ApiError> {
        self.media_quota_service.get_user_quota(user_id).await
    }

    /// See [`get_usage_stats`].
    pub async fn get_usage_stats(&self, user_id: &str) -> Result<Value, ApiError> {
        self.media_quota_service.get_usage_stats(user_id).await
    }

    /// See [`get_user_alerts`].
    pub async fn get_user_alerts(
        &self,
        user_id: &str,
        unread_only: bool,
    ) -> Result<Vec<synapse_storage::media_quota::MediaQuotaAlert>, ApiError> {
        self.media_quota_service.get_user_alerts(user_id, unread_only).await
    }
}

fn guess_content_type(filename: &str, data: &[u8]) -> &'static str {
    if let Some(kind) = infer::get(data) {
        return kind.mime_type();
    }

    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        _ => "application/octet-stream",
    }
}

const MEDIA_CONTENT_SECURITY_POLICY: &str = "sandbox; default-src 'none'; script-src 'none'; \
plugin-types application/pdf; style-src 'unsafe-inline'; media-src 'self'; \
object-src 'self'; img-src 'self';";

const SAFE_INLINE_MEDIA_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "audio/mpeg",
    "audio/wav",
    "audio/ogg",
    "audio/flac",
    "video/mp4",
    "video/webm",
    "application/pdf",
];

fn sanitize_attachment_filename(filename: &str) -> String {
    filename
        .chars()
        .filter(|c| !c.is_control() && !matches!(*c, '"' | '\\' | '/' | '\0'))
        .take(200)
        .collect::<String>()
        .trim()
        .to_string()
}

fn encode_rfc5987(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric()
                || matches!(c, '!' | '#' | '$' | '&' | '+' | '-' | '.' | '^' | '_' | '`' | '|' | '~')
            {
                c.to_string()
            } else {
                format!("{}{:02X}", "%", c as u32)
            }
        })
        .collect()
}

fn build_media_response_headers(
    content_type: String,
    content_length: usize,
    filename: Option<&str>,
) -> MediaResponseHeaders {
    let primary_type = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let inline_safe = SAFE_INLINE_MEDIA_TYPES.iter().any(|safe| *safe == primary_type);

    let disposition_kind = if inline_safe { "inline" } else { "attachment" };
    let content_disposition = match filename {
        Some(name) if !name.is_empty() => {
            let safe = sanitize_attachment_filename(name);
            if safe.is_empty() {
                disposition_kind.to_string()
            } else {
                let encoded = encode_rfc5987(&safe);
                format!("{disposition_kind}; filename=\"{safe}\"; filename*=UTF-8''{encoded}")
            }
        }
        _ => disposition_kind.to_string(),
    };

    MediaResponseHeaders {
        content_type,
        content_length,
        content_disposition,
        x_content_type_options: "nosniff",
        content_security_policy: MEDIA_CONTENT_SECURITY_POLICY,
        cross_origin_resource_policy: "cross-origin",
        referrer_policy: "no-referrer",
    }
}

#[cfg(test)]
mod quarantine_tests;

#[cfg(test)]
mod format_tests;
#[cfg(test)]
mod tests;
