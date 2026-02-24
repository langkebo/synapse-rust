use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentScanResult {
    pub safe: bool,
    pub threat_type: Option<String>,
    pub threat_message: Option<String>,
    pub scan_timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRequest {
    pub content_id: String,
    pub content_type: ContentType,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    MediaImage,
    MediaVideo,
    MediaAudio,
    MediaFile,
    MessageText,
    FileAttachment,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::MediaImage => "media_image",
            ContentType::MediaVideo => "media_video",
            ContentType::MediaAudio => "media_audio",
            ContentType::MediaFile => "media_file",
            ContentType::MessageText => "message_text",
            ContentType::FileAttachment => "file_attachment",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "media_image" => Some(ContentType::MediaImage),
            "media_video" => Some(ContentType::MediaVideo),
            "media_audio" => Some(ContentType::MediaAudio),
            "media_file" => Some(ContentType::MediaFile),
            "message_text" => Some(ContentType::MessageText),
            "file_attachment" => Some(ContentType::FileAttachment),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentScannerConfig {
    pub enabled: bool,
    pub scanner_type: ScannerType,
    pub clamav_socket_path: Option<String>,
    pub clamav_host: Option<String>,
    pub clamav_port: Option<u16>,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub allowed_threat_types: Vec<String>,
    pub block_on_scan_failure: bool,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScannerType {
    #[default]
    ClamAv,
    Webhook,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookScanRequest {
    pub content_id: String,
    pub content_type: String,
    pub file_name: Option<String>,
    pub file_size: u64,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookScanResponse {
    pub safe: bool,
    pub threat_type: Option<String>,
    pub threat_message: Option<String>,
}
