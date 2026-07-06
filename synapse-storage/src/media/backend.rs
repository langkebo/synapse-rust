use super::models::*;
use async_trait::async_trait;
use synapse_common::error::ApiError;

#[async_trait]
pub trait MediaStorageBackend: Send + Sync {
    async fn store(&self, media_id: &str, data: &[u8], content_type: &str) -> Result<(), ApiError>;
    async fn retrieve(&self, media_id: &str) -> Result<Option<Vec<u8>>, ApiError>;
    async fn delete(&self, media_id: &str) -> Result<bool, ApiError>;
    async fn exists(&self, media_id: &str) -> Result<bool, ApiError>;
    async fn get_size(&self, media_id: &str) -> Result<Option<u64>, ApiError>;
    async fn store_thumbnail(
        &self,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
        data: &[u8],
    ) -> Result<(), ApiError>;
    async fn retrieve_thumbnail(
        &self,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<Option<Vec<u8>>, ApiError>;
    async fn delete_thumbnails(&self, media_id: &str) -> Result<u64, ApiError>;
    async fn get_stats(&self) -> Result<MediaStorageStats, ApiError>;
    async fn health_check(&self) -> Result<bool, ApiError>;
}

pub struct MediaStorageBackendFactory;

impl MediaStorageBackendFactory {
    pub fn create(config: &StorageBackendConfig) -> Result<Box<dyn MediaStorageBackend>, ApiError> {
        match config.backend_type {
            StorageBackendType::Filesystem => {
                let fs_config = config.filesystem.clone().unwrap_or_default();
                Ok(Box::new(crate::media::filesystem::FilesystemBackend::new(&fs_config)?))
            }
            StorageBackendType::S3 => {
                let s3_config = config
                    .s3
                    .clone()
                    .ok_or_else(|| ApiError::internal("S3 configuration required for S3 backend".to_string()))?;
                Ok(Box::new(crate::media::s3::S3Backend::new(s3_config)))
            }
            StorageBackendType::Memory => Ok(Box::new(MemoryBackend::new())),
            _ => Err(ApiError::internal_with_log("Unsupported storage backend", &format!("{:?}", config.backend_type))),
        }
    }
}

pub struct MemoryBackend {
    storage: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>>,
    thumbnails: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            storage: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            thumbnails: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    fn thumbnail_key(media_id: &str, width: u32, height: u32, method: &str) -> String {
        format!("{media_id}_{width}x{height}_{method}")
    }
}

#[async_trait]
impl MediaStorageBackend for MemoryBackend {
    async fn store(&self, media_id: &str, data: &[u8], _content_type: &str) -> Result<(), ApiError> {
        let mut storage = self.storage.write().await;
        storage.insert(media_id.to_string(), data.to_vec());
        Ok(())
    }

    async fn retrieve(&self, media_id: &str) -> Result<Option<Vec<u8>>, ApiError> {
        let storage = self.storage.read().await;
        Ok(storage.get(media_id).cloned())
    }

    async fn delete(&self, media_id: &str) -> Result<bool, ApiError> {
        let mut storage = self.storage.write().await;
        Ok(storage.remove(media_id).is_some())
    }

    async fn exists(&self, media_id: &str) -> Result<bool, ApiError> {
        let storage = self.storage.read().await;
        Ok(storage.contains_key(media_id))
    }

    async fn get_size(&self, media_id: &str) -> Result<Option<u64>, ApiError> {
        let storage = self.storage.read().await;
        Ok(storage.get(media_id).map(|v| v.len() as u64))
    }

    async fn store_thumbnail(
        &self,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
        data: &[u8],
    ) -> Result<(), ApiError> {
        let key = Self::thumbnail_key(media_id, width, height, method);
        let mut thumbnails = self.thumbnails.write().await;
        thumbnails.insert(key, data.to_vec());
        Ok(())
    }

