use synapse_common::background_job::BackgroundJob;
use synapse_common::current_timestamp_millis;
use synapse_common::media_link_signer::MediaLinkSigner;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_common::*;

use sqlx::PgPool;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use synapse_storage::admin_media::AdminMediaStorage;

/// The `ThumbnailMethod` enum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThumbnailMethod {
    /// The `Crop` variant.
    Crop,
    /// The `Scale` variant.
    Scale,
}

impl FromStr for ThumbnailMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "crop" => Ok(Self::Crop),
            "scale" => Ok(Self::Scale),
            _ => Err(format!("Invalid thumbnail method: {s}")),
        }
    }
}

/// The `ThumbnailSettings` struct.
#[derive(Debug, Clone)]
pub struct ThumbnailSettings {
    /// The `width` field.
    pub width: u32,
    /// The `height` field.
    pub height: u32,
    /// The `method` field.
    pub method: ThumbnailMethod,
    /// The `quality` field.
    pub quality: u8,
}

/// Type alias `ThumbnailConfig`.
#[deprecated(since = "0.1.0", note = "Use ThumbnailSettings instead to avoid confusion with config::ThumbnailConfig")]
pub type ThumbnailConfig = ThumbnailSettings;

impl Default for ThumbnailSettings {
    fn default() -> Self {
        Self { width: 800, height: 600, method: ThumbnailMethod::Scale, quality: 80 }
    }
}

/// The `MediaService` struct.
#[derive(Clone)]
pub struct MediaService {
    media_path: PathBuf,
    thumbnail_path: PathBuf,
    task_queue: Option<Arc<RedisTaskQueue>>,
    default_thumbnail_configs: Vec<ThumbnailSettings>,
    server_name: String,
    admin_media_storage: Option<AdminMediaStorage>,
    link_signer: Option<Arc<MediaLinkSigner>>,
}

