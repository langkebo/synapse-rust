use super::models::*;
use crate::error::ApiError;
use async_trait::async_trait;

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
                Ok(Box::new(
                    crate::storage::media::filesystem::FilesystemBackend::new(fs_config)?,
                ))
            }
            StorageBackendType::S3 => {
                let s3_config = config.s3.clone().ok_or_else(|| {
                    ApiError::internal("S3 configuration required for S3 backend".to_string())
                })?;
                Ok(Box::new(crate::storage::media::s3::S3Backend::new(
                    s3_config,
                )))
            }
            StorageBackendType::Memory => Ok(Box::new(MemoryBackend::new())),
            _ => Err(ApiError::internal(format!(
                "Unsupported storage backend: {:?}",
                config.backend_type
            ))),
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
            storage: std::sync::Arc::new(
                tokio::sync::RwLock::new(std::collections::HashMap::new()),
            ),
            thumbnails: std::sync::Arc::new(tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    fn thumbnail_key(media_id: &str, width: u32, height: u32, method: &str) -> String {
        format!("{}_{}x{}_{}", media_id, width, height, method)
    }
}

#[async_trait]
impl MediaStorageBackend for MemoryBackend {
    async fn store(
        &self,
        media_id: &str,
        data: &[u8],
        _content_type: &str,
    ) -> Result<(), ApiError> {
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
        let keys_to_remove: Vec<String> = thumbnails
            .keys()
            .filter(|k| k.starts_with(media_id))
            .cloned()
            .collect();
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
