use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 媒体元数据模型 — 用于 API 序列化/反序列化，不直接映射数据库。
///
/// 时间字段说明：
/// - `created_at: DateTime<Utc>`：使用 chrono 的 `DateTime<Utc>` 类型，
///   由 serde 自动序列化为 ISO 8601 格式（如 `"2025-01-15T10:30:00Z"`），
///   符合 Matrix 规范对 API 响应时间格式的要求。
///   此处不映射数据库列，故无需使用 i64 时间戳或 `#[sqlx(rename)]`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub media_id: String,
    pub server_name: String,
    pub content_type: String,
    pub file_name: Option<String>,
    pub size: i64,
    pub uploader_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub quarantine_status: Option<String>,
}

/// 缩略图元数据模型 — 用于 API 序列化/反序列化，不直接映射数据库。
///
/// 时间字段说明：
/// - `created_at: DateTime<Utc>`：同上，serde 自动序列化为 ISO 8601 格式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailMetadata {
    pub media_id: String,
    pub width: i32,
    pub height: i32,
    pub method: String,
    pub content_type: String,
    pub size: i64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    #[serde(rename = "quarantined")]
    pub is_quarantined: bool,
    pub reason: String,
    pub quarantined_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_media_metadata_iso8601_serialization() {
        let created = Utc.with_ymd_and_hms(2025, 1, 15, 10, 30, 0).unwrap();
        let metadata = MediaMetadata {
            media_id: "abc123".to_string(),
            server_name: "example.com".to_string(),
            content_type: "image/png".to_string(),
            file_name: Some("test.png".to_string()),
            size: 1024,
            uploader_user_id: Some("@user:example.com".to_string()),
            created_at: created,
            last_accessed_at: None,
            quarantine_status: None,
        };

        let json = serde_json::to_string(&metadata).unwrap();

        assert!(
            json.contains("\"created_at\":\"2025-01-15T10:30:00Z\""),
            "created_at should serialize as ISO 8601 format, got: {}",
            json
        );

        let deserialized: MediaMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.created_at, created);
    }

    #[test]
    fn test_media_metadata_iso8601_deserialization() {
        let json = r#"{
            "media_id": "xyz789",
            "server_name": "matrix.org",
            "content_type": "video/mp4",
            "file_name": "video.mp4",
            "size": 2048000,
            "uploader_user_id": "@alice:matrix.org",
            "created_at": "2025-06-01T08:00:00Z",
            "last_accessed_at": null,
            "quarantine_status": null
        }"#;

        let metadata: MediaMetadata = serde_json::from_str(json).unwrap();

        assert_eq!(metadata.media_id, "xyz789");
        let expected = Utc.with_ymd_and_hms(2025, 6, 1, 8, 0, 0).unwrap();
        assert_eq!(metadata.created_at, expected);
    }

    #[test]
    fn test_thumbnail_metadata_iso8601_serialization() {
        let created = Utc.with_ymd_and_hms(2025, 3, 20, 14, 45, 30).unwrap();
        let thumbnail = ThumbnailMetadata {
            media_id: "thumb001".to_string(),
            width: 128,
            height: 128,
            method: "crop".to_string(),
            content_type: "image/jpeg".to_string(),
            size: 20480,
            created_at: created,
        };

        let json = serde_json::to_string(&thumbnail).unwrap();

        assert!(
            json.contains("\"created_at\":\"2025-03-20T14:45:30Z\""),
            "created_at should serialize as ISO 8601 format, got: {}",
            json
        );

        let deserialized: ThumbnailMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.created_at, created);
    }

    #[test]
    fn test_media_metadata_iso8601_with_milliseconds() {
        let json = r#"{
            "media_id": "ms001",
            "server_name": "test.local",
            "content_type": "application/pdf",
            "file_name": null,
            "size": 512000,
            "uploader_user_id": null,
            "created_at": "2025-12-31T23:59:59.999Z",
            "last_accessed_at": null,
            "quarantine_status": null
        }"#;

        let metadata: MediaMetadata = serde_json::from_str(json).unwrap();

        let expected = Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap()
            + chrono::Duration::milliseconds(999);
        assert_eq!(metadata.created_at, expected);
    }
}