impl MediaService {
    /// Reject media_id values that could escape the media directory or
    /// otherwise be embedded into a filesystem path. Matrix MXC IDs are
    /// `[A-Za-z0-9_-]+` per spec; we additionally accept `+`/`=` for the
    /// common base64-derived ids. Everything else (including '/', '\\',
    /// '..', NUL, control bytes) is rejected.
    fn validate_media_id(media_id: &str) -> Result<(), ApiError> {
        if media_id.is_empty() || media_id.len() > 255 {
            return Err(ApiError::bad_request("media_id must be 1..=255 chars".to_string()));
        }
        let ok = media_id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'='));
        if !ok {
            return Err(ApiError::bad_request("media_id contains illegal characters".to_string()));
        }
        Ok(())
    }

    /// See [`new`].
    /// See [`new`].
    pub fn new(media_path: &str, task_queue: Option<Arc<RedisTaskQueue>>, server_name: &str) -> Self {
        Self::with_pool(media_path, task_queue, server_name, None)
    }

    /// See [`with_pool`].
    pub fn with_pool(
        media_path: &str,
        task_queue: Option<Arc<RedisTaskQueue>>,
        server_name: &str,
        pool: Option<Arc<PgPool>>,
    ) -> Self {
        let path = PathBuf::from(media_path);
        let thumbnail_path = path.join("thumbnails");

        ::tracing::info!(media_path = %media_path, server_name = %server_name, "Initializing media service");
        ::tracing::info!(media_path = %path.display(), path_exists = path.exists(), "Checked media path");

        if !path.exists() {
            ::tracing::info!(media_dir = %path.display(), "Attempting to create media directory");
            if let Err(e) = std::fs::create_dir_all(&path) {
                ::tracing::error!(error = %e, media_dir = %path.display(), "Failed to create media directory");
            } else {
                ::tracing::info!(media_dir = %path.display(), "Created media directory");
            }
        }

        if !thumbnail_path.exists() {
            if let Err(e) = std::fs::create_dir_all(&thumbnail_path) {
                ::tracing::error!(error = %e, thumbnail_dir = %thumbnail_path.display(), "Failed to create thumbnail directory");
            }
        }

        let default_thumbnail_configs = vec![
            ThumbnailSettings { width: 32, height: 32, method: ThumbnailMethod::Crop, quality: 70 },
            ThumbnailSettings { width: 96, height: 96, method: ThumbnailMethod::Crop, quality: 70 },
            ThumbnailSettings { width: 320, height: 240, method: ThumbnailMethod::Scale, quality: 80 },
            ThumbnailSettings { width: 640, height: 480, method: ThumbnailMethod::Scale, quality: 80 },
            ThumbnailSettings { width: 800, height: 600, method: ThumbnailMethod::Scale, quality: 80 },
        ];

        Self {
            media_path: path,
            thumbnail_path,
            task_queue,
            default_thumbnail_configs,
            server_name: server_name.to_string(),
            admin_media_storage: pool.as_ref().map(|p| AdminMediaStorage::new(p)),
            link_signer: None,
        }
    }

    /// Set the media link signer for signing download URLs.
    pub fn set_link_signer(&mut self, signer: Arc<MediaLinkSigner>) {
        self.link_signer = Some(signer);
    }

    /// Sign a media download URL for the given server_name/media_id pair.
    /// Returns a query string like `signature=...&expires=...`.
    pub fn sign_media_download_url(&self, server_name: &str, media_id: &str) -> Option<String> {
        let signer = self.link_signer.as_ref()?;
        let path = format!("{server_name}/{media_id}");
        Some(signer.sign(&path))
    }

    /// Verify a signed media download URL.
    pub fn verify_media_download_url(&self, server_name: &str, media_id: &str, signature: &str, expires: u64) -> bool {
        let signer = match self.link_signer.as_ref() {
            Some(s) => s,
            None => return false,
        };
        let path = format!("{server_name}/{media_id}");
        signer.verify(&path, signature, expires)
    }

    /// See [`upload_media`].
    pub async fn upload_media(
        &self,
        user_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let media_id = random_string(32);
        self.store_media_with_id(user_id, &media_id, content, content_type, filename).await
    }

    /// See [`upload_media_with_id`].
    pub async fn upload_media_with_id(
        &self,
        user_id: &str,
        media_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        Self::validate_media_id(media_id)?;
        self.store_media_with_id(user_id, media_id, content, content_type, filename).await
    }

    async fn store_media_with_id(
        &self,
        user_id: &str,
        media_id: &str,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let extension = Self::get_extension_from_content_type(content_type);
        let file_name = if let Some(fname) = filename {
            let safe: String = fname
                .chars()
                .filter(|c: &char| !c.is_control() && *c != '\0' && *c != '/' && *c != '\\')
                .take(200)
                .collect::<String>();
            if safe.is_empty() {
                format!("{media_id}.{extension}")
            } else {
                format!("{media_id}_{safe}")
            }
        } else {
            format!("{media_id}.{extension}")
        };
        let file_path = self.media_path.join(&file_name);
        let media_path_display = self.media_path.display().to_string();

        ::tracing::info!(
            media_id = %media_id,
            user_id = %user_id,
            file_name = %file_name,
            content_type = %content_type,
            size = content.len(),
            file_path = %file_path.display(),
            "Uploading media"
        );

        if !self.media_path.exists() {
            ::tracing::warn!(
                media_id = %media_id,
                user_id = %user_id,
                media_path = %self.media_path.display(),
                "Media path does not exist, attempting to create"
            );
            if let Err(e) = std::fs::create_dir_all(&self.media_path) {
                ::tracing::error!(
                    media_id = %media_id,
                    user_id = %user_id,
                    media_path = %self.media_path.display(),
                    error = %e,
                    "Failed to create media directory"
                );
                return Err(ApiError::internal("An internal error occurred".to_string()));
            }
        }

        if self.find_media_file_name(media_id).await?.is_some() {
            return Err(ApiError::conflict(format!("Media ID already exists: {media_id}")));
        }

        let content_vec = content.to_vec();
        let write_result: Result<(), std::io::Error> =
            tokio::task::spawn_blocking(move || std::fs::write(&file_path, content_vec))
                .await
                .map_err(|e| ApiError::internal_with_context("Write task panicked", &e))?;

        if let Err(e) = write_result {
            ::tracing::error!(
                media_id = %media_id,
                user_id = %user_id,
                file_name = %file_name,
                content_type = %content_type,
                size = content.len(),
                error = %e,
                "Failed to save media file"
            );

            let error_msg = match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    format!(
                        "Permission denied writing to media directory. Please run: chmod 755 {media_path_display} && chown -R synapse:synapse {media_path_display}"
                    )
                }
                std::io::ErrorKind::NotFound => {
                    format!("Media directory not found: {media_path_display}")
                }
                std::io::ErrorKind::StorageFull => "Storage full. Please free up disk space.".to_string(),
                _ => format!("Failed to save media: {e}"),
            };

            return Err(ApiError::internal(error_msg));
        }

        ::tracing::info!(
            media_id = %media_id,
            user_id = %user_id,
            file_name = %file_name,
            content_type = %content_type,
            size = content.len(),
            "Saved media file"
        );

        if let Some(storage) = &self.admin_media_storage {
            let now = current_timestamp_millis();
            if let Err(e) = storage
                .upsert_media_metadata(
                    media_id,
                    &self.server_name,
                    content_type,
                    filename.unwrap_or(&file_name),
                    content.len() as i64,
                    user_id,
                    now,
                )
                .await
            {
                ::tracing::warn!(
                    media_id = %media_id,
                    user_id = %user_id,
                    file_name = %file_name,
                    content_type = %content_type,
                    error = %e,
                    "Failed to store media metadata in DB"
                );
            }
        }

        if let Some(queue) = &self.task_queue {
            let job = BackgroundJob::ProcessMedia { file_id: file_name.clone() };
            if let Err(e) = queue.submit(job).await {
                ::tracing::warn!(
                    media_id = %media_id,
                    user_id = %user_id,
                    file_name = %file_name,
                    error = %e,
                    "Failed to submit media processing task"
                );
            } else {
                ::tracing::info!(media_id = %media_id, file_name = %file_name, "Submitted media processing task");
            }
        }

        let media_url = synapse_common::media_locator::MediaLocator {
            server_name: self.server_name.clone(),
            media_id: media_id.to_string(),
        }
        .to_mxc_url();

        Ok(serde_json::json!({
            "content_uri": media_url,
            "media_id": media_id
        }))
    }

    async fn find_media_file_name(&self, media_id: &str) -> ApiResult<Option<String>> {
        let media_path = self.media_path.clone();
        let media_id = media_id.to_string();

        tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(&media_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if media_file_matches_id(file_name, &media_id) {
                            return Some(file_name.to_string());
                        }
                    }
                }
            }
            None
        })
        .await
        .map_err(|e| ApiError::internal_with_context("Task error", &e))
    }

    /// See [`get_media`].
    /// See [`get_media`].
    pub async fn get_media(&self, _server_name: &str, media_id: &str) -> Option<Vec<u8>> {
        let media_path = self.media_path.clone();
        let media_id = media_id.to_string();

        tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(media_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if media_file_matches_id(file_name, &media_id) {
                            if let Ok(content) = std::fs::read(entry.path()) {
                                return Some(content);
                            }
                        }
                    }
                }
            }
            None
        })
        .await
        .unwrap_or(None)
    }

    /// Returns the file system path for a media item **without** reading the
    /// file content into memory.  This is the streaming-friendly alternative
    /// to [`get_media`]; the caller can open the returned path with
    /// `tokio::fs::File::open` and stream it via `ReaderStream`.
    ///
    /// Returns `None` if the media_id fails validation or no matching file is
    /// found on disk.
    pub async fn get_media_file_path(&self, _server_name: &str, media_id: &str) -> Option<std::path::PathBuf> {
        if Self::validate_media_id(media_id).is_err() {
            return None;
        }
        let file_name = self.find_media_file_name(media_id).await.ok().flatten()?;
        Some(self.media_path.join(file_name))
    }

    /// See [`download_media`].
    /// See [`download_media`].
    pub async fn download_media(&self, _server_name: &str, media_id: &str) -> Result<Vec<u8>, ApiError> {
        Self::validate_media_id(media_id)?;
        self.get_media(_server_name, media_id).await.ok_or(ApiError::not_found("Media not found".to_string()))
    }

    /// See [`get_thumbnail`].
    pub async fn get_thumbnail(
        &self,
        _server_name: &str,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<Vec<u8>, ApiError> {
        Self::validate_media_id(media_id)?;
        let thumbnail_method = ThumbnailMethod::from_str(method).map_err(ApiError::bad_request)?;
        let thumbnail_filename = format!("{media_id}_{width}x{height}_{method}.jpg");
        let thumbnail_path = self.thumbnail_path.join(&thumbnail_filename);

        if let Ok(content) = tokio::fs::read(&thumbnail_path).await {
            ::tracing::info!(
                media_id = %media_id,
                width,
                height,
                method = %method,
                thumbnail_filename = %thumbnail_filename,
                "Serving cached thumbnail"
            );
            return Ok(content);
        }

        let original_content = self.download_media(_server_name, media_id).await?;

        // 审查 #1：解码+缩放是重 CPU 操作，移入 spawn_blocking 避免阻塞 tokio worker。
        let content_for_thumb = original_content.clone();
        let thumbnail = match tokio::task::spawn_blocking(move || {
            Self::generate_thumbnail(&content_for_thumb, width, height, thumbnail_method)
        })
        .await
        {
            Ok(Ok(t)) => t,
            _ => return Ok(original_content),
        };

        if let Err(e) = tokio::fs::write(&thumbnail_path, &thumbnail).await {
            ::tracing::warn!(
                media_id = %media_id,
                width,
                height,
                method = %method,
                thumbnail_filename = %thumbnail_filename,
                error = %e,
                "Failed to cache thumbnail"
            );
        }

        Ok(thumbnail)
    }

    fn generate_thumbnail(
        image_data: &[u8],
        target_width: u32,
        target_height: u32,
        method: ThumbnailMethod,
    ) -> Result<Vec<u8>, ApiError> {
        use image::imageops::FilterType;
        use image::{ImageFormat, ImageReader, Limits};

        // 审查 #1：解压炸弹防护。用 ImageReader + Limits 在解码前限制单边最大
        // 像素，高压缩比图片（如超宽 PNG）在分配完整解码图前即被拒绝，避免
        // 内存耗尽。阈值对齐 Synapse max_image_pixels 的保守上界。
        const MAX_IMAGE_DIMENSION: u32 = 8192;

        // MEDIA-01 (P1): 缩略图输出尺寸上限。即使请求方传入 10000×10000，
        // 也限制到 2048×2048（人类视觉阈值 + 避免 CPU/内存/磁盘三重耗尽）。
        // 渲染后尺寸 = target_width * target_height 像素，2048² ≈ 4.2MP，
        // 既保留细节又把 JPEG 输出大小控制在 < 2MB。
        const MAX_THUMB_OUTPUT_DIMENSION: u32 = 2048;
        let target_width = target_width.min(MAX_THUMB_OUTPUT_DIMENSION);
        let target_height = target_height.min(MAX_THUMB_OUTPUT_DIMENSION);

        let mut reader = ImageReader::new(std::io::Cursor::new(image_data))
            .with_guessed_format()
            .map_err(|e| ApiError::bad_request(format!("Unsupported image format: {e}")))?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
        reader.limits(limits);

        let mut img = reader.decode().map_err(|e| ApiError::bad_request(format!("Invalid image data: {e}")))?;

        let thumbnail = match method {
            ThumbnailMethod::Crop => {
                let (orig_width, orig_height) = (img.width(), img.height());
                let aspect_ratio =
                    (orig_width as f32 / target_width as f32).max(orig_height as f32 / target_height as f32);

                let crop_width = (target_width as f32 * aspect_ratio) as u32;
                let crop_height = (target_height as f32 * aspect_ratio) as u32;

                let x = (orig_width.saturating_sub(crop_width)) / 2;
                let y = (orig_height.saturating_sub(crop_height)) / 2;

                let cropped = img.crop(x, y, crop_width.min(orig_width), crop_height.min(orig_height));
                cropped.resize_exact(target_width, target_height, FilterType::Lanczos3)
            }
            ThumbnailMethod::Scale => img.resize(target_width, target_height, FilterType::Lanczos3),
        };

        let mut output = Vec::new();
        thumbnail
            .write_to(&mut std::io::Cursor::new(&mut output), ImageFormat::Jpeg)
            .map_err(|e| ApiError::internal_with_context("Failed to encode thumbnail", &e))?;

        Ok(output)
    }

    /// See [`generate_all_thumbnails`].
    /// See [`generate_all_thumbnails`].
    pub async fn generate_all_thumbnails(&self, media_id: &str) -> Result<Vec<String>, ApiError> {
        Self::validate_media_id(media_id)?;
        let original_content = self.download_media("", media_id).await?;
        let mut generated = Vec::new();

        for config in &self.default_thumbnail_configs {
            // 审查 #1：重 CPU 解码/缩放移入 spawn_blocking，避免阻塞 tokio worker。
            // 先复制 Copy 字段，避免闭包捕获 &self 引用导致 'static 约束失败。
            let (cfg_width, cfg_height, cfg_method) = (config.width, config.height, config.method);
            let content_for_thumb = original_content.clone();
            let thumbnail = tokio::task::spawn_blocking(move || {
                Self::generate_thumbnail(&content_for_thumb, cfg_width, cfg_height, cfg_method)
            })
            .await
            .map_err(|e| ApiError::internal_with_context("Thumbnail generation task panicked", &e))??;

            let method_str = match config.method {
                ThumbnailMethod::Crop => "crop",
                ThumbnailMethod::Scale => "scale",
            };
            let thumbnail_filename = format!("{}_{}x{}_{}.jpg", media_id, config.width, config.height, method_str);
            let thumbnail_path = self.thumbnail_path.join(&thumbnail_filename);

            if let Err(e) = tokio::fs::write(&thumbnail_path, &thumbnail).await {
                ::tracing::warn!(
                    media_id = %media_id,
                    width = config.width,
                    height = config.height,
                    method = %method_str,
                    thumbnail_filename = %thumbnail_filename,
                    error = %e,
                    "Failed to write thumbnail"
                );
            } else {
                generated.push(thumbnail_filename);
            }
        }

        Ok(generated)
    }

    /// See [`get_thumbnail_configurations`].
    /// See [`get_thumbnail_configurations`].
    pub fn get_thumbnail_configurations(&self) -> Vec<ThumbnailSettings> {
        self.default_thumbnail_configs.clone()
    }

    /// See [`cleanup_old_thumbnails`].
    /// See [`cleanup_old_thumbnails`].
    pub async fn cleanup_old_thumbnails(&self, max_age_days: u64) -> Result<u64, ApiError> {
        let thumbnail_path = self.thumbnail_path.clone();
        let now = std::time::SystemTime::now();
        let max_age = std::time::Duration::from_secs(max_age_days * 24 * 60 * 60);
        let mut deleted_count = 0u64;

        let result = tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(&thumbnail_path) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(age) = now.duration_since(modified) {
                                if age > max_age {
                                    let file_name = entry.file_name().to_string_lossy().to_string();
                                    if let Err(e) = std::fs::remove_file(entry.path()) {
                                        ::tracing::warn!(
                                            error = %e,
                                            file_name = %file_name,
                                            max_age_days,
                                            "Failed to delete old thumbnail"
                                        );
                                    } else {
                                        deleted_count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            deleted_count
        })
        .await
        .map_err(|e| ApiError::internal_with_context("Task error", &e))?;

        Ok(result)
    }

    /// See [`get_media_metadata`].
    /// See [`get_media_metadata`].
    pub async fn get_media_metadata(&self, _server_name: &str, media_id: &str) -> Option<serde_json::Value> {
        if Self::validate_media_id(media_id).is_err() {
            return None;
        }

        if let Some(storage) = &self.admin_media_storage {
            if let Ok(Some(media)) = storage.get_media_info(media_id).await {
                return Some(serde_json::json!({
                    "media_id": media_id,
                    "content_type": media.content_type,
                    "filename": media.file_name,
                    "size": media.size,
                    "uploader_user_id": media.uploader_user_id
                }));
            }
        }

        let media_path = self.media_path.clone();
        let media_id = media_id.to_string();

        tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(media_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if media_file_matches_id(file_name, &media_id) {
                            let metadata = serde_json::json!({
                                "media_id": media_id,
                                "content_uri": format!("/_matrix/media/v3/download/{}", file_name),
                                "filename": file_name
                            });
                            return Some(metadata);
                        }
                    }
                }
            }
            None
        })
        .await
        .unwrap_or(None)
    }

    fn get_extension_from_content_type(content_type: &str) -> &str {
        match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "application/pdf" => "pdf",
            "text/plain" => "txt",
            _ => "bin",
        }
    }

    /// See [`preview_url`].
    /// See [`preview_url`].
    pub fn preview_url(&self, url: &str, _ts: i64) -> ApiResult<serde_json::Value> {
        Ok(serde_json::json!({
            "url": url,
            "title": "URL Preview",
            "description": "Preview for the requested URL",
            "og:title": "URL Preview",
            "og:description": "Open Graph description",
            "og:image": format!("{}/preview.png", url),
            "matrix:image:size": 1024,
            "og:image:width": 800,
            "og:image:height": 600
        }))
    }

    /// See [`get_media_info`].
    /// See [`get_media_info`].
    pub async fn get_media_info(&self, server_name: &str, media_id: &str) -> ApiResult<serde_json::Value> {
        Self::validate_media_id(media_id)?;
        let media_path = self.media_path.clone();
        let server_name = server_name.to_string();
        let media_id = media_id.to_string();

        let result = tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(&media_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if media_file_matches_id(file_name, &media_id) {
                            if let Ok(metadata) = entry.metadata() {
                                let parts: Vec<&str> = file_name.split('.').collect();
                                let uploader = if parts.len() >= 3 {
                                    parts[1].replace("_at_", "@").replace("_col_", ":").replace("_dot_", ".")
                                } else {
                                    String::new()
                                };
                                return Some(serde_json::json!({
                                    "media_id": media_id,
                                    "server_name": server_name,
                                    "content_uri": synapse_common::media_locator::MediaLocator {
                                        server_name: server_name.to_string(),
                                        media_id: media_id.to_string(),
                                    }.to_mxc_url(),
                                    "filename": file_name,
                                    "size": metadata.len(),
                                    "uploader": uploader,
                                    "created_at": metadata.created()
                                        .map(|t| t.duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_millis() as i64)
                                            .unwrap_or(0))
                                        .unwrap_or(0)
                                }));
                            }
                        }
                    }
                }
            }
            None
        })
        .await
        .map_err(|e| ApiError::internal_with_context("Task error", &e))?;

        result.ok_or(ApiError::not_found("Media not found".to_string()))
    }

    /// See [`delete_media`].
    /// See [`delete_media`].
    pub async fn delete_media(&self, server_name: &str, media_id: &str) -> ApiResult<()> {
        Self::validate_media_id(media_id)?;
        let media_path = self.media_path.clone();
        let media_id = media_id.to_string();
        let server_name = server_name.to_string();

        let result = tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(&media_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if media_file_matches_id(file_name, &media_id) {
                            let path = entry.path();
                            if let Err(e) = std::fs::remove_file(&path) {
                                return Err(format!("Failed to delete media file: {e}"));
                            }
                            ::tracing::info!(
                                media_id = %media_id,
                                file_name = %file_name,
                                server_name = %server_name,
                                "Deleted media"
                            );
                            return Ok(());
                        }
                    }
                }
            }
            Err("Media not found".to_string())
        })
        .await
        .map_err(|e| ApiError::internal_with_context("Task error", &e))?;

        result.map_err(ApiError::not_found)
    }

    /// See [`purge_media_cache`].
    /// See [`purge_media_cache`].
    pub async fn purge_media_cache(&self, before_ts: i64) -> Result<u64, ApiError> {
        let media_path = self.media_path.clone();
        let thumbnail_path = self.thumbnail_path.clone();
        let before_time = std::time::UNIX_EPOCH + std::time::Duration::from_millis(before_ts as u64);
        let mut deleted_count = 0u64;

        let media_deleted = tokio::task::spawn_blocking(move || {
            let mut count = 0u64;
            if let Ok(entries) = std::fs::read_dir(&media_path) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if modified < before_time {
                                let file_name = entry.file_name().to_string_lossy().to_string();
                                if let Err(e) = std::fs::remove_file(entry.path()) {
                                    ::tracing::warn!(
                                        error = %e,
                                        file_name = %file_name,
                                        before_ts,
                                        "Failed to delete cached media"
                                    );
                                } else {
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
            count
        })
        .await
        .map_err(|e| ApiError::internal_with_context("Task error", &e))?;

        deleted_count += media_deleted;

        let thumb_deleted = tokio::task::spawn_blocking(move || {
            let mut count = 0u64;
            if let Ok(entries) = std::fs::read_dir(&thumbnail_path) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if modified < before_time {
                                let file_name = entry.file_name().to_string_lossy().to_string();
                                if let Err(e) = std::fs::remove_file(entry.path()) {
                                    ::tracing::warn!(
                                        error = %e,
                                        file_name = %file_name,
                                        before_ts,
                                        "Failed to delete cached thumbnail"
                                    );
                                } else {
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
            count
        })
        .await
        .map_err(|e| ApiError::internal_with_context("Task error", &e))?;

        deleted_count += thumb_deleted;
        ::tracing::info!(deleted_count, before_ts, "Purged media cache");
        Ok(deleted_count)
    }
}

fn media_file_matches_id(file_name: &str, media_id: &str) -> bool {
    file_name.strip_prefix(media_id).is_some_and(|rest| rest.starts_with('.') || rest.starts_with('_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_method_from_str() {
        assert_eq!(ThumbnailMethod::from_str("crop").unwrap(), ThumbnailMethod::Crop);
        assert_eq!(ThumbnailMethod::from_str("CROP").unwrap(), ThumbnailMethod::Crop);
        assert_eq!(ThumbnailMethod::from_str("scale").unwrap(), ThumbnailMethod::Scale);
        assert_eq!(ThumbnailMethod::from_str("SCALE").unwrap(), ThumbnailMethod::Scale);
        assert!(ThumbnailMethod::from_str("invalid").is_err());
    }

    #[test]
    fn test_thumbnail_config_default() {
        let config = ThumbnailSettings::default();
        assert_eq!(config.width, 800);
        assert_eq!(config.height, 600);
        assert_eq!(config.method, ThumbnailMethod::Scale);
        assert_eq!(config.quality, 80);
    }

    #[test]
    fn test_thumbnail_method_equality() {
        assert_eq!(ThumbnailMethod::Crop, ThumbnailMethod::Crop);
        assert_eq!(ThumbnailMethod::Scale, ThumbnailMethod::Scale);
        assert_ne!(ThumbnailMethod::Crop, ThumbnailMethod::Scale);
    }

    #[test]
    fn test_media_service_creation() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();

        let service = MediaService::new(media_path, None, "test.server");

        assert!(service.media_path.exists());
        assert!(service.thumbnail_path.exists());
        assert!(service.task_queue.is_none());
        assert_eq!(service.default_thumbnail_configs.len(), 5);
    }

    #[test]
    fn test_media_service_task_queue_field() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();

        let service = MediaService::new(media_path, None, "test.server");
        assert!(service.task_queue.is_none());
    }

    #[test]
    fn test_get_extension_from_content_type() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let _service = MediaService::new(media_path, None, "test.server");

        assert_eq!(MediaService::get_extension_from_content_type("image/jpeg"), "jpg");
        assert_eq!(MediaService::get_extension_from_content_type("image/png"), "png");
        assert_eq!(MediaService::get_extension_from_content_type("image/gif"), "gif");
        assert_eq!(MediaService::get_extension_from_content_type("application/pdf"), "pdf");
        assert_eq!(MediaService::get_extension_from_content_type("text/plain"), "txt");
        assert_eq!(MediaService::get_extension_from_content_type("unknown/type"), "bin");
        assert_eq!(MediaService::get_extension_from_content_type(""), "bin");
    }

    #[test]
    fn test_thumbnail_method_error_message() {
        let result = ThumbnailMethod::from_str("invalid_method");
        assert!(result.is_err());

        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Invalid thumbnail method"));
        assert!(error_msg.contains("invalid_method"));
    }

    #[test]
    fn test_thumbnail_config_custom() {
        let config = ThumbnailSettings { width: 1024, height: 768, method: ThumbnailMethod::Crop, quality: 90 };

        assert_eq!(config.width, 1024);
        assert_eq!(config.height, 768);
        assert_eq!(config.method, ThumbnailMethod::Crop);
        assert_eq!(config.quality, 90);
    }

    #[test]
    fn test_media_service_default_thumbnail_configs() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let configs = &service.default_thumbnail_configs;

        assert_eq!(configs.len(), 5);

        assert_eq!(configs[0].width, 32);
        assert_eq!(configs[0].height, 32);
        assert_eq!(configs[0].method, ThumbnailMethod::Crop);

        assert_eq!(configs[4].width, 800);
        assert_eq!(configs[4].height, 600);
        assert_eq!(configs[4].method, ThumbnailMethod::Scale);
    }

    #[test]
    fn test_thumbnail_method_case_insensitive() {
        assert!(ThumbnailMethod::from_str("crop").is_ok());
        assert!(ThumbnailMethod::from_str("CROP").is_ok());
        assert!(ThumbnailMethod::from_str("Crop").is_ok());
        assert!(ThumbnailMethod::from_str("CrOp").is_ok());

        assert!(ThumbnailMethod::from_str("scale").is_ok());
        assert!(ThumbnailMethod::from_str("SCALE").is_ok());
        assert!(ThumbnailMethod::from_str("Scale").is_ok());
        assert!(ThumbnailMethod::from_str("ScAlE").is_ok());
    }

    #[test]
    fn test_thumbnail_config_quality_range() {
        let valid_qualities: Vec<u8> = vec![1, 50, 80, 100];
        for quality in valid_qualities {
            let config = ThumbnailSettings { width: 100, height: 100, method: ThumbnailMethod::Scale, quality };
            assert!(config.quality > 0);
        }
    }

    #[test]
    fn test_thumbnail_config_dimension_boundaries() {
        let config = ThumbnailSettings { width: 1, height: 1, method: ThumbnailMethod::Scale, quality: 80 };
        assert_eq!(config.width, 1);
        assert_eq!(config.height, 1);

        let config_large =
            ThumbnailSettings { width: 10000, height: 10000, method: ThumbnailMethod::Crop, quality: 80 };
        assert_eq!(config_large.width, 10000);
        assert_eq!(config_large.height, 10000);
    }

    #[tokio::test]
    async fn test_get_thumbnail_configurations() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let configs = service.get_thumbnail_configurations();
        assert_eq!(configs.len(), 5);
    }

    #[tokio::test]
    async fn test_async_content_type_validation() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let _service = MediaService::new(media_path, None, "test.server");

        let test_cases = vec![
            ("image/jpeg", "jpg"),
            ("image/png", "png"),
            ("image/gif", "gif"),
            ("application/pdf", "pdf"),
            ("text/plain", "txt"),
            ("", "bin"),
            ("unknown/type", "bin"),
        ];

        for (content_type, expected_ext) in test_cases {
            let ext = MediaService::get_extension_from_content_type(content_type);
            assert_eq!(ext, expected_ext, "Failed for content type: {content_type}");
        }
    }

    #[tokio::test]
    async fn test_async_upload_different_types() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let content = b"test content";

        let result_jpeg = service.upload_media("@user:example.com", content, "image/jpeg", None).await;
        assert!(result_jpeg.is_ok());

        let result_png = service.upload_media("@user:example.com", content, "image/png", None).await;
        assert!(result_png.is_ok());

        let result_pdf = service.upload_media("@user:example.com", content, "application/pdf", None).await;
        assert!(result_pdf.is_ok());
    }

    #[tokio::test]
    async fn test_async_cleanup_empty_directory() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let result = service.cleanup_old_thumbnails(30).await;
        assert!(result.is_ok());
        let deleted = result.unwrap();
        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn test_async_preview_url_metadata() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let media_path = temp_dir.path().to_str().unwrap();
        let service = MediaService::new(media_path, None, "test.server");

        let url = "https://example.com/test";
        let ts = 1234567890i64;

        let result = service.preview_url(url, ts);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["url"], url);
    }

    // 审查 #1：解压炸弹防护——图片单边超过 MAX_IMAGE_DIMENSION 应被拒绝，
    // 而非全量解码撑爆内存。
    #[test]
    fn test_generate_thumbnail_rejects_oversized_image() {
        let img = image::RgbImage::from_pixel(8193, 1, image::Rgb([255u8, 0, 0]));
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();

        let result = MediaService::generate_thumbnail(&buf, 100, 100, ThumbnailMethod::Scale);
        assert!(result.is_err(), "oversized image must be rejected");
    }

    // MEDIA-01: 即使请求方传入超大目标尺寸，generate_thumbnail 也必须将其
    // 收敛到 MAX_THUMB_OUTPUT_DIMENSION (2048)。否则请求 ?width=10000&height=10000
    // 会触发 CPU/内存/磁盘三重耗尽 DoS。
    #[test]
    fn test_generate_thumbnail_caps_output_dimensions() {
        let img = image::RgbImage::from_pixel(500, 500, image::Rgb([128u8, 128, 128]));
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();

        // 请求 9999×9999 → 输出应被静默收敛到 2048×2048 而非 panic / 失败。
        let result =
            MediaService::generate_thumbnail(&buf, 9999, 9999, ThumbnailMethod::Scale).expect("缩略图生成应成功");
        assert!(!result.is_empty(), "应返回非空 JPEG 字节流");

        // 用 image crate 解码结果验证尺寸确实 ≤ 2048。
        let decoded =
            image::load_from_memory_with_format(&result, image::ImageFormat::Jpeg).expect("解码刚生成的缩略图");
        assert!(decoded.width() <= 2048, "输出宽度应 ≤ 2048，实际 {}", decoded.width());
        assert!(decoded.height() <= 2048, "输出高度应 ≤ 2048，实际 {}", decoded.height());
    }

    #[test]
    fn test_generate_thumbnail_handles_normal_image() {
        let img = image::RgbImage::from_pixel(100, 100, image::Rgb([0u8, 255, 0]));
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();

        let result = MediaService::generate_thumbnail(&buf, 32, 32, ThumbnailMethod::Scale);
        assert!(result.is_ok(), "normal image must succeed: {result:?}");
    }
}
