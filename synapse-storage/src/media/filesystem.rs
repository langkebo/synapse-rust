use super::backend::MediaStorageBackend;
use super::models::*;
use async_trait::async_trait;
use std::path::PathBuf;
use synapse_common::error::ApiError;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

pub struct FilesystemBackend {
    base_path: PathBuf,
    thumbnail_path: PathBuf,
    max_path_depth: u32,
}

impl FilesystemBackend {
    pub fn new(config: &FilesystemConfig) -> Result<Self, ApiError> {
        let base_path = PathBuf::from(&config.storage_path);
        let thumbnail_path = base_path.join("thumbnails");

        if config.create_directories {
            std::fs::create_dir_all(&base_path)
                .map_err(|e| ApiError::internal_with_log("Failed to create media directory", &e))?;
            std::fs::create_dir_all(&thumbnail_path)
                .map_err(|e| ApiError::internal_with_log("Failed to create thumbnail directory", &e))?;
        }

        Ok(Self { base_path, thumbnail_path, max_path_depth: config.max_path_depth })
    }

    fn get_media_path(&self, media_id: &str) -> PathBuf {
        if media_id.contains("..") || media_id.contains('/') || media_id.contains('\\') {
            tracing::warn!("Rejected media_id with path traversal characters: {}", &media_id[..media_id.len().min(32)]);
            return self.base_path.clone();
        }
        let mut path = self.base_path.clone();

        if self.max_path_depth > 0 {
            let chars: Vec<char> = media_id.chars().collect();
            for i in 0..self.max_path_depth.min(chars.len() as u32) {
                path.push(chars[i as usize].to_string());
            }
        }

        path.push(media_id);
        path
    }

    fn get_thumbnail_path(&self, media_id: &str, width: u32, height: u32, method: &str) -> PathBuf {
        if media_id.contains("..") || media_id.contains('/') || media_id.contains('\\') {
            tracing::warn!("Rejected media_id with path traversal characters: {}", &media_id[..media_id.len().min(32)]);
            return self.thumbnail_path.clone();
        }
        let filename = format!("{media_id}_{width}x{height}_{method}.jpg");
        self.thumbnail_path.join(filename)
    }
}

#[async_trait]
impl MediaStorageBackend for FilesystemBackend {
    async fn store(&self, media_id: &str, data: &[u8], _content_type: &str) -> Result<(), ApiError> {
        let path = self.get_media_path(media_id);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to create directory", &e))?;
        }

        let mut file =
            fs::File::create(&path).await.map_err(|e| ApiError::internal_with_log("Failed to create file", &e))?;

        if let Err(e) = file.write_all(data).await {
            // Clean up the partially written file to prevent corrupted media
            // from being served on subsequent retrievals.
            let _ = fs::remove_file(&path).await;
            return Err(ApiError::internal_with_log("Failed to write file", &e));
        }

