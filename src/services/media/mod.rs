pub mod chunked_upload;

pub use chunked_upload::*;

use crate::common::random_string;
use crate::common::ApiError;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaResponseHeaders {
    pub content_type: String,
    pub content_length: usize,
    pub content_disposition: String,
    pub x_content_type_options: &'static str,
    pub content_security_policy: &'static str,
    pub cross_origin_resource_policy: &'static str,
    pub referrer_policy: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaResponsePayload {
    pub content: Vec<u8>,
    pub headers: MediaResponseHeaders,
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
        Self { media_service, media_quota_service, chunked_upload_service }
    }

    async fn ensure_upload_allowed(&self, user_id: &str, file_size: i64) -> Result<(), ApiError> {
        let quota_check = self.media_quota_service.check_upload_quota(user_id, file_size).await?;

        if !quota_check.is_allowed {
            return Err(ApiError::bad_request(
                quota_check.reason.unwrap_or_else(|| "Media quota exceeded".to_string()),
            ));
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

    pub async fn upload_media(
        &self,
        user_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> Result<Value, ApiError> {
        let file_size = content.len() as i64;
        self.ensure_upload_allowed(user_id, file_size).await?;

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

        let response =
            self.media_service.upload_media_with_id(user_id, media_id, content, content_type, filename).await?;

        self.record_upload_usage(user_id, media_id, file_size, content_type).await;

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
        if let Some(size) = total_size {
            if size < 0 {
                return Err(ApiError::bad_request("total_size must not be negative".to_string()));
            }
            if size > 0 {
                self.ensure_upload_allowed(user_id, size).await?;
            }
        }

        self.chunked_upload_service.start_upload(user_id, filename, content_type, total_size, total_chunks).await
    }

    pub async fn upload_chunk(
        &self,
        request: chunked_upload::ChunkUploadRequest,
        user_id: &str,
    ) -> Result<chunked_upload::ChunkUploadResponse, ApiError> {
        self.chunked_upload_service.upload_chunk(request, user_id).await
    }

    pub async fn complete_chunked_upload(
        &self,
        upload_id: &str,
        user_id: &str,
    ) -> Result<MediaFinalizationResponse, ApiError> {
        let completed = self.chunked_upload_service.load_completed_upload(upload_id, user_id).await?;

        let media_id = random_string(32);
        let content_type = completed.content_type.as_deref().unwrap_or("application/octet-stream");
        let size = completed.data.len() as i64;

        let upload_response = self
            .media_service
            .upload_media_with_id(user_id, &media_id, &completed.data, content_type, completed.filename.as_deref())
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

    pub async fn cancel_chunked_upload(&self, upload_id: &str, user_id: &str) -> Result<(), ApiError> {
        self.chunked_upload_service.cancel_upload(upload_id, user_id).await
    }

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

    pub async fn get_thumbnail(
        &self,
        server_name: &str,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<MediaResponsePayload, ApiError> {
        let content = self.media_service.get_thumbnail(server_name, media_id, width, height, method).await?;
        let headers = build_media_response_headers("image/jpeg".to_string(), content.len(), None);
        Ok(MediaResponsePayload { content, headers })
    }

    pub fn preview_url(&self, url: &str, ts: i64) -> Result<Value, ApiError> {
        self.media_service.preview_url(url, ts)
    }

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

    pub async fn get_user_quota(
        &self,
        user_id: &str,
    ) -> Result<crate::services::media_quota_service::UserQuotaInfo, ApiError> {
        self.media_quota_service.get_user_quota(user_id).await
    }

    pub async fn get_usage_stats(&self, user_id: &str) -> Result<Value, ApiError> {
        self.media_quota_service.get_usage_stats(user_id).await
    }

    pub async fn get_user_alerts(
        &self,
        user_id: &str,
        unread_only: bool,
    ) -> Result<Vec<crate::storage::media_quota::MediaQuotaAlert>, ApiError> {
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
mod tests {
    use super::*;
    use crate::services::media_quota_service::MediaQuotaService;
    use crate::storage::media_quota::{MediaQuotaStorage, SetUserQuotaRequest};
    use crate::storage::user::UserStorage;
    use crate::test_utils;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;
    use std::time::Duration;

    async fn prepare_media_test_pool() -> Result<Arc<sqlx::PgPool>, String> {
        let database_url = test_utils::resolve_test_database_url().await?;
        let schema_name = format!("media_test_{}", uuid::Uuid::new_v4().simple());

        let admin_pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&database_url)
            .await
            .map_err(|error| format!("failed to connect admin pool: {error}"))?;

        sqlx::query(&format!("CREATE SCHEMA {schema_name}"))
            .execute(&admin_pool)
            .await
            .map_err(|error| format!("failed to create schema {schema_name}: {error}"))?;

        let search_path_sql = format!("SET search_path TO {schema_name}, public");
        let pool = Arc::new(
            PgPoolOptions::new()
                .max_connections(4)
                .min_connections(0)
                .acquire_timeout(Duration::from_secs(30))
                .after_connect(move |connection, _meta| {
                    let search_path_sql = search_path_sql.clone();
                    Box::pin(async move {
                        sqlx::query(&search_path_sql).execute(connection).await?;
                        Ok(())
                    })
                })
                .connect(&database_url)
                .await
                .map_err(|error| format!("failed to connect media test pool for {schema_name}: {error}"))?,
        );

        sqlx::raw_sql(
            r"
            CREATE TABLE users (
                user_id TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                password_hash TEXT,
                is_admin BOOLEAN NOT NULL DEFAULT FALSE,
                is_guest BOOLEAN NOT NULL DEFAULT FALSE,
                is_shadow_banned BOOLEAN NOT NULL DEFAULT FALSE,
                is_deactivated BOOLEAN NOT NULL DEFAULT FALSE,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                displayname TEXT,
                avatar_url TEXT,
                email TEXT,
                phone TEXT,
                generation BIGINT NOT NULL DEFAULT 0,
                consent_version TEXT,
                appservice_id TEXT,
                user_type TEXT,
                invalid_update_at BIGINT,
                migration_state TEXT,
                password_changed_ts BIGINT,
                is_password_change_required BOOLEAN NOT NULL DEFAULT FALSE,
                password_expires_at BIGINT,
                failed_login_attempts INTEGER NOT NULL DEFAULT 0,
                locked_until BIGINT,
                must_change_password BOOLEAN NOT NULL DEFAULT FALSE
            );

            CREATE TABLE media_metadata (
                media_id TEXT PRIMARY KEY,
                server_name TEXT NOT NULL,
                content_type TEXT NOT NULL,
                file_name TEXT,
                size BIGINT NOT NULL,
                uploader_user_id TEXT,
                created_ts BIGINT NOT NULL,
                last_accessed_at BIGINT,
                quarantine_status TEXT
            );

            CREATE TABLE upload_progress (
                upload_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                filename TEXT,
                content_type TEXT,
                total_size BIGINT,
                uploaded_size BIGINT NOT NULL DEFAULT 0,
                total_chunks INTEGER NOT NULL,
                uploaded_chunks INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'pending',
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                expires_at BIGINT NOT NULL
            );

            CREATE TABLE upload_chunks (
                upload_id TEXT NOT NULL REFERENCES upload_progress(upload_id) ON DELETE CASCADE,
                chunk_index INTEGER NOT NULL,
                chunk_data BYTEA NOT NULL,
                chunk_size BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                PRIMARY KEY (upload_id, chunk_index)
            );

            CREATE TABLE media_quota_config (
                id BIGSERIAL PRIMARY KEY,
                config_name TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL DEFAULT 'default',
                description TEXT,
                max_file_size BIGINT DEFAULT 10485760,
                max_upload_rate BIGINT,
                allowed_content_types TEXT[] DEFAULT ARRAY[]::TEXT[],
                retention_days INTEGER DEFAULT 90,
                max_storage_bytes BIGINT NOT NULL DEFAULT 10737418240,
                max_file_size_bytes BIGINT NOT NULL DEFAULT 10485760,
                max_files_count INTEGER NOT NULL DEFAULT 10000,
                allowed_mime_types JSONB NOT NULL DEFAULT '[]'::jsonb,
                blocked_mime_types JSONB NOT NULL DEFAULT '[]'::jsonb,
                is_default BOOLEAN NOT NULL DEFAULT FALSE,
                is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
                created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
                updated_ts BIGINT
            );

            CREATE TABLE user_media_quota (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL UNIQUE REFERENCES users(user_id) ON DELETE CASCADE,
                max_bytes BIGINT DEFAULT 1073741824,
                used_bytes BIGINT DEFAULT 0,
                file_count INTEGER DEFAULT 0,
                quota_config_id BIGINT,
                custom_max_storage_bytes BIGINT,
                custom_max_file_size_bytes BIGINT,
                custom_max_files_count INTEGER,
                current_storage_bytes BIGINT NOT NULL DEFAULT 0,
                current_files_count INTEGER NOT NULL DEFAULT 0,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT
            );

            CREATE TABLE media_usage_log (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                media_id TEXT NOT NULL,
                file_size_bytes BIGINT NOT NULL,
                mime_type TEXT,
                operation TEXT NOT NULL,
                timestamp BIGINT NOT NULL
            );

            CREATE TABLE media_quota_alerts (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                alert_type TEXT NOT NULL,
                threshold_percent INTEGER NOT NULL,
                current_usage_bytes BIGINT NOT NULL,
                quota_limit_bytes BIGINT NOT NULL,
                message TEXT,
                is_read BOOLEAN NOT NULL DEFAULT FALSE,
                created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
            );

            CREATE TABLE server_media_quota (
                id BIGSERIAL PRIMARY KEY,
                max_storage_bytes BIGINT,
                max_file_size_bytes BIGINT,
                max_files_count INTEGER,
                current_storage_bytes BIGINT NOT NULL DEFAULT 0,
                current_files_count INTEGER NOT NULL DEFAULT 0,
                alert_threshold_percent INTEGER NOT NULL DEFAULT 80,
                updated_ts BIGINT NOT NULL
            );

            INSERT INTO server_media_quota (
                id, max_storage_bytes, max_file_size_bytes, max_files_count,
                current_storage_bytes, current_files_count, alert_threshold_percent, updated_ts
            )
            VALUES (1, 10995116277760, 1073741824, 1000000, 0, 0, 80, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000);
            ",
        )
        .execute(pool.as_ref())
        .await
        .map_err(|error| format!("failed to create media test schema objects: {error}"))?;

        Ok(pool)
    }

    async fn setup_test_media_domain_users_with_quota(
        usernames: &[&str],
        max_storage_bytes: i64,
        max_file_size_bytes: i64,
    ) -> (MediaDomainService, MediaService, Vec<crate::storage::user::User>, tempfile::TempDir) {
        let pool = prepare_media_test_pool().await.expect("failed to prepare media test pool");
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let media_path = temp_dir.path().to_str().expect("temp dir path should be valid utf-8");

        let user_storage =
            UserStorage::new(&pool, Arc::new(synapse_cache::CacheManager::new(&synapse_cache::CacheConfig::default())));
        let mut users = Vec::new();
        for username in usernames {
            let user_id = format!("@{username}:test.server");
            let user = user_storage
                .create_user(&user_id, username, Some("password"), false)
                .await
                .expect("Failed to create test user");
            users.push(user);
        }

        let media_service = MediaService::with_pool(media_path, None, "test.server", Some(pool.clone()));
        let media_quota_storage = Arc::new(MediaQuotaStorage::new(&pool));
        let media_quota_service = Arc::new(MediaQuotaService::new(media_quota_storage));
        for user in &users {
            media_quota_service
                .set_user_quota(SetUserQuotaRequest {
                    user_id: user.user_id.clone(),
                    quota_config_id: None,
                    custom_max_storage_bytes: Some(max_storage_bytes),
                    custom_max_file_size_bytes: Some(max_file_size_bytes),
                    custom_max_files_count: Some(10),
                })
                .await
                .expect("failed to set user quota");
        }

        let chunked_upload_service = Arc::new(chunked_upload::ChunkedUploadService::new(pool.clone()));
        let media_domain_service =
            MediaDomainService::new(media_service.clone(), media_quota_service, chunked_upload_service);

        (media_domain_service, media_service, users, temp_dir)
    }

    async fn setup_test_media_domain(
        username: &str,
    ) -> (MediaDomainService, MediaService, crate::storage::user::User, tempfile::TempDir) {
        let (media_domain_service, media_service, mut users, temp_dir) =
            setup_test_media_domain_users_with_quota(&[username], 10 * 1024 * 1024, 10 * 1024 * 1024).await;
        (media_domain_service, media_service, users.remove(0), temp_dir)
    }

    async fn setup_test_media_domain_with_quota(
        username: &str,
        max_storage_bytes: i64,
        max_file_size_bytes: i64,
    ) -> (MediaDomainService, MediaService, crate::storage::user::User, tempfile::TempDir) {
        let (media_domain_service, media_service, mut users, temp_dir) =
            setup_test_media_domain_users_with_quota(&[username], max_storage_bytes, max_file_size_bytes).await;
        (media_domain_service, media_service, users.remove(0), temp_dir)
    }

    #[tokio::test]
    async fn test_chunked_complete_can_be_downloaded_via_media_service() {
        let (media_domain_service, media_service, user, _temp_dir) = setup_test_media_domain("chunk_tester").await;

        let user_id = &user.user_id;
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

        let downloaded = media_domain_service
            .download_media("test.server", &response.media_id, None)
            .await
            .expect("failed to download finalized media");

        assert_eq!(downloaded.content, b"hello world");
        assert_eq!(downloaded.headers.content_type, "text/plain");
        assert_eq!(
            downloaded.headers.content_disposition,
            "attachment; filename=\"greeting.txt\"; filename*=UTF-8''greeting.txt"
        );

        let raw_download = media_service
            .download_media("test.server", &response.media_id)
            .await
            .expect("failed to download finalized media directly");
        assert_eq!(raw_download, b"hello world");

        let progress = media_domain_service
            .get_chunked_upload_progress(&upload_id)
            .await
            .expect("finalized progress record should remain accessible");
        assert_eq!(progress.status, "finalized");
    }

    #[tokio::test]
    async fn test_delete_media_rolls_back_quota_usage() {
        let (media_domain_service, _media_service, user, _temp_dir) =
            setup_test_media_domain("quota_delete_tester").await;

        let upload = media_domain_service
            .upload_media(&user.user_id, b"delete me", "text/plain", Some("delete-me.txt"))
            .await
            .expect("failed to upload media");

        let media_id = upload
            .get("content_uri")
            .and_then(|v| v.as_str())
            .and_then(|content_uri| content_uri.rsplit('/').next())
            .expect("upload response should contain media_id")
            .to_string();

        let quota_after_upload =
            media_domain_service.get_user_quota(&user.user_id).await.expect("failed to load quota after upload");
        assert_eq!(quota_after_upload.current_storage_bytes, 9);
        assert_eq!(quota_after_upload.current_files_count, 1);

        media_domain_service
            .delete_media_for_user("test.server", &media_id, &user.user_id)
            .await
            .expect("failed to delete uploaded media");

        let quota_after_delete =
            media_domain_service.get_user_quota(&user.user_id).await.expect("failed to load quota after delete");
        assert_eq!(quota_after_delete.current_storage_bytes, 0);
        assert_eq!(quota_after_delete.current_files_count, 0);
    }

    #[tokio::test]
    async fn test_start_chunked_upload_rejects_when_quota_would_be_exceeded() {
        let (media_domain_service, _media_service, user, _temp_dir) =
            setup_test_media_domain_with_quota("quota_reject_tester", 4, 4).await;

        let error = media_domain_service
            .start_chunked_upload(&user.user_id, Some("too-large.txt"), Some("text/plain"), Some(5), 1)
            .await
            .expect_err("chunked upload start should fail when quota is exceeded");

        assert_eq!(error.http_status(), axum::http::StatusCode::BAD_REQUEST);
        assert!(
            error.message().contains("File size 5 exceeds maximum allowed size 4")
                || error.message().contains("Quota exceeded")
                || error.message().contains("quota"),
            "unexpected error message: {}",
            error.message()
        );
    }

    #[tokio::test]
    async fn test_start_chunked_upload_rejects_negative_total_size() {
        let (media_domain_service, _media_service, user, _temp_dir) =
            setup_test_media_domain("negative_size_tester").await;

        let error = media_domain_service
            .start_chunked_upload(&user.user_id, Some("bad.txt"), Some("text/plain"), Some(-1), 1)
            .await
            .expect_err("negative total_size should be rejected");

        assert_eq!(error.http_status(), axum::http::StatusCode::BAD_REQUEST);
        assert!(error.message().contains("total_size must not be negative"));
    }

    #[tokio::test]
    async fn test_delete_media_for_other_user_returns_forbidden() {
        let (media_domain_service, _media_service, users, _temp_dir) = setup_test_media_domain_users_with_quota(
            &["media_owner_tester", "media_intruder_tester"],
            10 * 1024 * 1024,
            10 * 1024 * 1024,
        )
        .await;
        let owner = &users[0];
        let intruder = &users[1];

        let upload = media_domain_service
            .upload_media(&owner.user_id, b"private media", "text/plain", Some("private.txt"))
            .await
            .expect("failed to upload owner media");

        let media_id = upload
            .get("content_uri")
            .and_then(|v| v.as_str())
            .and_then(|content_uri| content_uri.rsplit('/').next())
            .expect("upload response should contain media_id");

        let error = media_domain_service
            .delete_media_for_user("test.server", media_id, &intruder.user_id)
            .await
            .expect_err("deleting another user's media should be forbidden");

        assert_eq!(error.http_status(), axum::http::StatusCode::FORBIDDEN);
        assert!(error.message().contains("only delete your own media"));

        let downloaded = media_domain_service
            .download_media("test.server", media_id, None)
            .await
            .expect("media should remain downloadable after forbidden delete");
        assert_eq!(downloaded.content, b"private media");
    }

    #[test]
    fn test_guess_content_type_prefers_detected_bytes_over_filename_extension() {
        let png_bytes = b"\x89PNG\r\n\x1a\nrest";

        assert_eq!(guess_content_type("document.txt", png_bytes), "image/png");
    }

    #[test]
    fn test_guess_content_type_falls_back_to_filename_extension() {
        assert_eq!(guess_content_type("notes.json", b"plain text"), "application/json");
        assert_eq!(guess_content_type("archive.unknown", b"plain text"), "application/octet-stream");
    }

    #[test]
    fn test_sanitize_attachment_filename_removes_control_and_path_characters() {
        let sanitized = sanitize_attachment_filename("..\u{0000}/bad\\name\".txt");

        assert_eq!(sanitized, "..badname.txt");
    }

    #[test]
    fn test_build_media_response_headers_uses_inline_for_safe_media_types() {
        let headers = build_media_response_headers("image/png".to_string(), 42, Some("photo.png"));

        assert_eq!(headers.content_type, "image/png");
        assert_eq!(headers.content_length, 42);
        assert_eq!(headers.content_disposition, "inline; filename=\"photo.png\"; filename*=UTF-8''photo.png");
        assert_eq!(headers.x_content_type_options, "nosniff");
    }

    #[test]
    fn test_build_media_response_headers_uses_attachment_for_unsafe_types_and_encodes_filename() {
        let headers = build_media_response_headers("text/html".to_string(), 99, Some("report 2026.html"));

        assert_eq!(
            headers.content_disposition,
            "attachment; filename=\"report 2026.html\"; filename*=UTF-8''report%202026.html"
        );
        assert_eq!(headers.cross_origin_resource_policy, "cross-origin");
        assert_eq!(headers.referrer_policy, "no-referrer");
    }

    #[test]
    fn test_build_media_response_headers_falls_back_to_kind_when_filename_sanitizes_empty() {
        let headers = build_media_response_headers("text/html".to_string(), 7, Some("/\\\"\u{0000}"));

        assert_eq!(headers.content_disposition, "attachment");
    }

    #[test]
    fn test_build_media_response_headers_treats_safe_media_type_case_insensitively() {
        let headers = build_media_response_headers("IMAGE/PNG; charset=binary".to_string(), 5, Some("photo.png"));

        assert_eq!(headers.content_disposition, "inline; filename=\"photo.png\"; filename*=UTF-8''photo.png");
    }
}
