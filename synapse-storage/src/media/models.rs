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
    /// The `media_id` field.
    pub media_id: String,
    /// The `server_name` field.
    pub server_name: String,
    /// The `content_type` field.
    pub content_type: String,
    /// The `file_name` field.
    pub file_name: Option<String>,
    /// The `size` field.
    pub size: i64,
    /// The `uploader_user_id` field.
    pub uploader_user_id: Option<String>,
    /// The `created_at` field.
    pub created_at: DateTime<Utc>,
    /// The `last_accessed_at` field.
    pub last_accessed_at: Option<DateTime<Utc>>,
    /// The `quarantine_status` field.
    pub quarantine_status: Option<String>,
}

/// 缩略图元数据模型 — 用于 API 序列化/反序列化，不直接映射数据库。
///
/// 时间字段说明：
/// - `created_at: DateTime<Utc>`：同上，serde 自动序列化为 ISO 8601 格式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailMetadata {
    /// The `media_id` field.
    pub media_id: String,
    /// The `width` field.
    pub width: i32,
    /// The `height` field.
    pub height: i32,
    /// The `method` field.
    pub method: String,
    /// The `content_type` field.
    pub content_type: String,
    /// The `size` field.
    pub size: i64,
    /// The `created_at` field.
    pub created_at: DateTime<Utc>,
}

/// The `MediaUploadRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaUploadRequest {
    /// The `content_type` field.
    pub content_type: String,
    /// The `file_name` field.
    pub file_name: Option<String>,
    /// The `uploader_user_id` field.
    pub uploader_user_id: Option<String>,
}

/// The `MediaUploadResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaUploadResponse {
    /// The `content_uri` field.
    pub content_uri: String,
    /// The `media_id` field.
    pub media_id: String,
}

/// The `StorageBackendType` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StorageBackendType {
    #[default]
    /// The `Filesystem` variant.
    Filesystem,
    /// The `S3` variant.
    S3,
    /// The `Azure` variant.
    Azure,
    /// The `GCS` variant.
    GCS,
    /// The `Memory` variant.
    Memory,
}

/// The `StorageBackendConfig` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageBackendConfig {
    /// The `backend_type` field.
    pub backend_type: StorageBackendType,
    /// The `filesystem` field.
    pub filesystem: Option<FilesystemConfig>,
    /// The `s3` field.
    pub s3: Option<S3Config>,
    /// The `azure` field.
    pub azure: Option<AzureConfig>,
    /// The `gcs` field.
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

/// The `FilesystemConfig` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    /// The `storage_path` field.
    pub storage_path: String,
    /// The `create_directories` field.
    pub create_directories: bool,
    /// The `max_path_depth` field.
    pub max_path_depth: u32,
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self { storage_path: "./media".to_string(), create_directories: true, max_path_depth: 2 }
    }
}

/// The `S3Config` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    /// The `bucket` field.
    pub bucket: String,
    /// The `region` field.
    pub region: String,
    /// The `endpoint_url` field.
    pub endpoint_url: Option<String>,
    /// The `access_key_id` field.
    pub access_key_id: String,
    /// The `secret_access_key` field.
    pub secret_access_key: String,
    /// The `prefix` field.
    pub prefix: Option<String>,
    /// The `use_path_style` field.
    pub use_path_style: bool,
}

/// The `AzureConfig` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureConfig {
    /// The `account_name` field.
    pub account_name: String,
    /// The `account_key` field.
    pub account_key: String,
    /// The `container` field.
    pub container: String,
    /// The `endpoint_url` field.
    pub endpoint_url: Option<String>,
}

/// The `GCSConfig` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCSConfig {
    /// The `bucket` field.
    pub bucket: String,
    /// The `credentials_json` field.
    pub credentials_json: String,
    /// The `endpoint_url` field.
    pub endpoint_url: Option<String>,
}

/// The `MediaStorageStats` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaStorageStats {
    /// The `total_files` field.
    pub total_files: u64,
    /// The `total_size` field.
    pub total_size: u64,
    /// The `by_content_type` field.
    pub by_content_type: std::collections::HashMap<String, u64>,
    /// The `oldest_file` field.
    pub oldest_file: Option<DateTime<Utc>>,
    /// The `newest_file` field.
    pub newest_file: Option<DateTime<Utc>>,
}

/// The `MediaQuarantineRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaQuarantineRequest {
    /// The `media_id` field.
    pub media_id: String,
    /// The `reason` field.
    pub reason: String,
    /// The `quarantined_by` field.
    pub quarantined_by: String,
}

/// The `MediaQuarantineResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaQuarantineResponse {
    /// The `media_id` field.
    pub media_id: String,
    #[serde(rename = "quarantined")]
    /// The `is_quarantined` field.
    pub is_quarantined: bool,
    /// The `reason` field.
    pub reason: String,
    /// The `quarantined_at` field.
    pub quarantined_at: DateTime<Utc>,
}

/// 媒体隔离变更记录 — 直接映射 quarantined_media_changes 表。
///
/// 用于 stream writer 在多 worker 部署下同步媒体隔离状态变更。
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuarantinedMediaChange {
    /// The `stream_id` field.
    pub stream_id: i64,
    /// The `media_id` field.
    pub media_id: String,
    /// The `server_name` field.
    pub server_name: String,
    /// The `change_type` field.
    pub change_type: String,
    /// The `changed_by` field.
    pub changed_by: String,
    /// The `created_ts` field.
    pub created_ts: i64,
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
            "created_at should serialize as ISO 8601 format, got: {json}"
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
            "created_at should serialize as ISO 8601 format, got: {json}"
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

        let expected = Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap() + chrono::Duration::milliseconds(999);
        assert_eq!(metadata.created_at, expected);
    }
}
