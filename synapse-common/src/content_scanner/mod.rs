//! Content Scanner types (MSC3806).
//!
//! This module defines the configuration and request/response types for
//! content scanning services. The actual scanning logic lives in
//! `synapse_services::content_scanner`.

use serde::{Deserialize, Serialize};

/// The `ContentScanResult` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentScanResult {
    /// The `safe` field.
    pub safe: bool,
    /// The `threat_type` field.
    pub threat_type: Option<String>,
    /// The `threat_message` field.
    pub threat_message: Option<String>,
    /// The `scan_timestamp` field.
    pub scan_timestamp: i64,
}

/// The `ScanRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRequest {
    /// The `content_id` field.
    pub content_id: String,
    /// The `content_type` field.
    pub content_type: ContentType,
    /// The `data` field.
    pub data: Vec<u8>,
}

/// The `ContentType` enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    /// The `MediaImage` variant.
    MediaImage,
    /// The `MediaVideo` variant.
    MediaVideo,
    /// The `MediaAudio` variant.
    MediaAudio,
    /// The `MediaFile` variant.
    MediaFile,
    /// The `MessageText` variant.
    MessageText,
    /// The `FileAttachment` variant.
    FileAttachment,
}

impl ContentType {
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MediaImage => "media_image",
            Self::MediaVideo => "media_video",
            Self::MediaAudio => "media_audio",
            Self::MediaFile => "media_file",
            Self::MessageText => "message_text",
            Self::FileAttachment => "file_attachment",
        }
    }
}

impl std::str::FromStr for ContentType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "media_image" => Ok(Self::MediaImage),
            "media_video" => Ok(Self::MediaVideo),
            "media_audio" => Ok(Self::MediaAudio),
            "media_file" => Ok(Self::MediaFile),
            "message_text" => Ok(Self::MessageText),
            "file_attachment" => Ok(Self::FileAttachment),
            _ => Err(()),
        }
    }
}

/// The `ContentScannerConfig` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentScannerConfig {
    /// The `enabled` field.
    pub enabled: bool,
    /// The `scanner_type` field.
    pub scanner_type: ScannerType,
    /// The `clamav_socket_path` field.
    pub clamav_socket_path: Option<String>,
    /// The `clamav_host` field.
    pub clamav_host: Option<String>,
    /// The `clamav_port` field.
    pub clamav_port: Option<u16>,
    /// The `webhook_url` field.
    pub webhook_url: Option<String>,
    /// The `webhook_secret` field.
    pub webhook_secret: Option<String>,
    /// The `allowed_threat_types` field.
    pub allowed_threat_types: Vec<String>,
    /// The `block_on_scan_failure` field.
    pub block_on_scan_failure: bool,
    /// The `scan_timeout_ms` field.
    pub scan_timeout_ms: u64,
}

impl Default for ContentScannerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scanner_type: ScannerType::ClamAv,
            clamav_socket_path: None,
            clamav_host: Some("127.0.0.1".to_string()),
            clamav_port: Some(3310),
            webhook_url: None,
            webhook_secret: None,
            allowed_threat_types: vec![],
            block_on_scan_failure: true,
            scan_timeout_ms: 30000,
        }
    }
}

/// The `ScannerType` enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScannerType {
    #[default]
    /// The `ClamAv` variant.
    ClamAv,
    /// The `Webhook` variant.
    Webhook,
    /// The `Disabled` variant.
    Disabled,
}

/// The `WebhookScanRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookScanRequest {
    /// The `content_id` field.
    pub content_id: String,
    /// The `content_type` field.
    pub content_type: String,
    /// The `file_name` field.
    pub file_name: Option<String>,
    /// The `file_size` field.
    pub file_size: u64,
    /// The `checksum` field.
    pub checksum: Option<String>,
}

/// The `WebhookScanResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookScanResponse {
    /// The `safe` field.
    pub safe: bool,
    /// The `threat_type` field.
    pub threat_type: Option<String>,
    /// The `threat_message` field.
    pub threat_message: Option<String>,
}
