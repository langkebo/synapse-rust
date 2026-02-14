use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub media_id: String,
    pub server_name: String,
    pub content_type: String,
    pub file_name: Option<String>,
    pub size: u64,
    pub uploader_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub quarantine_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailMetadata {
    pub media_id: String,
    pub width: u32,
    pub height: u32,
    pub method: String,
    pub content_type: String,
    pub size: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaUploadRequest {
    pub content_type: String,
    pub file_name: Option<String>,
    pub uploader_user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaUploadResponse {
    pub content_uri: String,
    pub media_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum StorageBackendType {
    #[default]
    Filesystem,
    S3,
    Azure,
    GCS,
    Memory,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageBackendConfig {
    pub backend_type: StorageBackendType,
    pub filesystem: Option<FilesystemConfig>,
    pub s3: Option<S3Config>,
    pub azure: Option<AzureConfig>,
    pub gcs: Option<GCSConfig>,
}

impl Default for StorageBackendConfig {
    fn default() -> Self {
        Self {
            backend_type: StorageBackendType::Filesystem,
            filesystem: Some(FilesystemConfig::default()),
            s3: None,
            azure: None,
            gcs: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    pub storage_path: String,
    pub create_directories: bool,
    pub max_path_depth: u32,
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            storage_path: "./media".to_string(),
            create_directories: true,
            max_path_depth: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub endpoint_url: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub prefix: Option<String>,
    pub use_path_style: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureConfig {
    pub account_name: String,
    pub account_key: String,
    pub container: String,
    pub endpoint_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCSConfig {
    pub bucket: String,
    pub credentials_json: String,
    pub endpoint_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaStorageStats {
    pub total_files: u64,
    pub total_size: u64,
    pub by_content_type: std::collections::HashMap<String, u64>,
    pub oldest_file: Option<DateTime<Utc>>,
    pub newest_file: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaQuarantineRequest {
    pub media_id: String,
    pub reason: String,
    pub quarantined_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaQuarantineResponse {
    pub media_id: String,
    pub quarantined: bool,
    pub reason: String,
    pub quarantined_at: DateTime<Utc>,
}
