use crate::common::*;
use crate::services::*;
use std::path::PathBuf;

#[derive(Clone)]
pub struct MediaService {
    media_path: PathBuf,
}

impl MediaService {
    pub fn new(media_path: &str) -> Self {
        let path = PathBuf::from(media_path);
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
        Self { media_path: path }
    }

    pub async fn upload_media(
        &self,
        _user_id: &str,
        content: &[u8],
        content_type: &str,
        _filename: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let media_id = random_string(32);
        let extension = self.get_extension_from_content_type(content_type);
        let file_name = format!("{}.{}", media_id, extension);
        let file_path = self.media_path.join(&file_name);

        let content_vec = content.to_vec();
        tokio::task::spawn_blocking(move || std::fs::write(&file_path, content_vec))
            .await
            .map_err(|e| ApiError::internal(format!("Write task panicked: {}", e)))?
            .map_err(|e| ApiError::internal(format!("Failed to save media: {}", e)))?;

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
        _width: u32,
        _height: u32,
        _method: &str,
    ) -> Result<Vec<u8>, ApiError> {
        self.download_media(_server_name, media_id).await
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
}
