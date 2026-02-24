use crate::common::background_job::BackgroundJob;
use crate::common::task_queue::RedisTaskQueue;
use crate::common::*;
use crate::services::*;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThumbnailMethod {
    Crop,
    Scale,
}

impl FromStr for ThumbnailMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "crop" => Ok(ThumbnailMethod::Crop),
            "scale" => Ok(ThumbnailMethod::Scale),
            _ => Err(format!("Invalid thumbnail method: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThumbnailConfig {
    pub width: u32,
    pub height: u32,
    pub method: ThumbnailMethod,
    pub quality: u8,
}

impl Default for ThumbnailConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            method: ThumbnailMethod::Scale,
            quality: 80,
        }
    }
}

#[derive(Clone)]
pub struct MediaService {
    media_path: PathBuf,
    thumbnail_path: PathBuf,
    task_queue: Option<Arc<RedisTaskQueue>>,
    default_thumbnail_configs: Vec<ThumbnailConfig>,
}

impl MediaService {
    pub fn new(media_path: &str, task_queue: Option<Arc<RedisTaskQueue>>) -> Self {
        let path = PathBuf::from(media_path);
        let thumbnail_path = path.join("thumbnails");

        ::tracing::info!("MediaService::new called with path: {}", media_path);
        ::tracing::info!("Media path exists: {}", path.exists());

        if !path.exists() {
            ::tracing::info!("Attempting to create media directory: {}", path.display());
            if let Err(e) = std::fs::create_dir_all(&path) {
                ::tracing::error!("Failed to create media directory {}: {}", path.display(), e);
            } else {
                ::tracing::info!("Created media directory: {}", path.display());
            }
        }

        if !thumbnail_path.exists() {
            if let Err(e) = std::fs::create_dir_all(&thumbnail_path) {
                ::tracing::error!(
                    "Failed to create thumbnail directory {}: {}",
                    thumbnail_path.display(),
                    e
                );
            }
        }

        let default_thumbnail_configs = vec![
            ThumbnailConfig {
                width: 32,
                height: 32,
                method: ThumbnailMethod::Crop,
                quality: 70,
            },
            ThumbnailConfig {
                width: 96,
                height: 96,
                method: ThumbnailMethod::Crop,
                quality: 70,
            },
            ThumbnailConfig {
                width: 320,
                height: 240,
                method: ThumbnailMethod::Scale,
                quality: 80,
            },
            ThumbnailConfig {
                width: 640,
                height: 480,
                method: ThumbnailMethod::Scale,
                quality: 80,
            },
            ThumbnailConfig {
                width: 800,
                height: 600,
                method: ThumbnailMethod::Scale,
                quality: 80,
            },
        ];

        Self {
            media_path: path,
            thumbnail_path,
            task_queue,
            default_thumbnail_configs,
        }
    }

    pub async fn upload_media(
        &self,
        user_id: &str,
        content: &[u8],
        content_type: &str,
        _filename: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let media_id = random_string(32);
        let extension = self.get_extension_from_content_type(content_type);
        let user_id_encoded = user_id
            .replace('@', "_at_")
            .replace(':', "_col_")
            .replace('.', "_dot_");
        let file_name = format!("{}.{}.{}", media_id, user_id_encoded, extension);
        let file_path = self.media_path.join(&file_name);
        let media_path_display = self.media_path.display().to_string();

        ::tracing::info!(
            "Uploading media: {} bytes to {}",
            content.len(),
            file_path.display()
        );

        if !self.media_path.exists() {
            ::tracing::warn!(
                "Media path does not exist, attempting to create: {}",
                self.media_path.display()
            );
            if let Err(e) = std::fs::create_dir_all(&self.media_path) {
                ::tracing::error!("Failed to create media directory: {}", e);
                return Err(ApiError::internal(format!(
                    "Media storage not available: {}. Please ensure the media directory exists and has correct permissions.",
                    e
                )));
            }
        }

        let content_vec = content.to_vec();
        let write_result =
            tokio::task::spawn_blocking(move || std::fs::write(&file_path, content_vec))
                .await
                .map_err(|e| ApiError::internal(format!("Write task panicked: {}", e)))?;

        if let Err(e) = write_result {
            ::tracing::error!("Failed to save media file - Error: {}", e);

            let error_msg = match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    format!(
                        "Permission denied writing to media directory. Please run: chmod 755 {} && chown -R synapse:synapse {}",
                        media_path_display,
                        media_path_display
                    )
                }
                std::io::ErrorKind::NotFound => {
                    format!("Media directory not found: {}", media_path_display)
                }
                std::io::ErrorKind::StorageFull => {
                    "Storage full. Please free up disk space.".to_string()
                }
                _ => format!("Failed to save media: {}", e),
            };

