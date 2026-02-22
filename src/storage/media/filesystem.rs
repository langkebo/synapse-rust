use super::backend::MediaStorageBackend;
use super::models::*;
use crate::error::ApiError;
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

pub struct FilesystemBackend {
    base_path: PathBuf,
    thumbnail_path: PathBuf,
    max_path_depth: u32,
}

impl FilesystemBackend {
    pub fn new(config: FilesystemConfig) -> Result<Self, ApiError> {
        let base_path = PathBuf::from(&config.storage_path);
        let thumbnail_path = base_path.join("thumbnails");

        if config.create_directories {
            std::fs::create_dir_all(&base_path).map_err(|e| {
                ApiError::internal(format!("Failed to create media directory: {}", e))
            })?;
            std::fs::create_dir_all(&thumbnail_path).map_err(|e| {
                ApiError::internal(format!("Failed to create thumbnail directory: {}", e))
            })?;
        }

        Ok(Self {
            base_path,
            thumbnail_path,
            max_path_depth: config.max_path_depth,
        })
    }

    fn get_media_path(&self, media_id: &str) -> PathBuf {
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
        let filename = format!("{}_{}x{}_{}.jpg", media_id, width, height, method);
        self.thumbnail_path.join(filename)
    }
}

#[async_trait]
impl MediaStorageBackend for FilesystemBackend {
    async fn store(&self, media_id: &str, data: &[u8], _content_type: &str) -> Result<(), ApiError> {
        let path = self.get_media_path(media_id);
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                ApiError::internal(format!("Failed to create directory: {}", e))
            })?;
        }

        let mut file = fs::File::create(&path).await.map_err(|e| {
            ApiError::internal(format!("Failed to create file: {}", e))
        })?;

        file.write_all(data).await.map_err(|e| {
            ApiError::internal(format!("Failed to write file: {}", e))
        })?;

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
            Err(e) => return Err(ApiError::internal(format!("Failed to open file: {}", e))),
        };

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await.map_err(|e| {
            ApiError::internal(format!("Failed to read file: {}", e))
        })?;

        Ok(Some(buffer))
    }

    async fn delete(&self, media_id: &str) -> Result<bool, ApiError> {
        let path = self.get_media_path(media_id);
        
        if !path.exists() {
            return Ok(false);
        }

        fs::remove_file(&path).await.map_err(|e| {
            ApiError::internal(format!("Failed to delete file: {}", e))
        })?;

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

        let metadata = fs::metadata(&path).await.map_err(|e| {
            ApiError::internal(format!("Failed to get file metadata: {}", e))
        })?;

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
        
        fs::create_dir_all(&self.thumbnail_path).await.map_err(|e| {
            ApiError::internal(format!("Failed to create thumbnail directory: {}", e))
        })?;

        let mut file = fs::File::create(&path).await.map_err(|e| {
            ApiError::internal(format!("Failed to create thumbnail file: {}", e))
        })?;

        file.write_all(data).await.map_err(|e| {
            ApiError::internal(format!("Failed to write thumbnail: {}", e))
        })?;

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
            Err(e) => return Err(ApiError::internal(format!("Failed to open thumbnail: {}", e))),
        };

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await.map_err(|e| {
            ApiError::internal(format!("Failed to read thumbnail: {}", e))
        })?;

        Ok(Some(buffer))
    }

    async fn delete_thumbnails(&self, media_id: &str) -> Result<u64, ApiError> {
        let mut count = 0u64;
        
        let mut entries = match fs::read_dir(&self.thumbnail_path).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(ApiError::internal(format!("Failed to read thumbnail directory: {}", e))),
        };

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            ApiError::internal(format!("Failed to read directory entry: {}", e))
        })? {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            
            if name.starts_with(media_id)
                && fs::remove_file(entry.path()).await.is_ok() {
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
            let entries = std::fs::read_dir(path).map_err(|e| {
                ApiError::internal(format!("Failed to read directory: {}", e))
            })?;

            for entry in entries {
                let entry = entry.map_err(|e| {
                    ApiError::internal(format!("Failed to read entry: {}", e))
                })?;

                let path = entry.path();
                if path.is_dir() {
                    process_directory(&path, total_files, total_size, oldest, newest)?;
                } else {
                    let metadata = std::fs::metadata(&path).map_err(|e| {
                        ApiError::internal(format!("Failed to get metadata: {}", e))
                    })?;

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
