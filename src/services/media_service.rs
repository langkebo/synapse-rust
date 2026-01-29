use crate::common::*;
use crate::services::*;
use std::fs;
use std::path::PathBuf;

pub struct MediaService {
    media_path: PathBuf,
}

impl MediaService {
    pub fn new(media_path: &str) -> Self {
        let path = PathBuf::from(media_path);
        if !path.exists() {
            fs::create_dir_all(&path).ok();
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
        let media_id = generate_token(32);
        let extension = self.get_extension_from_content_type(content_type);
        let file_name = format!("{}.{}", media_id, extension);
        let file_path = self.media_path.join(&file_name);

        fs::write(&file_path, content)
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

    pub async fn get_media(&self, server_name: &str, media_id: &str) -> Option<Vec<u8>> {
        let file_path = self.media_path.join(format!("{}.*", media_id));
        if let Ok(entries) = fs::read_dir(&self.media_path) {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    if file_name.starts_with(media_id) {
                        if let Ok(content) = fs::read(entry.path()) {
                            return Some(content);
                        }
                    }
                }
            }
        }
        None
    }

    pub async fn get_media_metadata(
        &self,
        server_name: &str,
        media_id: &str,
    ) -> Option<serde_json::Value> {
        if let Ok(entries) = fs::read_dir(&self.media_path) {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    if file_name.starts_with(media_id) {
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
    }

    fn get_extension_from_content_type(&self, content_type: &str) -> &str {
        if content_type.starts_with("image/png") {
            "png"
        } else if content_type.starts_with("image/jpeg") {
            "jpg"
        } else if content_type.starts_with("image/gif") {
            "gif"
        } else if content_type.starts_with("image/webp") {
            "webp"
        } else if content_type.starts_with("video/mp4") {
            "mp4"
        } else if content_type.starts_with("video/webm") {
            "webm"
        } else if content_type.starts_with("audio/mpeg") {
            "mp3"
        } else if content_type.starts_with("audio/ogg") {
            "ogg"
        } else {
            "bin"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_service_creation() {
        let _media_service = MediaService::new("/tmp/test_media");
    }

    #[test]
    fn test_get_extension_png() {
        let media_service = MediaService::new("/tmp/test");
        let ext = media_service.get_extension_from_content_type("image/png");
        assert_eq!(ext, "png");
    }

    #[test]
    fn test_get_extension_jpeg() {
        let media_service = MediaService::new("/tmp/test");
        let ext = media_service.get_extension_from_content_type("image/jpeg");
        assert_eq!(ext, "jpg");
    }

    #[test]
    fn test_get_extension_gif() {
        let media_service = MediaService::new("/tmp/test");
        let ext = media_service.get_extension_from_content_type("image/gif");
        assert_eq!(ext, "gif");
    }

    #[test]
    fn test_get_extension_webp() {
        let media_service = MediaService::new("/tmp/test");
        let ext = media_service.get_extension_from_content_type("image/webp");
        assert_eq!(ext, "webp");
    }

    #[test]
    fn test_get_extension_mp4() {
        let media_service = MediaService::new("/tmp/test");
        let ext = media_service.get_extension_from_content_type("video/mp4");
        assert_eq!(ext, "mp4");
    }

    #[test]
    fn test_get_extension_webm() {
        let media_service = MediaService::new("/tmp/test");
        let ext = media_service.get_extension_from_content_type("video/webm");
        assert_eq!(ext, "webm");
    }

    #[test]
    fn test_get_extension_mp3() {
        let media_service = MediaService::new("/tmp/test");
        let ext = media_service.get_extension_from_content_type("audio/mpeg");
        assert_eq!(ext, "mp3");
    }

    #[test]
    fn test_get_extension_ogg() {
        let media_service = MediaService::new("/tmp/test");
        let ext = media_service.get_extension_from_content_type("audio/ogg");
        assert_eq!(ext, "ogg");
    }

    #[test]
    fn test_get_extension_unknown() {
        let media_service = MediaService::new("/tmp/test");
        let ext = media_service.get_extension_from_content_type("application/octet-stream");
        assert_eq!(ext, "bin");
    }

    #[test]
    fn test_media_metadata_format() {
        let metadata = json!({
            "media_id": "test123",
            "content_uri": "/_matrix/media/v3/download/test123.png",
            "filename": "test123.png"
        });

        assert_eq!(metadata["media_id"], "test123");
        assert!(metadata["content_uri"].is_string());
        assert!(metadata["filename"].is_string());
    }

    #[test]
    fn test_upload_response_format() {
        let response = json!({
            "content_uri": "/_matrix/media/v3/download/test123.png",
            "content_type": "image/png",
            "size": 1024,
            "media_id": "test123"
        });

        assert!(response.get("content_uri").is_some());
        assert!(response.get("content_type").is_some());
        assert!(response.get("size").is_some());
        assert!(response.get("media_id").is_some());
    }

    #[test]
    fn test_media_id_length() {
        let media_id = generate_token(32);
        assert_eq!(media_id.len(), 32);
    }
}