    async fn retrieve_thumbnail(
        &self,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<Option<Vec<u8>>, ApiError> {
        let key = Self::thumbnail_key(media_id, width, height, method);
        let thumbnails = self.thumbnails.read().await;
        Ok(thumbnails.get(&key).cloned())
    }

    async fn delete_thumbnails(&self, media_id: &str) -> Result<u64, ApiError> {
        let mut thumbnails = self.thumbnails.write().await;
        let keys_to_remove: Vec<String> = thumbnails.keys().filter(|k| k.starts_with(media_id)).cloned().collect();
        let count = keys_to_remove.len() as u64;
        for key in keys_to_remove {
            thumbnails.remove(&key);
        }
        Ok(count)
    }

    async fn get_stats(&self) -> Result<MediaStorageStats, ApiError> {
        let storage = self.storage.read().await;
        let total_files = storage.len() as u64;
        let total_size: u64 = storage.values().map(|v| v.len() as u64).sum();

        Ok(MediaStorageStats {
            total_files,
            total_size,
            by_content_type: std::collections::HashMap::new(),
            oldest_file: None,
            newest_file: None,
        })
    }

    async fn health_check(&self) -> Result<bool, ApiError> {
        Ok(true)
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_store_and_retrieve() {
        let backend = MemoryBackend::new();
        backend.store("media-1", b"hello world", "text/plain").await.unwrap();
        let data = backend.retrieve("media-1").await.unwrap();
        assert_eq!(data.as_deref(), Some(&b"hello world"[..]));
    }

    #[tokio::test]
    async fn memory_retrieve_nonexistent() {
        let backend = MemoryBackend::new();
        let data = backend.retrieve("nonexistent").await.unwrap();
        assert!(data.is_none());
    }

    #[tokio::test]
    async fn memory_delete_returns_true_if_existed() {
        let backend = MemoryBackend::new();
        backend.store("media-1", b"data", "text/plain").await.unwrap();
        assert!(backend.delete("media-1").await.unwrap());
        assert!(!backend.delete("media-1").await.unwrap());
    }

    #[tokio::test]
    async fn memory_exists_check() {
        let backend = MemoryBackend::new();
        assert!(!backend.exists("media-1").await.unwrap());
        backend.store("media-1", b"data", "text/plain").await.unwrap();
        assert!(backend.exists("media-1").await.unwrap());
    }

    #[tokio::test]
    async fn memory_get_size() {
        let backend = MemoryBackend::new();
        backend.store("media-1", b"1234567890", "text/plain").await.unwrap();
        assert_eq!(backend.get_size("media-1").await.unwrap(), Some(10));
        assert_eq!(backend.get_size("nonexistent").await.unwrap(), None);
    }

    #[tokio::test]
    async fn memory_thumbnail_store_and_retrieve() {
        let backend = MemoryBackend::new();
        backend.store_thumbnail("media-1", 100, 100, "crop", b"thumb").await.unwrap();
        let thumb = backend.retrieve_thumbnail("media-1", 100, 100, "crop").await.unwrap();
        assert_eq!(thumb.as_deref(), Some(&b"thumb"[..]));
    }

    #[tokio::test]
    async fn memory_thumbnail_key_isolation() {
        let backend = MemoryBackend::new();
        backend.store_thumbnail("media-1", 100, 100, "crop", b"small").await.unwrap();
        let thumb = backend.retrieve_thumbnail("media-1", 200, 200, "scale").await.unwrap();
        assert!(thumb.is_none());
    }

    #[tokio::test]
    async fn memory_delete_thumbnails_removes_all_for_media() {
        let backend = MemoryBackend::new();
        backend.store_thumbnail("media-1", 100, 100, "crop", b"a").await.unwrap();
        backend.store_thumbnail("media-1", 200, 200, "scale", b"b").await.unwrap();
        let deleted = backend.delete_thumbnails("media-1").await.unwrap();
        assert_eq!(deleted, 2);
    }

    #[tokio::test]
    async fn memory_get_stats_counts_correctly() {
        let backend = MemoryBackend::new();
        backend.store("a", b"12345", "text/plain").await.unwrap();
        backend.store("b", b"1234567890", "image/png").await.unwrap();
        let stats = backend.get_stats().await.unwrap();
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.total_size, 15);
    }

    #[tokio::test]
    async fn memory_health_check() {
        let backend = MemoryBackend::new();
        assert!(backend.health_check().await.unwrap());
    }
}