        Ok(())
    }

    async fn retrieve(&self, media_id: &str) -> Result<Option<Vec<u8>>, ApiError> {
        let path = self.get_media_path(media_id);

        if !path.exists() {
            return Ok(None);
        }

        let mut file = match fs::File::open(&path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ApiError::internal_with_log("Failed to open file", &e)),
        };

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await.map_err(|e| ApiError::internal_with_log("Failed to read file", &e))?;

        Ok(Some(buffer))
    }

    async fn delete(&self, media_id: &str) -> Result<bool, ApiError> {
        let path = self.get_media_path(media_id);

        if !path.exists() {
            return Ok(false);
        }

        fs::remove_file(&path).await.map_err(|e| ApiError::internal_with_log("Failed to delete file", &e))?;

        Ok(true)
    }

    async fn exists(&self, media_id: &str) -> Result<bool, ApiError> {
        let path = self.get_media_path(media_id);
        Ok(path.exists())
    }

    async fn get_size(&self, media_id: &str) -> Result<Option<u64>, ApiError> {
        let path = self.get_media_path(media_id);

        if !path.exists() {
            return Ok(None);
        }

        let metadata =
            fs::metadata(&path).await.map_err(|e| ApiError::internal_with_log("Failed to get file metadata", &e))?;

        Ok(Some(metadata.len()))
    }

    async fn store_thumbnail(
        &self,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
        data: &[u8],
    ) -> Result<(), ApiError> {
        let path = self.get_thumbnail_path(media_id, width, height, method);

        fs::create_dir_all(&self.thumbnail_path)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create thumbnail directory", &e))?;

        let mut file = fs::File::create(&path)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create thumbnail file", &e))?;

        if let Err(e) = file.write_all(data).await {
            let _ = fs::remove_file(&path).await;
            return Err(ApiError::internal_with_log("Failed to write thumbnail", &e));
        }

        Ok(())
    }

    async fn retrieve_thumbnail(
        &self,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<Option<Vec<u8>>, ApiError> {
        let path = self.get_thumbnail_path(media_id, width, height, method);

        if !path.exists() {
            return Ok(None);
        }

        let mut file = match fs::File::open(&path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ApiError::internal_with_log("Failed to open thumbnail", &e)),
        };

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await.map_err(|e| ApiError::internal_with_log("Failed to read thumbnail", &e))?;

        Ok(Some(buffer))
    }

    async fn delete_thumbnails(&self, media_id: &str) -> Result<u64, ApiError> {
        let mut count = 0u64;

        let mut entries = match fs::read_dir(&self.thumbnail_path).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(ApiError::internal_with_log("Failed to read thumbnail directory", &e)),
        };

        while let Some(entry) =
            entries.next_entry().await.map_err(|e| ApiError::internal_with_log("Failed to read directory entry", &e))?
        {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            if name.starts_with(media_id) && fs::remove_file(entry.path()).await.is_ok() {
                count += 1;
            }
        }

        Ok(count)
    }

    async fn get_stats(&self) -> Result<MediaStorageStats, ApiError> {
        let mut total_files = 0u64;
        let mut total_size = 0u64;
        let mut oldest_file: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut newest_file: Option<chrono::DateTime<chrono::Utc>> = None;

        fn process_directory(
            path: &PathBuf,
            total_files: &mut u64,
            total_size: &mut u64,
            oldest: &mut Option<chrono::DateTime<chrono::Utc>>,
            newest: &mut Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<(), ApiError> {
            let entries =
                std::fs::read_dir(path).map_err(|e| ApiError::internal_with_log("Failed to read directory", &e))?;

            for entry in entries {
                let entry = entry.map_err(|e| ApiError::internal_with_log("Failed to read entry", &e))?;

                let path = entry.path();
                if path.is_dir() {
                    process_directory(&path, total_files, total_size, oldest, newest)?;
                } else {
                    let metadata = std::fs::metadata(&path)
                        .map_err(|e| ApiError::internal_with_log("Failed to get metadata", &e))?;

                    *total_files += 1;
                    *total_size += metadata.len();

                    if let Ok(modified) = metadata.modified() {
                        let datetime: chrono::DateTime<chrono::Utc> = modified.into();

                        if oldest.as_ref().is_none_or(|&old| datetime < old) {
                            *oldest = Some(datetime);
                        }
                        if newest.as_ref().is_none_or(|&new| datetime > new) {
                            *newest = Some(datetime);
                        }
                    }
                }
            }

            Ok(())
        }

        process_directory(&self.base_path, &mut total_files, &mut total_size, &mut oldest_file, &mut newest_file)?;

        Ok(MediaStorageStats {
            total_files,
            total_size,
            by_content_type: std::collections::HashMap::new(),
            oldest_file,
            newest_file,
        })
    }

    async fn health_check(&self) -> Result<bool, ApiError> {
        Ok(self.base_path.exists() && self.base_path.is_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend(path: &str, max_depth: u32) -> FilesystemBackend {
        let config = FilesystemConfig {
            storage_path: path.to_string(),
            create_directories: false,
            max_path_depth: max_depth,
        };
        FilesystemBackend::new(&config).expect("backend construction should succeed")
    }

    #[test]
    fn test_get_media_path_normal_id() {
        let backend = make_backend("/tmp/media_test", 0);
        let path = backend.get_media_path("abc123");
        assert!(path.starts_with("/tmp/media_test"));
        assert!(path.ends_with("abc123"));
    }

    #[test]
    fn test_get_media_path_rejects_dotdot_traversal() {
        let backend = make_backend("/tmp/media_test", 0);
        let path = backend.get_media_path("../etc/passwd");
        // Should return base_path directly (rejected), not include the media_id
        assert_eq!(path, std::path::PathBuf::from("/tmp/media_test"));
    }

    #[test]
    fn test_get_media_path_rejects_forward_slash() {
        let backend = make_backend("/tmp/media_test", 0);
        let path = backend.get_media_path("a/b/c");
        assert_eq!(path, std::path::PathBuf::from("/tmp/media_test"));
    }

    #[test]
    fn test_get_media_path_rejects_backslash() {
        let backend = make_backend("/tmp/media_test", 0);
        let path = backend.get_media_path("a\\b\\c");
        assert_eq!(path, std::path::PathBuf::from("/tmp/media_test"));
    }

    #[test]
    fn test_get_media_path_with_depth_sharding() {
        let backend = make_backend("/tmp/media_test", 2);
        let path = backend.get_media_path("abcde");
        // With max_path_depth=2, path should be /tmp/media_test/a/b/abcde
        let mut expected = std::path::PathBuf::from("/tmp/media_test");
        expected.push("a");
        expected.push("b");
        expected.push("abcde");
        assert_eq!(path, expected);
    }

    #[test]
    fn test_get_media_path_depth_exceeds_id_length() {
        let backend = make_backend("/tmp/media_test", 10);
        let path = backend.get_media_path("ab");
        // max_path_depth=10 but media_id has only 2 chars; should shard only 2 levels
        let mut expected = std::path::PathBuf::from("/tmp/media_test");
        expected.push("a");
        expected.push("b");
        expected.push("ab");
        assert_eq!(path, expected);
    }

    #[test]
    fn test_get_media_path_empty_id() {
        let backend = make_backend("/tmp/media_test", 0);
        let path = backend.get_media_path("");
        // Empty media_id: no traversal chars, but path is just base_path
        assert_eq!(path, std::path::PathBuf::from("/tmp/media_test"));
    }

    #[test]
    fn test_get_thumbnail_path_rejects_traversal() {
        let backend = make_backend("/tmp/media_test", 0);
        let path = backend.get_thumbnail_path("../evil", 100, 100, "scale");
        // Should return thumbnail_path directly (rejected)
        assert_eq!(path, std::path::PathBuf::from("/tmp/media_test/thumbnails"));
    }

    #[test]
    fn test_get_thumbnail_path_normal() {
        let backend = make_backend("/tmp/media_test", 0);
        let path = backend.get_thumbnail_path("media123", 200, 150, "crop");
        let expected = std::path::PathBuf::from("/tmp/media_test/thumbnails/media123_200x150_crop.jpg");
        assert_eq!(path, expected);
    }

    #[test]
    fn test_new_creates_backend_with_correct_fields() {
        let backend = make_backend("/tmp/media_new_test", 3);
        assert_eq!(backend.base_path, std::path::PathBuf::from("/tmp/media_new_test"));
        assert_eq!(backend.thumbnail_path, std::path::PathBuf::from("/tmp/media_new_test/thumbnails"));
        assert_eq!(backend.max_path_depth, 3);
    }
}