            return Err(ApiError::internal(error_msg));
        }

        ::tracing::info!("Successfully saved media file: {}", file_name);

        if let Some(queue) = &self.task_queue {
            let job = BackgroundJob::ProcessMedia {
                file_id: file_name.clone(),
            };
            if let Err(e) = queue.submit(job).await {
                ::tracing::warn!(
                    "Failed to submit media processing task for {}: {}",
                    file_name,
                    e
                );
            } else {
                ::tracing::info!("Submitted media processing task for {}", file_name);
            }
        }

        let media_url = format!("/_matrix/media/v3/download/{}", file_name);

        let json_metadata = serde_json::json!({
            "content_uri": media_url,
            "content_type": content_type,
            "size": content.len(),
            "media_id": media_id
        });

        Ok(json_metadata)
    }

    pub async fn get_media(&self, _server_name: &str, media_id: &str) -> Option<Vec<u8>> {
        let media_path = self.media_path.clone();
        let media_id = media_id.to_string();

        tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(media_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.starts_with(&media_id) {
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

    pub async fn download_media(
        &self,
        _server_name: &str,
        media_id: &str,
    ) -> Result<Vec<u8>, ApiError> {
        self.get_media(_server_name, media_id)
            .await
            .ok_or(ApiError::not_found("Media not found".to_string()))
    }

    pub async fn get_thumbnail(
        &self,
        _server_name: &str,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<Vec<u8>, ApiError> {
        let thumbnail_method = ThumbnailMethod::from_str(method).map_err(ApiError::bad_request)?;
        let thumbnail_filename = format!("{}_{}x{}_{}.jpg", media_id, width, height, method);
        let thumbnail_path = self.thumbnail_path.join(&thumbnail_filename);

        if let Ok(content) = tokio::fs::read(&thumbnail_path).await {
            ::tracing::info!("Serving cached thumbnail: {}", thumbnail_filename);
            return Ok(content);
        }

        let original_content = self.download_media(_server_name, media_id).await?;

        let thumbnail =
            self.generate_thumbnail(&original_content, width, height, thumbnail_method)?;

        if let Err(e) = tokio::fs::write(&thumbnail_path, &thumbnail).await {
            ::tracing::warn!("Failed to cache thumbnail {}: {}", thumbnail_filename, e);
        }

        Ok(thumbnail)
    }

    fn generate_thumbnail(
        &self,
        image_data: &[u8],
        target_width: u32,
        target_height: u32,
        method: ThumbnailMethod,
    ) -> Result<Vec<u8>, ApiError> {
        use image::imageops::FilterType;
        use image::ImageFormat;

        let mut img = image::load_from_memory(image_data)
            .map_err(|e| ApiError::bad_request(format!("Invalid image data: {}", e)))?;

        let thumbnail = match method {
            ThumbnailMethod::Crop => {
                let (orig_width, orig_height) = (img.width(), img.height());
                let aspect_ratio = (orig_width as f32 / target_width as f32)
                    .max(orig_height as f32 / target_height as f32);

                let crop_width = (target_width as f32 * aspect_ratio) as u32;
                let crop_height = (target_height as f32 * aspect_ratio) as u32;

                let x = (orig_width.saturating_sub(crop_width)) / 2;
                let y = (orig_height.saturating_sub(crop_height)) / 2;

                let cropped = img.crop(
                    x,
                    y,
                    crop_width.min(orig_width),
                    crop_height.min(orig_height),
                );
                cropped.resize_exact(target_width, target_height, FilterType::Lanczos3)
            }
            ThumbnailMethod::Scale => img.resize(target_width, target_height, FilterType::Lanczos3),
        };

        let mut output = Vec::new();
        thumbnail
            .write_to(&mut std::io::Cursor::new(&mut output), ImageFormat::Jpeg)
            .map_err(|e| ApiError::internal(format!("Failed to encode thumbnail: {}", e)))?;

        Ok(output)
    }

    pub async fn generate_all_thumbnails(&self, media_id: &str) -> Result<Vec<String>, ApiError> {
        let original_content = self.download_media("", media_id).await?;
        let mut generated = Vec::new();

        for config in &self.default_thumbnail_configs {
            let thumbnail = self.generate_thumbnail(
                &original_content,
                config.width,
                config.height,
                config.method,
            )?;

            let method_str = match config.method {
                ThumbnailMethod::Crop => "crop",
                ThumbnailMethod::Scale => "scale",
            };
            let thumbnail_filename = format!(
                "{}_{}x{}_{}.jpg",
                media_id, config.width, config.height, method_str
            );
            let thumbnail_path = self.thumbnail_path.join(&thumbnail_filename);

            if let Err(e) = tokio::fs::write(&thumbnail_path, &thumbnail).await {
                ::tracing::warn!("Failed to write thumbnail {}: {}", thumbnail_filename, e);
            } else {
                generated.push(thumbnail_filename);
            }
        }

        Ok(generated)
    }

    pub async fn get_thumbnail_configurations(&self) -> Vec<ThumbnailConfig> {
        self.default_thumbnail_configs.clone()
    }

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
                                    if let Err(e) = std::fs::remove_file(entry.path()) {
                                        ::tracing::warn!("Failed to delete old thumbnail: {}", e);
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
        .map_err(|e| ApiError::internal(format!("Task error: {}", e)))?;

        Ok(result)
    }

    pub async fn get_media_metadata(
        &self,
        _server_name: &str,
        media_id: &str,
    ) -> Option<serde_json::Value> {
        let media_path = self.media_path.clone();
        let media_id = media_id.to_string();

        tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(media_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.starts_with(&media_id) {
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

    fn get_extension_from_content_type(&self, content_type: &str) -> &str {
        match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "application/pdf" => "pdf",
            "text/plain" => "txt",
            _ => "bin",
        }
    }

    pub async fn preview_url(&self, url: &str, _ts: i64) -> ApiResult<serde_json::Value> {
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

    pub async fn get_media_info(
        &self,
        server_name: &str,
        media_id: &str,
    ) -> ApiResult<serde_json::Value> {
        let media_path = self.media_path.clone();
        let server_name = server_name.to_string();
        let media_id = media_id.to_string();

        let result = tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(&media_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.starts_with(&media_id) {
                            if let Ok(metadata) = entry.metadata() {
                                let parts: Vec<&str> = file_name.split('.').collect();
                                let uploader = if parts.len() >= 3 {
                                    parts[1]
                                        .replace("_at_", "@")
                                        .replace("_col_", ":")
                                        .replace("_dot_", ".")
                                } else {
                                    String::new()
                                };
                                return Some(serde_json::json!({
                                    "media_id": media_id,
                                    "server_name": server_name,
                                    "content_uri": format!("mxc://{}/{}", server_name, media_id),
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
        .map_err(|e| ApiError::internal(format!("Task error: {}", e)))?;

        result.ok_or(ApiError::not_found("Media not found".to_string()))
    }

    pub async fn delete_media(&self, server_name: &str, media_id: &str) -> ApiResult<()> {
        let media_path = self.media_path.clone();
        let media_id = media_id.to_string();
        let server_name = server_name.to_string();

        let result = tokio::task::spawn_blocking(move || {
            if let Ok(entries) = std::fs::read_dir(&media_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.starts_with(&media_id) {
                            let path = entry.path();
                            if let Err(e) = std::fs::remove_file(&path) {
                                return Err(format!("Failed to delete media file: {}", e));
                            }
                            ::tracing::info!(
                                "Deleted media: {} from server {}",
                                file_name,
                                server_name
                            );
                            return Ok(());
                        }
                    }
                }
            }
            Err("Media not found".to_string())
        })
        .await
        .map_err(|e| ApiError::internal(format!("Task error: {}", e)))?;

        result.map_err(ApiError::not_found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_method_from_str() {
        assert_eq!(
            ThumbnailMethod::from_str("crop").unwrap(),
            ThumbnailMethod::Crop
        );
        assert_eq!(
            ThumbnailMethod::from_str("CROP").unwrap(),
            ThumbnailMethod::Crop
        );
        assert_eq!(
            ThumbnailMethod::from_str("scale").unwrap(),
            ThumbnailMethod::Scale
        );
        assert_eq!(
            ThumbnailMethod::from_str("SCALE").unwrap(),
            ThumbnailMethod::Scale
        );
        assert!(ThumbnailMethod::from_str("invalid").is_err());
    }

    #[test]
    fn test_thumbnail_config_default() {
        let config = ThumbnailConfig::default();
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
}
